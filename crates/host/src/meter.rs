//! What each service used, per day: counted in memory and written in batches
//! to `service_meters` on the host the service runs on, beside the bytes it
//! stored, sampled once a day.

use crate::{host, store::Store, usage};
use anyhow::Result;
use berm::{Meter, Now};
use berm_api::host::Metered;
use parking_lot::Mutex;
use rusqlite::params;
use std::{collections::HashMap, mem, path::Path};

/// Meters not yet written, by service and day.
pub(crate) struct Meters {
    pending: Mutex<HashMap<(String, i32), Meter>>,
    now: Now,
}

impl Meters {
    pub(crate) fn new(now: Now) -> Self {
        Self {
            pending: Mutex::default(),
            now,
        }
    }

    /// Count `used` against `service` today.
    pub(crate) fn record(&self, service: &str, used: Meter) {
        let day = (usage::seconds(&self.now) / 86_400) as i32;
        *self
            .pending
            .lock()
            .entry((service.to_owned(), day))
            .or_default() += used;
    }

    /// Take what is pending.
    pub(crate) fn take(&self) -> Vec<((String, i32), Meter)> {
        mem::take(&mut *self.pending.lock()).into_iter().collect()
    }
}

/// Add `metered` to what is recorded. A service no longer deployed is dropped.
pub(crate) async fn write(store: &Store, metered: Vec<((String, i32), Meter)>) -> Result<()> {
    if metered.is_empty() {
        return Ok(());
    }
    store
        .run(move |conn| {
            let tx = conn.transaction()?;
            for ((service, day), meter) in &metered {
                let (scope, name) = host::split(service);
                tx.execute(
                    "INSERT INTO service_meters
                            (scope, name, day, calls, instructions, memory, read, written,
                             proxied, overage)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10
                      WHERE EXISTS (SELECT 1 FROM services WHERE scope = ?1 AND name = ?2)
                     ON CONFLICT (scope, name, day) DO UPDATE SET
                        calls = calls + excluded.calls,
                        instructions = instructions + excluded.instructions,
                        memory = MAX(memory, excluded.memory),
                        read = read + excluded.read,
                        written = written + excluded.written,
                        proxied = proxied + excluded.proxied,
                        overage = overage + excluded.overage",
                    params![
                        scope,
                        name,
                        day,
                        meter.calls as i64,
                        meter.instructions as i64,
                        meter.memory as i64,
                        meter.read as i64,
                        meter.written as i64,
                        meter.proxied as i64,
                        meter.overage as i64
                    ],
                )?;
            }
            tx.commit()
        })
        .await
}

/// Record what `service` stores on `day`: its database file, if any, at
/// `database`, and its state and logs.
pub(crate) async fn sample(store: &Store, service: &str, database: &Path, day: i32) -> Result<()> {
    let file: u64 = ["sqlite", "sqlite-wal"]
        .iter()
        .filter_map(|extension| std::fs::metadata(database.with_extension(extension)).ok())
        .map(|metadata| metadata.len())
        .sum();
    let (scope, name) = host::split(service);
    let (scope, name) = (scope.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            conn.execute(
                "INSERT INTO service_meters (scope, name, day, stored)
                 SELECT ?1, ?2, ?3, ?4
                        + (SELECT COALESCE(SUM(LENGTH(key) + LENGTH(value)), 0) FROM service_state
                            WHERE scope = ?1 AND name = ?2)
                        + (SELECT COALESCE(SUM(LENGTH(topics) + LENGTH(data)), 0) FROM service_logs
                            WHERE scope = ?1 AND name = ?2)
                  WHERE EXISTS (SELECT 1 FROM services WHERE scope = ?1 AND name = ?2)
                 ON CONFLICT (scope, name, day) DO UPDATE SET stored = excluded.stored",
                params![scope, name, day, file as i64],
            )?;
            Ok(())
        })
        .await
}

/// `scope/name`'s meters over the [`usage::WINDOW`] days to `today`, oldest
/// first.
pub(crate) async fn read(
    store: &Store,
    scope: &str,
    name: &str,
    today: i32,
) -> Result<Vec<Metered>> {
    let (scope, name) = (scope.to_owned(), name.to_owned());
    let since = today - usage::WINDOW;
    store
        .run(move |conn| {
            let mut statement = conn.prepare(
                "SELECT day, calls, instructions, memory, read, written, proxied, stored,
                        overage
                   FROM service_meters
                  WHERE scope = ?1 AND name = ?2 AND day > ?3
                  ORDER BY day",
            )?;
            let rows = statement.query_map(params![scope, name, since], |row| {
                Ok(Metered {
                    day: row.get(0)?,
                    calls: row.get::<_, i64>(1)? as u64,
                    instructions: row.get::<_, i64>(2)? as u64,
                    memory: row.get::<_, i64>(3)? as u64,
                    read: row.get::<_, i64>(4)? as u64,
                    written: row.get::<_, i64>(5)? as u64,
                    proxied: row.get::<_, i64>(6)? as u64,
                    stored: row.get::<_, i64>(7)? as u64,
                    overage: row.get::<_, i64>(8)? as u64,
                })
            })?;
            rows.collect()
        })
        .await
}
