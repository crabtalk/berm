//! What services emit: each log taken as it is emitted, whatever its call
//! ends as, held in memory and written in batches to `service_logs` beside
//! the call it was emitted in, kept [`usage::WINDOW`] days.

use crate::{host, store::Store, usage};
use anyhow::Result;
use berm::{Callsite, Log, Now};
use parking_lot::Mutex;
use rusqlite::{OptionalExtension, params};
use std::{collections::HashMap, mem};

/// Logs not yet written, and the call each running chain is.
pub(crate) struct Pending {
    pending: Mutex<Vec<Emitted>>,
    /// The call each chain started here is, by [`Callsite::chain`].
    runs: Mutex<HashMap<u64, u64>>,
    now: Now,
}

/// A log not yet written.
pub(crate) struct Emitted {
    /// The key of the service that emitted it.
    service: String,
    call: Option<u64>,
    at: u64,
    topics: Vec<String>,
    data: String,
}

impl Pending {
    pub(crate) fn new(now: Now) -> Self {
        Self {
            pending: Mutex::default(),
            runs: Mutex::default(),
            now,
        }
    }

    /// Hold that chain `chain` is call `call` until [`Pending::ended`].
    pub(crate) fn started(&self, chain: u64, call: u64) {
        self.runs.lock().insert(chain, call);
    }

    pub(crate) fn ended(&self, chain: u64) {
        self.runs.lock().remove(&chain);
    }

    /// The call chain `chain` is, while it runs.
    pub(crate) fn run(&self, chain: u64) -> Option<u64> {
        self.runs.lock().get(&chain).copied()
    }

    /// Hold `log`, emitted at `at`. `false`, holding nothing, in a dry chain.
    pub(crate) fn take(&self, at: &Callsite, log: Log) -> bool {
        if at.dry.is_some() {
            return false;
        }
        let call = self.run(at.chain);
        self.pending.lock().push(Emitted {
            service: at.program.to_string(),
            call,
            at: (self.now)(),
            topics: log.topics,
            data: log.data,
        });
        true
    }

    pub(crate) fn take_all(&self) -> Vec<Emitted> {
        mem::take(&mut *self.pending.lock())
    }
}

/// Add `logs`, each numbered after its service's last, and drop what is older
/// than [`usage::WINDOW`] days before `now`. Logs of a service this host does
/// not run are dropped.
pub(crate) async fn write(store: &Store, logs: Vec<Emitted>, now: u64) -> Result<()> {
    let since = now.saturating_sub(usage::WINDOW as u64 * 86_400_000);
    store
        .run(move |conn| {
            let tx = conn.transaction()?;
            for log in &logs {
                let (scope, name) = host::split(&log.service);
                let Some(seq) = tx
                    .query_row(
                        "UPDATE services SET seq = seq + 1 WHERE scope = ?1 AND name = ?2
                         RETURNING seq",
                        params![scope, name],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                else {
                    continue;
                };
                tx.execute(
                    "INSERT INTO service_logs (scope, name, seq, call, at, topics, data)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        scope,
                        name,
                        seq,
                        log.call.map(|call| call as i64),
                        log.at as i64,
                        serde_json::to_string(&log.topics).unwrap_or_default(),
                        log.data
                    ],
                )?;
            }
            tx.execute(
                "DELETE FROM service_logs WHERE at < ?1",
                params![since as i64],
            )?;
            tx.commit()
        })
        .await
}
