//! A bound on how long a guest may run.
//!
//! How long, and how a nested call inherits what is left, is berm's. Stopping
//! a guest that has run past it is wasmtime's, counting down an epoch.

use std::time::{Duration, Instant};

/// How long a chain of invocations may run before it is asked to stop.
///
/// A guest suspended in a syscall cannot notice until the reply resolves, so
/// an embedder's syscall has to bound its own wait inside this. The bound
/// stops non-termination; a program doing slow but finite work finishes.
///
/// A nested invocation runs under its caller's deadline, so depth *n* buys
/// one [`TIMEOUT`], not *n*.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The deadline of a chain entered now.
pub(crate) fn fresh() -> Instant {
    Instant::now() + TIMEOUT
}
