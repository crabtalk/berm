//! Each run of a service this host runs: held in memory and written in batches
//! to `service_calls` on the host the service runs on, kept [`usage::WINDOW`]
//! days.

use crate::{
    host,
    store::Store,
    usage::{self, Outcome, Source},
};
use anyhow::Result;
use berm::Now;
use berm_api::{
    Meter,
    host::{Call, CallPage},
};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, Row, named_params, params};
use std::{
    mem,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

/// Runs not yet written, each with the key of the service it ran.
pub(crate) struct Pending {
    pending: Mutex<Vec<Ran>>,
    /// The last id handed out.
    last: AtomicU64,
    now: Now,
}

/// A run not yet written.
pub(crate) struct Ran {
    id: u64,
    /// The key of the service it ran.
    service: String,
    at: u64,
    tool: String,
    source: Source,
    caller: String,
    outcome: Outcome,
    ms: u64,
    meter: Option<Meter>,
}

/// A run being timed, from [`Pending::start`].
#[derive(Clone, Copy)]
pub(crate) struct Started {
    /// The id its row takes.
    pub id: u64,
    at: u64,
    began: Instant,
}

impl Pending {
    /// Handing out ids after `last`, the largest written.
    pub(crate) fn new(now: Now, last: u64) -> Self {
        Self {
            pending: Mutex::default(),
            last: AtomicU64::new(last),
            now,
        }
    }

    pub(crate) fn start(&self) -> Started {
        Started {
            id: self.last.fetch_add(1, Ordering::Relaxed) + 1,
            at: (self.now)(),
            began: Instant::now(),
        }
    }

    /// Hold a run of `tool` on the service keyed `service`, begun at
    /// `started`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &self,
        service: &str,
        started: Started,
        tool: &str,
        source: Source,
        caller: &str,
        outcome: Outcome,
        meter: Option<Meter>,
    ) {
        self.pending.lock().push(Ran {
            id: started.id,
            service: service.to_owned(),
            at: started.at,
            tool: tool.to_owned(),
            source,
            caller: caller.to_owned(),
            outcome,
            ms: started.began.elapsed().as_millis() as u64,
            meter,
        });
    }

    pub(crate) fn take(&self) -> Vec<Ran> {
        mem::take(&mut *self.pending.lock())
    }
}

/// Add `calls`, and drop what is older than [`usage::WINDOW`] days before
/// `now`. Runs of a service this host does not run are dropped.
pub(crate) async fn write(store: &Store, ran: Vec<Ran>, now: u64) -> Result<()> {
    let since = now.saturating_sub(usage::WINDOW as u64 * 86_400_000);
    store
        .run(move |conn| {
            let tx = conn.transaction()?;
            for call in &ran {
                let (scope, name) = host::split(&call.service);
                let meter = call
                    .meter
                    .as_ref()
                    .map(|meter| serde_json::to_string(meter).expect("a meter is JSON"));
                tx.execute(
                    "INSERT INTO service_calls
                            (id, scope, name, at, tool, source, caller, outcome, ms, meter)
                     SELECT ?10, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
                      WHERE EXISTS (SELECT 1 FROM services WHERE scope = ?1 AND name = ?2)",
                    params![
                        scope,
                        name,
                        call.at as i64,
                        call.tool,
                        call.source.as_str(),
                        call.caller,
                        call.outcome.as_str(),
                        call.ms as i64,
                        meter,
                        call.id as i64
                    ],
                )?;
            }
            tx.execute(
                "DELETE FROM service_calls WHERE at < ?1",
                params![since as i64],
            )?;
            tx.commit()
        })
        .await
}

/// `scope/name`'s runs on `page`, newest first, and how many match it in all.
/// A service caller is named by its name.
pub(crate) async fn read(
    store: &Store,
    scope: &str,
    name: &str,
    page: CallPage,
) -> Result<(Vec<Call>, i64)> {
    let (scope, name) = (scope.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            const MATCHING: &str = "FROM service_calls c
                  WHERE c.scope = :scope AND c.name = :name
                    AND (:tool IS NULL OR c.tool = :tool)
                    AND (:outcome IS NULL OR c.outcome = :outcome)
                    AND (:since IS NULL OR c.at >= :since)
                    AND (:until IS NULL OR c.at < :until)
                    AND (:event IS NULL OR EXISTS (
                        SELECT 1 FROM service_logs l, json_each(l.topics) t
                         WHERE l.scope = c.scope AND l.name = c.name AND l.call = c.id
                           AND t.value = :event))";
            let since = page.since.map(|since| since as i64);
            let until = page.until.map(|until| until as i64);
            let matching = named_params! {
                ":scope": scope, ":name": name, ":tool": page.tool, ":event": page.event,
                ":outcome": page.outcome, ":since": since, ":until": until,
            };
            let total =
                conn.query_row(&format!("SELECT COUNT(*) {MATCHING}"), matching, |row| {
                    row.get(0)
                })?;
            let mut statement = conn.prepare(&format!(
                "SELECT {COLUMNS} {MATCHING}
                  ORDER BY c.id DESC LIMIT :limit OFFSET :offset"
            ))?;
            let offset = (page.page - 1).saturating_mul(page.limit);
            let rows = statement.query_map(
                named_params! {
                    ":scope": scope, ":name": name, ":tool": page.tool, ":event": page.event,
                    ":outcome": page.outcome, ":since": since, ":until": until,
                    ":limit": page.limit, ":offset": offset,
                },
                call,
            )?;
            let calls = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((calls, total))
        })
        .await
}

/// The last run of `tool` on `scope/name` that `caller`, a service key, made.
pub(crate) fn last(
    conn: &Connection,
    scope: &str,
    name: &str,
    tool: &str,
    caller: &str,
) -> rusqlite::Result<Option<Call>> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM service_calls c
              WHERE c.scope = ?1 AND c.name = ?2 AND c.tool = ?3 AND c.source = 'service'
                AND c.caller = ?4
              ORDER BY c.id DESC LIMIT 1"
        ),
        params![scope, name, tool, caller],
        call,
    )
    .optional()
}

/// The columns [`call`] reads, in order, of `service_calls c`.
const COLUMNS: &str = "c.id, c.at, c.tool, c.source, c.caller, c.outcome, c.ms, c.meter,
    (SELECT COUNT(*) FROM service_logs l
      WHERE l.scope = c.scope AND l.name = c.name AND l.call = c.id)";

/// A row of [`COLUMNS`] as a call. A service caller is named by its name.
fn call(row: &Row) -> rusqlite::Result<Call> {
    let source: String = row.get(3)?;
    let caller: String = row.get(4)?;
    let caller = match source.as_str() {
        "service" => host::split(&caller).1.to_owned(),
        _ => caller,
    };
    let meter: Option<String> = row.get(7)?;
    Ok(Call {
        id: row.get::<_, i64>(0)? as u64,
        at: row.get::<_, i64>(1)? as u64,
        tool: row.get(2)?,
        source,
        caller,
        outcome: row.get(5)?,
        ms: row.get::<_, i64>(6)? as u64,
        meter: meter.and_then(|meter| serde_json::from_str(&meter).ok()),
        logs: row.get::<_, i64>(8)? as u64,
    })
}

/// The largest call id written, or 0.
pub(crate) async fn newest(store: &Store) -> Result<u64> {
    store
        .run(|conn| {
            conn.query_row(
                "SELECT COALESCE(MAX(id), 0) FROM service_calls",
                [],
                |row| row.get::<_, i64>(0),
            )
        })
        .await
        .map(|last| last as u64)
}
