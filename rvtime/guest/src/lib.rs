//! Guest-side support for programs running under rvtime.
//!
//! This crate is compiled *for* RISC-V and linked into the guest. It supplies
//! the two things every guest needs and nothing else: a way to reach the host,
//! and an allocator.
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
//! ## Off the guest's target
//!
//! It also builds for the host, where every wrapper is present but panics if
//! it is actually reached, and no global allocator is installed. That is not
//! for running guests off-target — it is so a guest author can `cargo test`
//! their own logic natively, without a RISC-V toolchain and without an
//! embedder, and only cross-compile to test the parts that genuinely talk to a
//! host.
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

#[cfg_attr(target_arch = "riscv64", path = "sys/riscv.rs")]
#[cfg_attr(not(target_arch = "riscv64"), path = "sys/stub.rs")]
mod sys;

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
    sys::abort()
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
    unsafe { sys::call0(number) }
}

/// Make a host call with one argument.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call1(number: u64, a0: u64) -> u64 {
    unsafe { sys::call1(number, a0) }
}

/// Make a host call with two arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call2(number: u64, a0: u64, a1: u64) -> u64 {
    unsafe { sys::call2(number, a0, a1) }
}

/// Make a host call with three arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call3(number: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    unsafe { sys::call3(number, a0, a1, a2) }
}

/// Make a host call with four arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call4(number: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    unsafe { sys::call4(number, a0, a1, a2, a3) }
}

/// Make a host call with five arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
pub unsafe fn call5(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> u64 {
    unsafe { sys::call5(number, a0, a1, a2, a3, a4) }
}

/// Make a host call with six arguments.
///
/// # Safety
///
/// See [`call0`].
#[inline]
#[allow(clippy::too_many_arguments)]
pub unsafe fn call6(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    unsafe { sys::call6(number, a0, a1, a2, a3, a4, a5) }
}
