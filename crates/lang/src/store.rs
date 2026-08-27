//! This harness's own bytes, surviving its invocations.
//!
//! Guest memory does not cross an invocation — every call gets a fresh one —
//! so anything a harness needs next time goes here.
//!
//! Neither door names a harness. The keyspace is whichever one is asking, which
//! the host reads off the call itself, so another harness's keys are not
//! refused: there is nothing to say to reach them.
//!
//! Whether a host serves these at all is its own decision, the same as
//! [`crate::call`]: under a plain `berm::Berm` the call traps as an unknown
//! host call.
#![cfg(feature = "alloc")]

use crate::abi::{self, host, host::CallError, wire};
use alloc::{string::String, vec::Vec};

/// Read `key`, or `None` if this harness has never written it.
///
/// ```ignore
/// let seen = berm_lang::get("last-run")?.unwrap_or_default();
/// ```
pub fn get(key: &str) -> Result<Option<Vec<u8>>, CallError> {
    let reply = host::call(abi::HOST_GET, &wire::request(&[key.as_bytes()]))?;
    // One field is a value, none is an absent key — which is why the reply is
    // framed rather than the bytes themselves: a stored empty value is a
    // value, and would otherwise read as never written.
    let Some(fields) = wire::fields(&reply) else {
        return Err(CallError::Failed(String::from(
            "host framed a reply this guest cannot read",
        )));
    };
    Ok(fields.first().map(|value| value.to_vec()))
}

/// Write `key`, replacing whatever was there.
pub fn set(key: &str, value: &[u8]) -> Result<(), CallError> {
    host::call(abi::HOST_SET, &wire::request(&[key.as_bytes(), value]))?;
    Ok(())
}
