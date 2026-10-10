//! What a tool subscribed with `#[logs(...)]` is called with.
//!
//! The same shape as `berm_api::Delivery`, which the host serializes.
#![cfg(feature = "args")]

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
