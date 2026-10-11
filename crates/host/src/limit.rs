//! Calls per minute per caller.

use parking_lot::Mutex;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

/// How many calls a caller may make in a minute.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Per account, app user or service.
    pub caller: u32,
    /// Per client address, for an app's anonymous calls.
    pub anonymous: u32,
}

/// This minute's counts.
#[derive(Default)]
pub(crate) struct Counter(Mutex<(u64, HashMap<String, u32>)>);

impl Counter {
    /// Count one call by `who`; `false` once it is past `limit` this minute.
    pub(crate) fn take(&self, who: &str, limit: u32) -> bool {
        let minute = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() / 60);
        let mut counts = self.0.lock();
        if counts.0 != minute {
            *counts = (minute, HashMap::new());
        }
        let count = counts.1.entry(who.to_owned()).or_default();
        *count += 1;
        *count <= limit
    }
}
