//! Exercises the shapes the translator has to get right: leaf functions,
//! recursion, an indirect call through a pointer table, atomics, and a switch.
//!
//! `#[inline(never)]` is load-bearing. Without it LLVM folds everything into
//! `_start` and the fixture stops testing anything.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU64, Ordering};

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn op_add(a: u64, b: u64) -> u64 {
    a.wrapping_add(b)
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn op_sub(a: u64, b: u64) -> u64 {
    a.wrapping_sub(b)
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn op_mul(a: u64, b: u64) -> u64 {
    a.wrapping_mul(b)
}

type Op = extern "C" fn(u64, u64) -> u64;

/// Forces `R_RISCV_64` relocations naming each function, which is how the
/// loader recovers the indirect-call target set.
#[no_mangle]
pub static OPS: [Op; 3] = [op_add, op_sub, op_mul];

#[inline(never)]
#[no_mangle]
pub extern "C" fn dispatch(which: usize, a: u64, b: u64) -> u64 {
    unsafe { read_volatile(&OPS[which % 3])(a, b) }
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn switcher(x: u64) -> u64 {
    match x {
        0 => 100,
        1 => 201,
        2 => 302,
        3 => 403,
        4 => 504,
        5 => 605,
        6 => 706,
        7 => 807,
        8 => 908,
        9 => 1009,
        10 => 1110,
        11 => 1211,
        12 => 1312,
        13 => 1413,
        14 => 1514,
        15 => 1615,
        16 => 1716,
        17 => 1817,
        _ => 0,
    }
}

#[no_mangle]
pub static COUNTER: AtomicU64 = AtomicU64::new(0);

#[inline(never)]
#[no_mangle]
pub extern "C" fn bump(n: u64) -> u64 {
    COUNTER.fetch_add(n, Ordering::SeqCst)
}

/// LLVM flattens this into a loop, so it exercises backward branches rather
/// than calls.
#[inline(never)]
#[no_mangle]
pub extern "C" fn recurse(n: u64) -> u64 {
    if n == 0 { 1 } else { n.wrapping_mul(recurse(n - 1)) }
}

/// Two recursive calls, so neither can become a tail call and at least one
/// real self-call survives optimisation.
#[inline(never)]
#[no_mangle]
pub extern "C" fn fib(n: u64) -> u64 {
    if n < 2 { n } else { fib(n - 1).wrapping_add(fib(n - 2)) }
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn shifts(a: u64, b: u64) -> u64 {
    let x = a << (b & 63);
    let y = a >> (b & 63);
    let z = (a as i64 >> (b & 63)) as u64;
    let w = (a as u32).wrapping_shl(b as u32) as u64;
    x ^ y ^ z ^ w
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn divides(a: u64, b: u64) -> u64 {
    let s = (a as i64).wrapping_div(b as i64 | 1) as u64;
    let u = a / (b | 1);
    let r = (a as i64).wrapping_rem(b as i64 | 1) as u64;
    let m = a % (b | 1);
    s ^ u ^ r ^ m
}

#[no_mangle]
pub static mut SINK: u64 = 0;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut acc = 0u64;
    acc = acc.wrapping_add(dispatch(1, 10, 3));
    acc = acc.wrapping_add(switcher(7));
    acc = acc.wrapping_add(bump(5));
    acc = acc.wrapping_add(recurse(10));
    acc = acc.wrapping_add(fib(15));
    acc = acc.wrapping_add(shifts(0x1234_5678_9abc_def0, 13));
    acc = acc.wrapping_add(divides(1_000_003, 97));
    unsafe { write_volatile(&raw mut SINK, acc) };
    loop {}
}
