//! Recovering from guest memory faults.

use rvtime_compiler::trap;
use std::ptr;

/// A stand-in guest region. Any power of two will do here.
const REGION: u64 = 64 << 20;

/// Stand in for guest memory: a reserved window with nothing committed, so
/// every address in it is a guard page.
fn reserve() -> *mut libc::c_void {
    let base = unsafe {
        libc::mmap(
            ptr::null_mut(),
            REGION as usize,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    assert_ne!(base, libc::MAP_FAILED, "reservation failed");
    trap::set_guest_region(base as usize, REGION);
    base
}

fn release(base: *mut libc::c_void) {
    unsafe { libc::munmap(base, REGION as usize) };
}

#[test]
fn returns_the_value_when_nothing_faults() {
    assert_eq!(trap::protect(|| 7u64), Ok(7));
}

#[test]
fn catches_a_fault_and_reports_the_guest_address() {
    let base = reserve();

    let fault = trap::protect(|| unsafe { ptr::read_volatile((base as *const u8).add(0x4000)) })
        .expect_err("reading a guard page must fault");

    assert_eq!(fault.guest, Some(0x4000));
    // Linux reports these as SIGSEGV; macOS on arm64 reports SIGBUS.
    assert!(fault.signal == libc::SIGSEGV || fault.signal == libc::SIGBUS);

    release(base);
}

#[test]
fn recovers_well_enough_to_run_again() {
    let base = reserve();

    for _ in 0..3 {
        assert!(trap::protect(|| unsafe { ptr::read_volatile(base as *const u8) }).is_err());
        assert_eq!(trap::protect(|| 1u8), Ok(1));
    }

    release(base);
}

/// A guest entered from inside another guest's host call must not leave its
/// region behind: the outer guest keeps running afterwards, and its next fault
/// has to be reported against its own memory.
#[test]
fn a_nested_region_does_not_outlive_its_entry() {
    let outer = reserve();
    let inner = reserve();

    let fault = trap::protect_guest(outer as usize, REGION, || {
        // The inner guest, entered and returned from while the outer is still
        // on the stack.
        assert_eq!(trap::protect_guest(inner as usize, REGION, || 7u64), Ok(7));

        unsafe { ptr::read_volatile((outer as *const u8).add(0x4000)) }
    })
    .expect_err("reading a guard page must fault");

    assert_eq!(fault.guest, Some(0x4000));

    release(outer);
    release(inner);
}
