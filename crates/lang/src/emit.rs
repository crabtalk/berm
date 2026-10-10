//! Logs, for whoever reads this program's logs.
//!
//! A log is kept only if the call the host started returns a result: a failed
//! chain, or a failed call this one made, takes its logs with it.
#![cfg(feature = "alloc")]

use crate::abi::{self, host, host::CallError, wire};
use alloc::vec::Vec;

/// Emit `data` under `topics`, which a reader filters by.
///
/// ```ignore
/// berm_lang::emit(&["item", "hn"], r#"{"id":1}"#)?;
/// ```
pub fn emit(topics: &[&str], data: &str) -> Result<(), CallError> {
    let mut request = Vec::new();
    wire::field(&mut request, data.as_bytes());
    for topic in topics {
        wire::field(&mut request, topic.as_bytes());
    }
    host::call(abi::HOST_EMIT, &request)?;
    Ok(())
}
