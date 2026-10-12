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

/// A typed log: an `#[event]` struct in a `#[program]` module, or one
/// `declare_program!` generated from another program's events. It is emitted
/// under the topic [`Event::NAME`].
pub trait Event {
    const NAME: &'static str;
}

/// What [`crate::emit!`] expands to.
#[cfg(feature = "args")]
#[doc(hidden)]
pub fn event<T: Event + serde_guest::Serialize>(event: &T) -> Result<usize, CallError> {
    let data = serde_json_guest::to_string(event)
        .map_err(|error| CallError::Failed(alloc::format!("unserializable event: {error}")))?;
    emit(&[T::NAME], &data)?;
    Ok(data.len())
}

/// Emit an `#[event]` struct as a log under its name, and answer the bytes of
/// its data.
///
/// ```ignore
/// berm_lang::emit!(Posted { id: 1 })?;
/// ```
#[cfg(feature = "args")]
#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        $crate::__event(&$event)
    };
}
