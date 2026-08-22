//! The guest's own target: `ecall` with the number in `a7`, and an allocator
//! over the region the host committed.

#[inline]
pub fn abort() -> ! {
    unsafe { core::arch::asm!("ebreak", options(noreturn, nostack)) }
}

#[inline]
pub unsafe fn call0(number: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!("ecall", in("a7") number, out("a0") out);
    }
    out
}

#[inline]
pub unsafe fn call1(number: u64, a0: u64) -> u64 {
    let out: u64;
    unsafe {
        core::arch::asm!("ecall", in("a7") number, inlateout("a0") a0 => out);
    }
    out
}

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

#[inline]
#[allow(clippy::too_many_arguments)]
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

/// The allocator, installed only where a guest actually runs.
#[cfg(feature = "alloc")]
mod allocator {
    use core::alloc::{GlobalAlloc, Layout};
    use linked_list_allocator::LockedHeap;

    #[global_allocator]
    static ALLOCATOR: Heap = Heap(LockedHeap::empty());

    /// Wraps the allocator so an allocation before `init` fails loudly instead
    /// of writing through a null region.
    struct Heap(LockedHeap);

    unsafe impl GlobalAlloc for Heap {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { self.0.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { self.0.dealloc(ptr, layout) }
        }
    }

    pub unsafe fn init(start: usize, size: usize) {
        unsafe { ALLOCATOR.0.lock().init(start as *mut u8, size) };
    }

    pub fn used() -> usize {
        ALLOCATOR.0.lock().used()
    }

    pub fn free() -> usize {
        ALLOCATOR.0.lock().free()
    }
}

#[cfg(feature = "alloc")]
pub use allocator::{free, init, used};
