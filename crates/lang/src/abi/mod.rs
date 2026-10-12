//! The wire between a program and its host.
//!
//! A host call carries a number — but the number is *derived* from a name
//! rather than assigned (RFC 0205). A syscall is identified by what it is
//! called, so adding one cannot collide with someone else's allocation and no
//! registry of integers has to be maintained.
//!
//! The same hash is computed host-side in `crates/berm/src/abi.rs`. The two
//! must agree; if they ever drift, every call traps immediately as an unknown
//! host call rather than reaching the wrong syscall.
//!
//! Whether a call reaches a host at all is `sys`'s business, not this file's.
//!
//! Nothing here is for a program author to read: [`host`] and [`wire`] are
//! reached by generated code, and the numbers below by this module's own
//! functions. What an author writes against is the crate root.

use crate::sys;

/// One call path shared by every syscall. Needs a heap: a result whose
/// size the guest learns at runtime has nowhere else to go.
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub mod host;
/// Request framing, on the same terms as [`host`].
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub mod wire;

/// Write a UTF-8 message to the host's trace.
pub const HOST_TRACE: u64 = hash("berm.trace");
/// Byte length of this invocation's argument blob.
pub const HOST_ARG_LEN: u64 = hash("berm.args.len");
/// Copy the argument blob into guest memory.
pub const HOST_ARG_READ: u64 = hash("berm.args.read");
/// Finish this invocation with a result.
pub const HOST_DONE: u64 = hash("berm.done");
/// Fail this invocation with a message.
pub const HOST_FAIL: u64 = hash("berm.fail");
/// Copy the last syscall call's staged result into guest memory.
pub const HOST_RESULT_READ: u64 = hash("berm.result.read");
/// Read one of this program's own keys.
pub const HOST_GET: u64 = hash("berm.get");
/// Write one.
pub const HOST_SET: u64 = hash("berm.set");
/// Emit one log.
pub const HOST_EMIT: u64 = hash("berm.emit");
/// Call a tool on another program the same host is running.
pub const HOST_CALL: u64 = hash("berm.call");
/// Call a tool on one later, replacing whatever this program had pending.
pub const HOST_CALL_AFTER: u64 = hash("berm.call.after");
/// Make one HTTP request.
pub const HOST_HTTP_FETCH: u64 = hash("berm.http.fetch");
/// Make one HTTP request through the host's proxy.
pub const HOST_HTTP_PROXY: u64 = hash("berm.http.proxy");
/// Run one SQL statement against this program's own database.
pub const HOST_SQL: u64 = hash("berm.sql");
/// Milliseconds since the Unix epoch, as the host reads its clock.
pub const HOST_NOW: u64 = hash("berm.now");
/// Who made this call.
pub const HOST_CALLER: u64 = hash("berm.caller");
/// The end user the chain runs for.
pub const HOST_USER: u64 = hash("berm.user");
/// Mark this invocation's failure as refusing its caller.
pub const HOST_DENY: u64 = hash("berm.deny");

/// Set on the length a syscall returns when the staged bytes are an error
/// message rather than a result. A length never reaches this bit on its own,
/// so one return value carries both without a second call to ask which.
pub(crate) const ERROR: u64 = 1 << 63;

/// Set beside [`ERROR`] when the host refused the call and nothing ran, as
/// against something running and reporting failure.
pub(crate) const REFUSED: u64 = 1 << 62;

/// FNV-1a over the syscall's name, evaluated at compile time.
pub const fn hash(name: &str) -> u64 {
    let bytes = name.as_bytes();
    let mut result: u64 = 0xcbf2_9ce4_8422_2325;
    let mut at = 0;
    while at < bytes.len() {
        result ^= bytes[at] as u64;
        result = result.wrapping_mul(0x0000_0100_0000_01b3);
        at += 1;
    }
    result
}

/// Write a line to the host's trace.
pub fn trace(message: &str) {
    sys::call2(HOST_TRACE, message.as_ptr() as u64, message.len() as u64);
}

/// What time the host says it is, in milliseconds since the Unix epoch.
///
/// The one clock a program has. An invocation an event started cannot
/// otherwise tell a wake that arrived on time from one that arrived late.
pub fn now() -> u64 {
    sys::call0(HOST_NOW)
}

/// Mark this invocation's failure as refusing its caller.
pub fn deny() {
    sys::call0(HOST_DENY);
}

/// How many bytes this invocation was given.
pub fn args_len() -> usize {
    sys::call0(HOST_ARG_LEN) as usize
}

/// Pull the argument blob into `buffer`, returning the blob's *full* length —
/// not what fit. A caller that gets back more than it offered was truncated
/// and must say so rather than acting on half a request.
pub fn read_args(buffer: &mut [u8]) -> usize {
    sys::call2(
        HOST_ARG_READ,
        buffer.as_mut_ptr() as u64,
        buffer.len() as u64,
    ) as usize
}

/// Hand the host what this tool produced.
pub fn done(result: &[u8]) {
    sys::call2(HOST_DONE, result.as_ptr() as u64, result.len() as u64);
}

/// Report failure, the other half of [`done`]. The host marks the invocation
/// an error rather than a result, which is the difference between a tool that
/// failed and a tool that returned the word "error".
pub fn fail(message: &[u8]) {
    sys::call2(HOST_FAIL, message.as_ptr() as u64, message.len() as u64);
}

/// Pull the last syscall call's staged result, returning its *full* length
/// exactly as [`read_args`] does — the one pattern both use.
pub(crate) fn read_result(buffer: &mut [u8]) -> usize {
    sys::call2(
        HOST_RESULT_READ,
        buffer.as_mut_ptr() as u64,
        buffer.len() as u64,
    ) as usize
}
