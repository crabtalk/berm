//! Guest-side support for programs running under rvtime.
//!
//! This crate is compiled *for* RISC-V and linked into the guest, not into the
//! host. It supplies the two things every guest needs and nothing else: a way
//! to reach the host, and an allocator.
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! use rvtime_guest::{call2, heap};
//!
//! /// The embedder calls this first, passing the bounds of the heap it
//! /// committed (`Store::heap()`).
//! #[unsafe(no_mangle)]
//! pub extern "C" fn init(start: u64, size: u64) -> u64 {
//!     unsafe { heap::init(start as usize, size as usize) };
//!     0
//! }
//!
//! #[unsafe(no_mangle)]
//! pub extern "C" fn add(a: u64, b: u64) -> u64 {
//!     // Whatever call number the embedder registered.
//!     unsafe { call2(1, a, b) }
//! }
//! ```
//!
//! ## What is deliberately absent
//!
//! There are no standard host functions here. rvtime is a mechanism and the
//! host interface is policy: the numbers, their meanings, and what a guest is
//! permitted to do belong to the embedder. This crate only knows *how* to make
//! a call, never *which* calls exist.
//!
//! ## Calling convention
//!
//! `ecall` takes the call number in `a7` and arguments in `a0` onwards, which
//! is the standard RISC-V syscall convention. `a7` carries the number, so at
//! most seven registers are left for arguments; the host side accepts six,
//! which is what these wrappers expose. Buffers are passed the usual way, as a
//! pointer and a length in two registers.

#![no_std]

#[cfg(feature = "alloc")]
pub mod heap;

/// Stop the guest, reporting a breakpoint trap to the host.
///
/// Use this as the body of `#[panic_handler]`. The obvious alternative —
/// `loop {}` — hangs the thread the host called in on, which for an embedder
/// running many guests is far worse than a trap it can catch and report.
///
/// ```ignore
/// #[panic_handler]
/// fn panic(_: &core::panic::PanicInfo) -> ! {
///     rvtime_guest::abort()
/// }
/// ```
#[inline]
pub fn abort() -> ! {
    unsafe { core::arch::asm!("ebreak", options(noreturn, nostack)) }
}

/// Make a host call with no arguments.
///
/// # Safety
///
/// A host call is an FFI boundary. The host may do anything the embedder
/// permitted, including reading and writing this guest's memory through
/// pointers passed to it. The caller is responsible for the call number
/// meaning what it thinks it means.
#[inline]
pub unsafe fn call0(number: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!("ecall", in("a7") number, out("a0") out);
    }
    out
}

/// Make a host call with one argument. See [`call0`] for safety.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call1(number: u64, a0: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!("ecall", in("a7") number, inlateout("a0") a0 => out);
    }
    out
}

/// Make a host call with two arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call2(number: u64, a0: u64, a1: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            inlateout("a0") a0 => out,
            in("a1") a1,
        );
    }
    out
}

/// Make a host call with three arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call3(number: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            inlateout("a0") a0 => out,
            in("a1") a1,
            in("a2") a2,
        );
    }
    out
}

/// Make a host call with four arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call4(number: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            inlateout("a0") a0 => out,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
        );
    }
    out
}

/// Make a host call with five arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call5(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            inlateout("a0") a0 => out,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
        );
    }
    out
}

/// Make a host call with six arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call6(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            inlateout("a0") a0 => out,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            in("a5") a5,
        );
    }
    out
}
