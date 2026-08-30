//! How many guests this thread is already inside.
//!
//! A guest reached from another guest's host call runs synchronously on the
//! thread that entered the outer one, so the count is the thread's. It lives
//! here because berm is what enters a guest: a syscall reads it off its
//! [`crate::Callsite`] rather than tracking entries it cannot see.

use std::cell::Cell;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// How many guests are on this thread's stack.
pub(crate) fn current() -> u32 {
    DEPTH.get()
}

/// Counts one guest for as long as it lives.
pub(crate) struct Level;

impl Level {
    pub(crate) fn enter() -> Self {
        DEPTH.set(DEPTH.get() + 1);
        Self
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        DEPTH.set(DEPTH.get() - 1);
    }
}
