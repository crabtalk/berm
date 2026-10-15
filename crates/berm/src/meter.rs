//! What a chain used, by program.

use crate::Refused;
use anyhow::Result;
use berm_api::Meter;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

/// One chain's meters, by program, shared by every invocation in it. Filled
/// whether the chain returns, fails or traps.
#[derive(Debug, Default)]
pub struct Meters(Mutex<BTreeMap<Arc<str>, Meter>>);

impl Meters {
    pub(crate) fn add(&self, program: &str, used: Meter) {
        let mut meters = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match meters.get_mut(program) {
            Some(meter) => *meter += used,
            None => {
                meters.insert(program.into(), used);
            }
        }
    }

    pub fn take(&self) -> BTreeMap<Arc<str>, Meter> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// What a program's invocations hold while they run.
#[derive(Debug, Default)]
pub struct Running {
    calls: AtomicU64,
    memory: AtomicU64,
}

impl Running {
    /// Its invocations running now, and the bytes of linear memory they hold.
    pub fn now(&self) -> (u64, u64) {
        (
            self.calls.load(Ordering::Relaxed),
            self.memory.load(Ordering::Relaxed),
        )
    }
}

/// One invocation's part of its program's [`Running`], from when it is built
/// until it is dropped. Its store's limiter, so the memory it holds is counted
/// as it grows.
#[derive(Debug)]
pub(crate) struct Held {
    running: Arc<Running>,
    memory: u64,
}

impl Held {
    pub(crate) fn new(running: Arc<Running>) -> Self {
        running.calls.fetch_add(1, Ordering::Relaxed);
        Self { running, memory: 0 }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.running.calls.fetch_sub(1, Ordering::Relaxed);
        self.running
            .memory
            .fetch_sub(self.memory, Ordering::Relaxed);
    }
}

impl wasmtime::ResourceLimiter for Held {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let allowed = maximum.is_none_or(|maximum| desired <= maximum);
        if allowed {
            let grown = (desired - current) as u64;
            self.memory += grown;
            self.running.memory.fetch_add(grown, Ordering::Relaxed);
        }
        Ok(allowed)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(maximum.is_none_or(|maximum| desired <= maximum))
    }
}

/// One invocation's rows read and written, against its budget.
#[derive(Debug)]
pub(crate) struct Rows {
    used: AtomicU64,
    budget: u64,
}

impl Rows {
    pub(crate) fn new(budget: u64) -> Self {
        Self {
            used: AtomicU64::new(0),
            budget,
        }
    }

    pub(crate) fn add(&self, rows: u64) {
        self.used.fetch_add(rows, Ordering::Relaxed);
    }

    /// Refused once the budget is used.
    pub(crate) fn left(&self) -> Result<()> {
        if self.used.load(Ordering::Relaxed) >= self.budget {
            return Err(Refused(format!(
                "this call has used its budget of {} rows",
                self.budget
            ))
            .into());
        }
        Ok(())
    }
}
