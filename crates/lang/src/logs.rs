//! What a tool subscribed with `#[logs(...)]` is called with: its parameters
//! are `program: String` and `logs: Vec<Logged>`, the fields of the
//! `berm_api::Delivery` the host serializes.

use crate::Event;
use alloc::{string::String, vec::Vec};

/// One log as delivered.
#[derive(crate::Schema)]
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
