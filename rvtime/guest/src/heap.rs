//! A global allocator over the region rvtime committed.
//!
//! rvtime reserves a heap between the guest image and its stack, but does not
//! tell the guest where it is — the bounds travel through whatever host
//! interface the embedder defined. Read them with `Store::heap()` on the host
//! side, pass them in, and hand them to [`init`].
//!
//! Nothing here allocates until [`init`] has been called, so a guest that never
//! calls it simply has no heap rather than a corrupt one.
//!
//! Off the guest's target these are inert and no allocator is installed — see
//! the crate docs for why the host build exists at all.

use crate::sys;

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
    unsafe { sys::init(start, size) }
}

/// Bytes currently handed out.
pub fn used() -> usize {
    sys::used()
}

/// Bytes still available.
pub fn free() -> usize {
    sys::free()
}
