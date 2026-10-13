//! What `declare_program!`'s typed calls expand to.

use crate::CallError;
use alloc::{format, string::String};

/// `value` as JSON, for a call's arguments.
pub fn to<T: serde_guest::Serialize>(value: &T) -> Result<String, CallError> {
    serde_json_guest::to_string(value)
        .map_err(|error| CallError::Failed(format!("unserializable arguments: {error}")))
}

/// A call's result, read as `T`.
pub fn from<T: for<'de> serde_guest::Deserialize<'de>>(result: &str) -> Result<T, CallError> {
    serde_json_guest::from_str(result)
        .map_err(|error| CallError::Failed(format!("unreadable result: {error}")))
}
