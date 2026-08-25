//! Executable pages for code that came from a file
//!
//! Every relocation the compiler emits is a call from one guest function to
//! another, and both live in the same `.text`. The distance between them is
//! therefore fixed when the code is laid out, and patching it needs no load
//! address -- which is what makes an artifact runnable wherever it lands.

use anyhow::{Result, bail};
use std::{ffi::c_void, ptr};

unsafe extern "C" {
    fn rvtime_flush_icache(start: *mut i8, len: u64);
}

/// One call site waiting for the distance to its callee.
pub(crate) struct Patch {
    /// Where in `.text` the instruction sits.
    pub at: u64,

    /// Where in `.text` the callee starts.
    pub target: u64,

    /// The correction the object format supplies alongside the relocation.
    ///
    /// ELF carries it in the relocation; Mach-O leaves it in the instruction
    /// field instead, which [`Patch::implicit`] marks. Applying the wrong one
    /// lands on a real function rather than faulting, so the two cases are kept
    /// apart rather than guessed at.
    pub addend: i64,

    /// Whether `addend` still has to be read out of the instruction.
    pub implicit: bool,
}

/// Code mapped for execution, owned for as long as anything points into it.
pub(crate) struct Mapping {
    base: *mut u8,
    len: usize,
}

impl Mapping {
    /// Map `code` writable and copy it in. Not executable until [`Self::protect`].
    pub fn new(code: &[u8]) -> Result<Self> {
        if code.is_empty() {
            bail!("the artifact contains no code");
        }

        let base = unsafe {
            libc::mmap(
                ptr::null_mut(),
                code.len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            bail!(
                "failed to map {} bytes for compiled code: {}",
                code.len(),
                std::io::Error::last_os_error()
            );
        }

        let mapping = Mapping {
            base: base as *mut u8,
            len: code.len(),
        };
        unsafe { ptr::copy_nonoverlapping(code.as_ptr(), mapping.base, code.len()) };
        Ok(mapping)
    }

    /// Where the mapped code starts.
    pub fn base(&self) -> *const u8 {
        self.base
    }

    /// Resolve every call site. Must happen while the mapping is still writable.
    pub fn relocate(&mut self, patches: &[Patch]) -> Result<()> {
        let code = unsafe { std::slice::from_raw_parts_mut(self.base, self.len) };
        for patch in patches {
            apply(code, patch)?;
        }
        Ok(())
    }

    /// Make the code executable and visible to the instruction fetcher.
    pub fn protect(&self) -> Result<()> {
        if unsafe {
            libc::mprotect(
                self.base as *mut c_void,
                self.len,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        } != 0
        {
            bail!(
                "failed to make compiled code executable: {}",
                std::io::Error::last_os_error()
            );
        }

        unsafe { rvtime_flush_icache(self.base as *mut i8, self.len as u64) };
        Ok(())
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.base as *mut c_void, self.len) };
    }
}

// The mapping is written once, before anything can reach it, and is read-only
// thereafter. `Module` owns it and hands out only the pointers into it.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}

/// Read the `len`-byte little-endian word a patch targets.
fn read(code: &[u8], at: u64, len: usize) -> Result<u64> {
    let start = at as usize;
    let end = start
        .checked_add(len)
        .filter(|end| *end <= code.len())
        .ok_or_else(|| anyhow::anyhow!("relocation at {at:#x} falls outside the code"))?;

    let mut bytes = [0u8; 8];
    bytes[..len].copy_from_slice(&code[start..end]);
    Ok(u64::from_le_bytes(bytes))
}

fn write(code: &mut [u8], at: u64, len: usize, value: u64) {
    let start = at as usize;
    code[start..start + len].copy_from_slice(&value.to_le_bytes()[..len]);
}

/// Patch one call site with the distance to its callee.
///
/// The target triple is part of the artifact fingerprint, so an artifact only
/// reaches here on the architecture it was built for.
#[cfg(target_arch = "aarch64")]
fn apply(code: &mut [u8], patch: &Patch) -> Result<()> {
    const MASK: u64 = 0x03ff_ffff;

    let word = read(code, patch.at, 4)?;
    let addend = if patch.implicit {
        // A signed 26-bit instruction count, so sign-extend before scaling.
        (((word & MASK) as i32) << 6 >> 6) as i64 * 4
    } else {
        patch.addend
    };

    let distance = (patch.target as i64) + addend - (patch.at as i64);
    if distance % 4 != 0 {
        bail!("call at {:#x} is not instruction aligned", patch.at);
    }
    if !(-(1 << 27)..(1 << 27)).contains(&distance) {
        bail!(
            "call at {:#x} reaches {distance:#x} bytes, past the ±128 MiB a branch encodes",
            patch.at
        );
    }

    let imm = ((distance / 4) as u64) & MASK;
    write(code, patch.at, 4, (word & !MASK) | imm);
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn apply(code: &mut [u8], patch: &Patch) -> Result<()> {
    let word = read(code, patch.at, 4)?;
    let addend = if patch.implicit {
        word as u32 as i32 as i64
    } else {
        patch.addend
    };

    let distance = (patch.target as i64) + addend - (patch.at as i64);
    if !(-(1 << 31)..(1 << 31)).contains(&distance) {
        bail!(
            "call at {:#x} reaches {distance:#x} bytes, past the ±2 GiB a branch encodes",
            patch.at
        );
    }

    write(code, patch.at, 4, distance as u32 as u64);
    Ok(())
}
