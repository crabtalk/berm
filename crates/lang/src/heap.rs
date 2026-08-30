//! The guest's allocator, initialized the first time something allocates.
//!
//! Where the memory comes from is the one thing the two targets do not share.
//! On RV64 the host committed a region and the guest has to learn where it is;
//! the obvious way — the host enters the guest to hand the bounds over — costs
//! a second entry, and entering a guest is around 13µs against ~30ns for a host
//! call. So the guest asks instead, from inside the entry it is already in. On
//! wasm there is nobody to ask: the guest grows its own memory.
//!
//! Either way a program that never allocates never asks, which is why there is
//! nothing to declare.
#![cfg(all(
    feature = "alloc",
    any(target_arch = "wasm32", target_arch = "riscv64")
))]

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

/// The host's region, once. There is no more of it than what it committed, so
/// a second call is the allocation that did not fit failing.
#[cfg(target_arch = "riscv64")]
fn grow(heap: &mut linked_list_allocator::Heap, _: usize) -> bool {
    use crate::{
        abi::{HOST_HEAP_SIZE, HOST_HEAP_START},
        sys,
    };
    if heap.size() != 0 {
        return false;
    }
    let start = sys::call0(HOST_HEAP_START) as usize;
    let size = sys::call0(HOST_HEAP_SIZE) as usize;
    // Safety: the bounds come from the host, which committed exactly this
    // region for this guest and nothing else can reach it.
    unsafe { heap.init(start as *mut u8, size) };
    true
}

/// One wasm page. Fixed by the specification, so the rounding below invents no
/// size of its own: a grow asks for what did not fit and no more.
#[cfg(target_arch = "wasm32")]
const PAGE: usize = 64 * 1024;

/// More memory, taken from the engine.
///
/// `extend` needs the new bytes to sit directly on top of the old, which holds
/// because this is the only thing in the guest that grows memory.
#[cfg(target_arch = "wasm32")]
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
