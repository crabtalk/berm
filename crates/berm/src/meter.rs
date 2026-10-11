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
