//! What a tool subscribed with `#[logs(...)]` is called with.
//!
//! The same shape as `berm_api::Delivery`, which the host serializes.
#![cfg(feature = "args")]

use crate::Event;
use alloc::{string::String, vec::Vec};

/// Logs of one program, oldest first.
///
/// ```ignore
/// let delivery: berm_lang::Delivery = berm_lang::tool::parse(args, out)?;
/// ```
#[derive(serde_guest::Deserialize)]
#[serde(crate = "serde_guest")]
pub struct Delivery {
    pub program: String,
    pub logs: Vec<Logged>,
}

/// One log as delivered.
#[derive(serde_guest::Deserialize)]
#[serde(crate = "serde_guest")]
pub struct Logged {
    /// Its position in the program's logs, counting from 1.
    pub seq: u64,
    pub topics: Vec<String>,
    pub data: String,
}

impl Logged {
    /// This log as the event `T`, or `None` when its topics do not name `T`.
    pub fn read<T>(&self) -> Option<Result<T, serde_json_guest::Error>>
    where
        T: Event + for<'de> serde_guest::Deserialize<'de>,
    {
        self.topics
            .iter()
            .any(|topic| topic == T::NAME)
            .then(|| serde_json_guest::from_str(&self.data))
    }
}
