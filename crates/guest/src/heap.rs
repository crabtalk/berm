//! A global allocator over the region rvtime committed.
//!
//! rvtime reserves a heap between the guest image and its stack, but does not
//! tell the guest where it is — the bounds travel through whatever host
//! interface the embedder defined. Read them with `Store::heap()` on the host
//! side, pass them in, and hand them to [`init`].
//!
//! Nothing here allocates until [`init`] has been called, so a guest that never
//! calls it simply has no heap rather than a corrupt one.

use core::alloc::{GlobalAlloc, Layout};
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: Heap = Heap(LockedHeap::empty());

/// Wraps the allocator so an allocation before [`init`] fails loudly instead of
/// writing through a null region.
struct Heap(LockedHeap);

unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.0.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.0.dealloc(ptr, layout) }
    }
}

/// Give the allocator the heap region the host reserved.
///
/// Call once, before anything allocates.
///
/// # Safety
///
/// `start..start + size` must be readable, writable, and owned by this guest —
/// which is exactly what rvtime's `Store::heap()` describes. Passing any other
/// region, or calling twice, is undefined.
pub unsafe fn init(start: usize, size: usize) {
    unsafe { ALLOCATOR.0.lock().init(start as *mut u8, size) };
}

/// Bytes currently handed out.
pub fn used() -> usize {
    ALLOCATOR.0.lock().used()
}

/// Bytes still available.
pub fn free() -> usize {
    ALLOCATOR.0.lock().free()
}
