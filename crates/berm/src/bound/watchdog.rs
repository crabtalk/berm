//! A bound on how long a guest may run.
//!
//! How long, and how a nested call inherits what is left, is berm's. Stopping
//! a guest that has run past it is the backend's: rvtime trips a flag its
//! compiled code polls, wasmtime counts down an epoch, and neither is a
//! decision about the program model.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

/// How long a guest may run before it is asked to stop.
///
/// A guest blocked in a syscall cannot notice until that call returns, so an
/// embedder's syscall has to time out well inside this. The bound exists to
/// stop non-termination, not to enforce latency, and a program doing slow but
/// finite work should finish rather than be killed.
///
/// This is a bound on a whole chain, not on each link.
const TIMEOUT: Duration = Duration::from_secs(60);

thread_local! {
    /// The deadline of the invocation this thread is already inside, if any.
    ///
    /// A guest reached from another guest's syscall runs on that guest's
    /// thread, so the caller's bound is here to be read.
    static INHERITED: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// When the invocation being entered has to be done by.
///
/// A nested invocation cannot outlive the one that called it: a guest blocked
/// in a syscall does not notice its own deadline until that call returns, so
/// without inheriting, depth *n* would buy *n* × [`TIMEOUT`] and the outer
/// bound would mean nothing.
pub(crate) struct Deadline {
    at: Instant,
    /// Restored on the way out, so the caller's bound outlives this one.
    outer: Option<Instant>,
}

impl Deadline {
    pub(crate) fn enter() -> Self {
        let outer = INHERITED.get();
        let at = outer.map_or_else(
            || Instant::now() + TIMEOUT,
            |inherited| inherited.min(Instant::now() + TIMEOUT),
        );
        INHERITED.set(Some(at));
        Self { at, outer }
    }

    pub(crate) fn at(&self) -> Instant {
        self.at
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        INHERITED.set(self.outer);
    }
}
