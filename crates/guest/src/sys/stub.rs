//! What a host call does when there is no host.
//!
//! Off RISC-V this crate is an ordinary library linked into someone's tests, so
//! the wrappers still exist and still type-check — a guest author can build and
//! test their own logic natively instead of cross-compiling to run anything at
//! all. A call that actually reaches the boundary panics, naming the number, so
//! a test that wandered across it says which one rather than reading a
//! plausible zero.

#[inline]
pub fn abort() -> ! {
    panic!("guest aborted outside a guest");
}

#[cold]
fn no_host(number: u64) -> ! {
    panic!("host call {number:#x} outside a guest");
}

#[inline]
pub unsafe fn call0(number: u64) -> u64 {
    no_host(number)
}

#[inline]
pub unsafe fn call1(number: u64, _a0: u64) -> u64 {
    no_host(number)
}

#[inline]
pub unsafe fn call2(number: u64, _a0: u64, _a1: u64) -> u64 {
    no_host(number)
}

#[inline]
pub unsafe fn call3(number: u64, _a0: u64, _a1: u64, _a2: u64) -> u64 {
    no_host(number)
}

#[inline]
pub unsafe fn call4(number: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    no_host(number)
}

#[inline]
pub unsafe fn call5(number: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64) -> u64 {
    no_host(number)
}

#[inline]
#[allow(clippy::too_many_arguments)]
pub unsafe fn call6(
    number: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
    _a4: u64,
    _a5: u64,
) -> u64 {
    no_host(number)
}

/// No allocator is installed off-target: a library has no business replacing
/// the allocator of a test binary that merely links it.
#[cfg(feature = "alloc")]
pub unsafe fn init(_start: usize, _size: usize) {}

#[cfg(feature = "alloc")]
pub fn used() -> usize {
    0
}

#[cfg(feature = "alloc")]
pub fn free() -> usize {
    0
}
