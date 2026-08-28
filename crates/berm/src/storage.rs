//! What the runtime writes down, and where a host puts it.
//!
//! Three things outlive a process: the images that were deployed, the
//! connections that were open, and the wakes that were pending. berm names
//! them and reaches them through [`Storage`]; what a record costs to write and
//! where it lands is a decision about a host, which is why the implementation
//! is never here.
//!
//! Distinct from [`crate::system::store`], which is a *harness's* own keyspace
//! and something a guest reaches for. Nothing below is addressable from a
//! guest at all.

use anyhow::Result;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// Which of the runtime's records a key belongs to.
///
/// A hard partition: a listing of one never sees another's keys, so the same
/// name under two of them is two records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Records {
    /// Deployed images, by the name each answers to.
    Harnesses,
    /// Open connections, by id.
    Sockets,
    /// Pending wakes, by the harness that armed each.
    Wakes,
}

impl Records {
    /// Every one, for a host that must open them all at once.
    pub const ALL: [Records; 3] = [Records::Harnesses, Records::Sockets, Records::Wakes];

    /// A name a host can use for a directory, a table, or a key prefix.
    pub fn as_str(&self) -> &'static str {
        match self {
            Records::Harnesses => "harnesses",
            Records::Sockets => "sockets",
            Records::Wakes => "wakes",
        }
    }
}

/// Where the runtime's own records live.
///
/// Three methods, which is what restoring needs: write one, drop one, and read
/// back everything under a kind. Nothing fetches a single record by key —
/// coming back after a restart reads all of them, and everything else is
/// already in memory.
///
/// Synchronous because a write happens inside a guest's host call, where an
/// async one cannot go. A host backing this with something remote pays a
/// blocked thread for the call's duration.
pub trait Storage: Send + Sync + 'static {
    fn put(&self, records: Records, key: &str, value: &[u8]) -> Result<()>;

    /// Drop a record. `false` if it was not there.
    fn remove(&self, records: Records, key: &str) -> Result<bool>;

    /// Every record of one kind, as `(key, value)`. The order is the host's.
    fn list(&self, records: Records) -> Result<Vec<(String, Vec<u8>)>>;
}

/// Records held in memory, which is what an embedder with no disk gets.
///
/// A [`Berm`](crate::Berm) built on this forgets everything when the process
/// does — correct for a test or an example, and the reason it is here rather
/// than left for each of them to write.
#[derive(Default)]
pub struct Memory {
    held: Mutex<BTreeMap<(Records, String), Vec<u8>>>,
}

impl Memory {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

impl Storage for Memory {
    fn put(&self, records: Records, key: &str, value: &[u8]) -> Result<()> {
        self.held
            .lock()
            .expect("held records")
            .insert((records, key.to_owned()), value.to_vec());
        Ok(())
    }

    fn remove(&self, records: Records, key: &str) -> Result<bool> {
        Ok(self
            .held
            .lock()
            .expect("held records")
            .remove(&(records, key.to_owned()))
            .is_some())
    }

    fn list(&self, records: Records) -> Result<Vec<(String, Vec<u8>)>> {
        Ok(self
            .held
            .lock()
            .expect("held records")
            .iter()
            .filter(|((kind, _), _)| *kind == records)
            .map(|((_, key), value)| (key.clone(), value.clone()))
            .collect())
    }
}
