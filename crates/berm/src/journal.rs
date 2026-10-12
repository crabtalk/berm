//! What a chain wrote and emitted, held until its first call returns.
//!
//! `berm.set` and `berm.emit` land here, not in the host. A chain whose first
//! call returns a result hands the whole of it back from [`crate::Program::call`]
//! as [`Changes`]; one that fails, traps or runs past its deadline hands back
//! nothing. A nested call that fails takes its own entries with it, and the
//! caller carries on from where it was.
//!
//! What lands here is what the host leaves to it: [`crate::syscall::store`]
//! and [`crate::syscall::emit`] ask the host first.

use crate::Refused;
use anyhow::Result;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

/// Bytes of keys, values, topics and data one chain may hold before the next
/// entry is refused.
pub const LIMIT: usize = 16 << 20;

/// What a chain left behind, for the host to commit as one.
#[derive(Debug, Default)]
pub struct Changes {
    /// The last value each program wrote under each key.
    pub writes: Vec<Write>,
    /// Every log, in the order the chain emitted it.
    pub logs: Vec<Log>,
}

/// One key a program wrote.
#[derive(Debug, Clone)]
pub struct Write {
    pub program: Arc<str>,
    pub key: String,
    pub value: Vec<u8>,
}

/// One log a program emitted.
#[derive(Debug, Clone)]
pub struct Log {
    pub program: Arc<str>,
    pub topics: Vec<String>,
    pub data: String,
}

/// Where in a [`Journal`] a nested call began.
#[derive(Clone, Copy)]
pub(crate) struct Mark {
    entries: usize,
    bytes: usize,
}

/// One chain's entries. Shared by every invocation in it.
#[derive(Debug, Default)]
pub(crate) struct Journal(Mutex<Entries>);

#[derive(Debug, Default)]
struct Entries {
    list: Vec<Entry>,
    bytes: usize,
}

#[derive(Debug)]
enum Entry {
    Write(Write),
    Log(Log),
}

impl Journal {
    /// Hold `value` under `program`'s `key`.
    pub(crate) fn write(&self, program: &Arc<str>, key: &str, value: &[u8]) -> Result<()> {
        self.push(
            key.len() + value.len(),
            Entry::Write(Write {
                program: program.clone(),
                key: key.to_owned(),
                value: value.to_vec(),
            }),
        )
    }

    /// Hold one log.
    pub(crate) fn emit(&self, log: Log) -> Result<()> {
        let size = log.data.len() + log.topics.iter().map(String::len).sum::<usize>();
        self.push(size, Entry::Log(log))
    }

    /// What `program` last wrote under `key` in this chain, if it has.
    pub(crate) fn read(&self, program: &str, key: &str) -> Option<Vec<u8>> {
        self.lock().list.iter().rev().find_map(|entry| match entry {
            Entry::Write(write) if &*write.program == program && write.key == key => {
                Some(write.value.clone())
            }
            _ => None,
        })
    }

    /// Hold what a call that kept a journal of its own left behind.
    pub(crate) fn extend(&self, changes: Changes) -> Result<()> {
        for write in changes.writes {
            self.push(write.key.len() + write.value.len(), Entry::Write(write))?;
        }
        for log in changes.logs {
            self.emit(log)?;
        }
        Ok(())
    }

    pub(crate) fn mark(&self) -> Mark {
        let entries = self.lock();
        Mark {
            entries: entries.list.len(),
            bytes: entries.bytes,
        }
    }

    /// Drop everything held since `mark`.
    pub(crate) fn revert(&self, mark: Mark) {
        let mut entries = self.lock();
        entries.list.truncate(mark.entries);
        entries.bytes = mark.bytes;
    }

    /// Everything held, with each key's writes collapsed to its last.
    pub(crate) fn take(&self) -> Changes {
        let list = std::mem::take(&mut self.lock().list);
        let mut writes = BTreeMap::new();
        let mut logs = Vec::new();
        for entry in list {
            match entry {
                Entry::Write(write) => {
                    writes.insert((write.program.clone(), write.key.clone()), write);
                }
                Entry::Log(log) => logs.push(log),
            }
        }
        Changes {
            writes: writes.into_values().collect(),
            logs,
        }
    }

    fn push(&self, size: usize, entry: Entry) -> Result<()> {
        let mut entries = self.lock();
        if entries.bytes + size > LIMIT {
            return Err(Refused(format!(
                "the chain already holds {} bytes of writes and logs, of {LIMIT}",
                entries.bytes
            ))
            .into());
        }
        entries.bytes += size;
        entries.list.push(entry);
        Ok(())
    }

    /// Reached from syscalls, which may not panic: a poisoned lock is read
    /// through.
    fn lock(&self) -> MutexGuard<'_, Entries> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
