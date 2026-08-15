//! Host-call fixture.
//!
//! `ecall` takes the call number in `a7` and arguments in `a0`..`a6`, which is
//! the standard RISC-V syscall convention. Unlike the other fixtures, `_start`
//! here returns normally so the embedder's `run()` can be tested.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

#[inline(always)]
unsafe fn ecall2(number: u64, a0: u64, a1: u64) -> u64 {
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

#[inline(always)]
unsafe fn ecall1(number: u64, a0: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            inlateout("a0") a0 => out,
        );
    }
    out
}

#[inline(always)]
unsafe fn ecall0(number: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            out("a0") out,
        );
    }
    out
}

/// Adds two numbers on the host side.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_add(a: u64, b: u64) -> u64 {
    unsafe { ecall2(1, a, b) }
}

/// Asks the host to sum `len` bytes of guest memory at `ptr`.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_sum(ptr: u64, len: u64) -> u64 {
    unsafe { ecall2(2, ptr, len) }
}

/// Asks the host to fill `len` bytes of guest memory at `ptr`.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_fill(ptr: u64, len: u64) -> u64 {
    unsafe { ecall2(3, ptr, len) }
}

/// Bumps a counter held by the host, not the guest.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_tick() -> u64 {
    unsafe { ecall0(4) }
}

/// A call the host is expected to refuse.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_refused(x: u64) -> u64 {
    unsafe { ecall1(5, x) }
}

/// A call number the host never registers.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_unknown() -> u64 {
    unsafe { ecall0(99) }
}

/// Interleaves host calls with guest work, so register save/restore around
/// `ecall` has to be right.
#[inline(never)]
#[no_mangle]
pub extern "C" fn call_mixed(a: u64, b: u64) -> u64 {
    let x = a.wrapping_mul(3);
    let y = unsafe { ecall2(1, a, b) };
    let z = b.wrapping_add(7);
    let w = unsafe { ecall2(1, y, z) };
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

#[no_mangle]
pub static mut RESULT: u64 = 0;

// Keeps every exported function alive against `--gc-sections`, which would
// otherwise drop everything `_start` does not reach. Grouped by signature so
// no transmute is needed in a const initialiser.

#[no_mangle]
pub static EXPORTS_2: [extern "C" fn(u64, u64) -> u64; 5] =
    [call_add, call_sum, call_fill, call_mixed, write_at];

#[no_mangle]
pub static EXPORTS_1: [extern "C" fn(u64) -> u64; 3] = [call_refused, round_trip, read_at];

#[no_mangle]
pub static EXPORTS_0: [extern "C" fn() -> u64; 2] = [call_tick, call_unknown];

/// Dereferences whatever address it is handed, for testing guard pages and
/// the 32-bit address mask.
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
