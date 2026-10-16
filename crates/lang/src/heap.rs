//! The guest's allocator, initialized the first time something allocates by
//! growing the guest's own memory. A program that never allocates never grows
//! it.
#![cfg(target_arch = "wasm32")]

use core::alloc::{GlobalAlloc, Layout};
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: Heap = Heap(LockedHeap::empty());

struct Heap(LockedHeap);

unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut heap = self.0.lock();
        if let Ok(ptr) = heap.allocate_first_fit(layout) {
            return ptr.as_ptr();
        }
        if !grow(&mut heap, layout.size()) {
            return core::ptr::null_mut();
        }
        heap.allocate_first_fit(layout)
            .map_or(core::ptr::null_mut(), |p| p.as_ptr())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(ptr) = core::ptr::NonNull::new(ptr) {
            unsafe { self.0.lock().deallocate(ptr, layout) };
        }
    }
}

/// One wasm page. Fixed by the specification, so the rounding below invents no
/// size of its own: a grow asks for what did not fit and no more.
const PAGE: usize = 64 * 1024;

/// More memory, taken from the engine.
///
/// `extend` needs the new bytes to sit directly on top of the old, which holds
/// because this is the only thing in the guest that grows memory.
fn grow(heap: &mut linked_list_allocator::Heap, at_least: usize) -> bool {
    let pages = at_least.div_ceil(PAGE).max(1);
    let previous = core::arch::wasm32::memory_grow(0, pages);
    if previous == usize::MAX {
        return false;
    }
    let size = pages * PAGE;
    // Safety: the engine just reserved these bytes for this guest, and nothing
    // else in it can reach them.
    match heap.size() {
        0 => unsafe { heap.init((previous * PAGE) as *mut u8, size) },
        _ => unsafe { heap.extend(size) },
    }
    true
}
