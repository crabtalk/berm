//! Guest memory
//!
//! The whole 4 GiB guest address space is reserved in one `PROT_NONE` mapping
//! and segments are committed into it, so a guest address is just an offset
//! from the base. Everything not committed is a guard page, which is what
//! makes bounds checking free: the translator masks an address to 32 bits and
//! adds the base, and any access outside the committed region faults into
//! [`crate::trap`].
//!
//! Guest code is mapped read-only, never executable. The guest's instructions
//! are compiled, not run in place; the only reason to map `.text` at all is
//! that programs read constants out of it.
//!
//! ## Protection granularity
//!
//! Permissions apply at *host* page granularity, which is not the guest's
//! 4 KiB page. macOS on arm64 uses 16 KiB pages, and a typical RISC-V image
//! puts its read-only, executable, and writable segments 4 KiB apart -- all
//! three land in one host page. Where segments share a host page the page gets
//! the union of their permissions, because the alternative is for the last one
//! written to silently strip rights from the others.
//!
//! This only weakens the guest against itself. The sandbox boundary is the
//! 4 GiB window, and that is unaffected.

use anyhow::{Context, Result, bail};
use rv::{Perms, Program};
use std::{collections::BTreeMap, ops::Range, ptr, sync::OnceLock};

/// The host page size, which is the granularity `mprotect` accepts.
///
/// Public because it constrains the caller: [`Memory::new`] requires a stack
/// size that is a whole number of these, and it is not the guest's 4 KiB page.
pub fn host_page() -> u64 {
    static SIZE: OnceLock<u64> = OnceLock::new();
    *SIZE.get_or_init(|| {
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        assert!(size > 0, "sysconf(_SC_PAGESIZE) returned {size}");
        size as u64
    })
}

/// The reserved guest address space.
pub struct Memory {
    base: *mut u8,
    size: u64,
    stack: Range<u64>,
}

impl Memory {
    /// Reserve `size` bytes of address space and commit `program` into it.
    ///
    /// The stack occupies the top `stack_size` bytes. Everything below it is
    /// left uncommitted, so an overflow faults rather than running into
    /// whatever is mapped underneath.
    ///
    /// `size` must be a power of two: guest addresses are confined by masking
    /// with `size - 1`, and any other size would leave part of the mask's range
    /// outside the reservation.
    pub fn new(program: &Program, size: u64, stack_size: u64) -> Result<Self> {
        rv::check_memory_size(size)?;

        let page = host_page();
        if stack_size == 0 || !stack_size.is_multiple_of(page) {
            bail!("stack size {stack_size:#x} must be a non-zero multiple of the host page size {page:#x}");
        }
        if stack_size >= size {
            bail!("stack size {stack_size:#x} does not fit in a {size:#x} address space");
        }

        // The image and the stack must not overlap, or committing the stack
        // would silently reopen pages the image expects to be protected.
        let image_end = program.image_end();
        if image_end > size - stack_size {
            bail!(
                "the guest image ends at {image_end:#x}, which leaves no room for a \
                 {stack_size:#x} stack in a {size:#x} address space; raise Config::memory_size"
            );
        }

        let base = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size as usize,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            bail!(
                "failed to reserve {size:#x} bytes of guest address space: {}",
                std::io::Error::last_os_error()
            );
        }

        let memory = Memory {
            base: base as *mut u8,
            size,
            stack: (size - stack_size)..size,
        };
        memory.commit(program)?;
        Ok(memory)
    }

    /// Base of the reserved region. Guest address `a` lives at `base + a`.
    pub fn base(&self) -> *mut u8 {
        self.base
    }

    /// Size of the reserved region.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The mask that confines a guest address to this region.
    pub fn mask(&self) -> u64 {
        self.size - 1
    }

    /// The initial stack pointer: the top of the stack, 16-byte aligned as the
    /// RISC-V ABI requires.
    pub fn stack_pointer(&self) -> u64 {
        self.stack.end - 16
    }

    /// Copy `data` to guest address `addr`.
    pub fn write(&mut self, addr: u64, data: &[u8]) -> Result<()> {
        self.bounds(addr, data.len() as u64)?;
        unsafe { ptr::copy_nonoverlapping(data.as_ptr(), self.base.add(addr as usize), data.len()) };
        Ok(())
    }

    /// Read `len` bytes from guest address `addr`.
    pub fn read(&self, addr: u64, len: u64) -> Result<&[u8]> {
        self.bounds(addr, len)?;
        Ok(unsafe { std::slice::from_raw_parts(self.base.add(addr as usize), len as usize) })
    }

    fn bounds(&self, addr: u64, len: u64) -> Result<()> {
        let end = addr.checked_add(len).context("guest address overflows")?;
        if end > self.size {
            bail!(
                "guest range {addr:#x}..{end:#x} escapes the {:#x} address space",
                self.size
            );
        }
        Ok(())
    }

    /// Commit the program image and the stack.
    fn commit(&self, program: &Program) -> Result<()> {
        let page = host_page();

        // Accumulate permissions per host page before applying anything, so a
        // segment sharing a page with another cannot strip its rights.
        let mut pages: BTreeMap<u64, Perms> = BTreeMap::new();
        for segment in &program.segments {
            let first = segment.addr / page;
            let last = (segment.addr + segment.size).div_ceil(page);
            for index in first..last {
                let entry = pages.entry(index).or_insert(Perms {
                    read: false,
                    write: false,
                    exec: false,
                });
                entry.read |= segment.perms.read;
                entry.write |= segment.perms.write;
                entry.exec |= segment.perms.exec;
            }
        }

        // Open every mapped page for writing, fill it, then lock it down.
        for run in runs(&pages) {
            self.protect(
                run.start * page,
                (run.end - run.start) * page,
                libc::PROT_READ | libc::PROT_WRITE,
            )?;
        }
        for segment in &program.segments {
            unsafe {
                ptr::copy_nonoverlapping(
                    segment.data.as_ptr(),
                    self.base.add(segment.addr as usize),
                    segment.data.len(),
                );
            }
        }
        for (&index, perms) in &pages {
            self.protect(index * page, page, native(*perms))?;
        }

        self.protect(
            self.stack.start,
            self.stack.end - self.stack.start,
            libc::PROT_READ | libc::PROT_WRITE,
        )
    }

    fn protect(&self, addr: u64, len: u64, prot: i32) -> Result<()> {
        let code =
            unsafe { libc::mprotect(self.base.add(addr as usize) as *mut _, len as usize, prot) };
        if code != 0 {
            bail!(
                "failed to protect guest {addr:#x}..{:#x}: {}",
                addr + len,
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }
}

/// Group consecutive page indices into contiguous runs.
fn runs(pages: &BTreeMap<u64, Perms>) -> Vec<Range<u64>> {
    let mut runs: Vec<Range<u64>> = Vec::new();
    for &index in pages.keys() {
        match runs.last_mut() {
            Some(run) if run.end == index => run.end = index + 1,
            _ => runs.push(index..index + 1),
        }
    }
    runs
}

/// Guest permissions as `mprotect` flags.
///
/// The execute bit is dropped: compiled code lives in the JIT's own pages, so
/// guest memory never needs to be executable, and keeping it that way means a
/// guest that corrupts its own image cannot produce host-executable pages.
fn native(perms: Perms) -> i32 {
    let mut prot = 0;
    if perms.read || perms.exec {
        prot |= libc::PROT_READ;
    }
    if perms.write {
        prot |= libc::PROT_WRITE;
    }
    if prot == 0 { libc::PROT_NONE } else { prot }
}

impl Drop for Memory {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.base as *mut _, self.size as usize);
        }
    }
}

// The base pointer is owned exclusively by this `Memory`.
unsafe impl Send for Memory {}
