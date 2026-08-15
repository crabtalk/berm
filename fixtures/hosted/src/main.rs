//! Host-call fixture, written against the `rvtime-guest` SDK.
//!
//! Everything here goes through the SDK rather than hand-rolled inline asm, so
//! the existing host-call tests double as the SDK's integration test. Unlike
//! the other fixtures, `_start` returns normally so `Instance::run` can be
//! tested.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::panic::PanicInfo;
use rvtime_guest::{call0, call1, call2, heap};

/// Traps instead of looping, so a guest bug reaches the host as
/// `Trap::Breakpoint` rather than hanging the thread that called in.
#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    rvtime_guest::abort()
}

/// Adds two numbers on the host side.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_add(a: u64, b: u64) -> u64 {
    unsafe { call2(1, a, b) }
}

/// Asks the host to sum `len` bytes of guest memory at `ptr`.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_sum(ptr: u64, len: u64) -> u64 {
    unsafe { call2(2, ptr, len) }
}

/// Asks the host to fill `len` bytes of guest memory at `ptr`.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_fill(ptr: u64, len: u64) -> u64 {
    unsafe { call2(3, ptr, len) }
}

/// Bumps a counter held by the host, not the guest.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_tick() -> u64 {
    unsafe { call0(4) }
}

/// A call the host is expected to refuse.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_refused(x: u64) -> u64 {
    unsafe { call1(5, x) }
}

/// A call number the host never registers.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_unknown() -> u64 {
    unsafe { call0(99) }
}

/// Interleaves host calls with guest work, so register save/restore around
/// `ecall` has to be right.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_mixed(a: u64, b: u64) -> u64 {
    let x = a.wrapping_mul(3);
    let y = unsafe { call2(1, a, b) };
    let z = b.wrapping_add(7);
    let w = unsafe { call2(1, y, z) };
    x.wrapping_add(w)
}

#[no_mangle]
pub static mut BUFFER: [u8; 32] = [0; 32];

/// Writes into a guest global, then has the host read it back.
#[inline(never)]
#[no_mangle]
pub extern "C" fn round_trip(len: u64) -> u64 {
    let buffer = &raw mut BUFFER as *mut u8;
    let mut index = 0u64;
    while index < len {
        unsafe { buffer.add(index as usize).write_volatile((index + 1) as u8) };
        index += 1;
    }
    call_sum(buffer as u64, len)
}

// -- heap ------------------------------------------------------------------

/// Hands the allocator the region the host committed.
///
/// The embedder reads the bounds from `Store::heap()` and passes them in;
/// rvtime never tells the guest where its heap is.
#[inline(never)]
#[no_mangle]
pub extern "C" fn init_heap(start: u64, size: u64) -> u64 {
    unsafe { heap::init(start as usize, size as usize) };
    0
}

/// Allocates a growing vector, sums it, and drops it.
///
/// Exercises allocation, reallocation as the vector grows, and free.
#[inline(never)]
#[no_mangle]
pub extern "C" fn alloc_sum(n: u64) -> u64 {
    let mut values: Vec<u64> = Vec::new();
    for value in 0..n {
        values.push(value);
    }
    values.iter().sum()
}

/// Bytes the allocator has handed out.
#[inline(never)]
#[no_mangle]
pub extern "C" fn heap_used() -> u64 {
    heap::used() as u64
}

/// Bytes the allocator still has.
#[inline(never)]
#[no_mangle]
pub extern "C" fn heap_free() -> u64 {
    heap::free() as u64
}

// -- memory probes ---------------------------------------------------------

/// Dereferences whatever address it is handed, for testing guard pages and
/// the address mask.
#[inline(never)]
#[no_mangle]
pub extern "C" fn read_at(addr: u64) -> u64 {
    unsafe { core::ptr::read_volatile(addr as *const u64) }
}

/// Writes to whatever address it is handed.
#[inline(never)]
#[no_mangle]
pub extern "C" fn write_at(addr: u64, value: u64) -> u64 {
    unsafe { core::ptr::write_volatile(addr as *mut u64, value) };
    0
}

// Keeps every exported function alive against `--gc-sections`, which would
// otherwise drop everything `_start` does not reach. Grouped by signature so
// no transmute is needed in a const initialiser.

#[no_mangle]
pub static EXPORTS_2: [extern "C" fn(u64, u64) -> u64; 6] =
    [call_add, call_sum, call_fill, call_mixed, write_at, init_heap];

#[no_mangle]
pub static EXPORTS_1: [extern "C" fn(u64) -> u64; 4] =
    [call_refused, round_trip, read_at, alloc_sum];

#[no_mangle]
pub static EXPORTS_0: [extern "C" fn() -> u64; 4] =
    [call_tick, call_unknown, heap_used, heap_free];

#[no_mangle]
pub static mut RESULT: u64 = 0;

#[no_mangle]
pub static mut ANCHOR: u64 = 0;

/// The ELF entry point. Returns, so `Instance::run` can be tested.
#[no_mangle]
pub extern "C" fn _start() {
    let total = call_add(20, 22).wrapping_add(call_tick());
    unsafe {
        core::ptr::write_volatile(&raw mut RESULT, total);
        // Anchors the export tables, and through them every exported function,
        // so `--gc-sections` keeps them.
        core::ptr::write_volatile(
            &raw mut ANCHOR,
            EXPORTS_0.as_ptr() as u64 ^ EXPORTS_1.as_ptr() as u64 ^ EXPORTS_2.as_ptr() as u64,
        );
    }
}
