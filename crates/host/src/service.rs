//! The `services` table and what a service's calls commit beside it.

use crate::{calls, host, store::Store};
use anyhow::Result;
use berm_api::{Logged, host::Scheduled};
use rusqlite::{OptionalExtension, Transaction, params};

/// One service, as a row.
pub(crate) struct Row {
    pub scope: String,
    pub name: String,
    pub reference: String,
    pub digest: String,
}

/// Every deployed service.
pub(crate) async fn all(store: &Store) -> Result<Vec<Row>> {
    store
        .run(|conn| {
            let mut statement =
                conn.prepare("SELECT scope, name, reference, digest FROM services")?;
            let rows = statement.query_map([], |row| {
                Ok(Row {
                    scope: row.get(0)?,
                    name: row.get(1)?,
                    reference: row.get(2)?,
                    digest: row.get(3)?,
                })
            })?;
            rows.collect()
        })
        .await
}

/// What `scope/name` runs, if it exists.
pub(crate) async fn row(store: &Store, scope: &str, name: &str) -> Result<Option<Row>> {
    let (scope, name) = (scope.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            conn.query_row(
                "SELECT scope, name, reference, digest FROM services
                  WHERE scope = ?1 AND name = ?2",
                params![scope, name],
                |row| {
                    Ok(Row {
                        scope: row.get(0)?,
                        name: row.get(1)?,
                        reference: row.get(2)?,
                        digest: row.get(3)?,
                    })
                },
            )
            .optional()
        })
        .await
}

/// `scope/name`'s commit count, if it exists.
pub(crate) async fn commits(store: &Store, scope: &str, name: &str) -> Result<Option<i64>> {
    let (scope, name) = (scope.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            conn.query_row(
                "SELECT commits FROM services WHERE scope = ?1 AND name = ?2",
                params![scope, name],
                |row| row.get(0),
            )
            .optional()
        })
        .await
}

/// Point `scope/name` at a new version, creating the service on its first
/// deploy, and record the tools `deprecated` in it: each keeps the time it was
/// first deployed deprecated, `now` in epoch seconds for one deprecated by this
/// deploy, and the rest are forgotten with their sunsets.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn install(
    store: &Store,
    scope: &str,
    name: &str,
    reference: &str,
    digest: &str,
    version: &str,
    deprecated: Vec<String>,
    now: i64,
) -> Result<()> {
    let row = [scope, name, reference, digest, version].map(str::to_owned);
    store
        .run(move |conn| {
            let [scope, name, reference, digest, version] = row;
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO services (scope, name, reference, digest, version)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (scope, name) DO UPDATE
                    SET reference = excluded.reference, digest = excluded.digest,
                        version = excluded.version, updated_at = unixepoch()",
                params![scope, name, reference, digest, version],
            )?;
            let kept = serde_json::to_string(&deprecated).unwrap_or_default();
            tx.execute(
                "DELETE FROM service_deprecations
                  WHERE scope = ?1 AND name = ?2
                    AND tool NOT IN (SELECT value FROM json_each(?3))",
                params![scope, name, kept],
            )?;
            for tool in &deprecated {
                tx.execute(
                    "INSERT INTO service_deprecations (scope, name, tool, since)
                     VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
                    params![scope, name, tool, now],
                )?;
            }
            tx.commit()
        })
        .await
}

/// Delete `scope/name` and every row beside it. `false` when there was none.
pub(crate) async fn remove(store: &Store, scope: &str, name: &str) -> Result<bool> {
    let (scope, name) = (scope.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            let removed = conn.execute(
                "DELETE FROM services WHERE scope = ?1 AND name = ?2",
                params![scope, name],
            )?;
            Ok(removed == 1)
        })
        .await
}

/// What `scope/name` last committed under `key`.
pub(crate) async fn read(
    store: &Store,
    scope: &str,
    name: &str,
    key: String,
) -> Result<Option<Vec<u8>>> {
    let (scope, name) = (scope.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            conn.query_row(
                "SELECT value FROM service_state WHERE scope = ?1 AND name = ?2 AND key = ?3",
                params![scope, name, key],
                |row| row.get(0),
            )
            .optional()
        })
        .await
}

/// Commit one service's `writes` inside `tx`: upsert them, and advance its
/// commit count.
pub(crate) fn commit(
    tx: &Transaction<'_>,
    scope: &str,
    name: &str,
    writes: &[(String, Vec<u8>)],
) -> rusqlite::Result<()> {
    tx.execute(
        "UPDATE services SET commits = commits + 1 WHERE scope = ?1 AND name = ?2",
        params![scope, name],
    )?;
    for (key, value) in writes {
        tx.execute(
            "INSERT INTO service_state (scope, name, key, value) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (scope, name, key) DO UPDATE SET value = excluded.value",
            params![scope, name, key, value],
        )?;
    }
    Ok(())
}

/// `scope/name`'s logs after `after`, oldest first, those carrying `topic`
/// and emitted in `call` when given: at most `limit` past the first `offset`,
/// and how many there are in all.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn logs(
    store: &Store,
    scope: &str,
    name: &str,
    topic: Option<String>,
    call: Option<i64>,
    after: i64,
    offset: i64,
    limit: i64,
) -> Result<(Vec<Logged>, i64)> {
    let (scope, name) = (scope.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            const MATCHING: &str = "FROM service_logs
                  WHERE scope = ?1 AND name = ?2 AND seq > ?3
                    AND (?4 IS NULL OR EXISTS (SELECT 1 FROM json_each(topics) WHERE value = ?4))
                    AND (?5 IS NULL OR call = ?5)";
            let total = conn.query_row(
                &format!("SELECT COUNT(*) {MATCHING}"),
                params![scope, name, after, topic, call],
                |row| row.get(0),
            )?;
            let mut statement = conn.prepare(&format!(
                "SELECT seq, call, at, topics, data {MATCHING} ORDER BY seq LIMIT ?6 OFFSET ?7"
            ))?;
            let rows = statement.query_map(
                params![scope, name, after, topic, call, limit, offset],
                |row| {
                    let topics: String = row.get(3)?;
                    Ok(Logged {
                        seq: row.get::<_, i64>(0)? as u64,
                        call: row.get::<_, Option<i64>>(1)?.map(|call| call as u64),
                        at: row.get::<_, i64>(2)? as u64,
                        topics: serde_json::from_str(&topics).unwrap_or_default(),
                        data: row.get(4)?,
                    })
                },
            )?;
            let logs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((logs, total))
        })
        .await
}

/// One pending wake.
pub(crate) struct Wake {
    pub scope: String,
    /// The service that armed it.
    pub name: String,
    pub program: String,
    pub tool: String,
    pub args: String,
    pub armed: i64,
}

/// Hold a wake for `scope/name`, replacing whatever it had pending.
pub(crate) async fn arm(
    store: &Store,
    scope: &str,
    name: &str,
    program: &str,
    tool: &str,
    args: &str,
    due: i64,
) -> Result<()> {
    let row = [scope, name, program, tool, args].map(str::to_owned);
    store
        .run(move |conn| {
            let [scope, name, program, tool, args] = row;
            conn.execute(
                "INSERT INTO service_wakes (scope, name, program, tool, args, due)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (scope, name) DO UPDATE
                    SET program = excluded.program, tool = excluded.tool,
                        args = excluded.args, due = excluded.due,
                        armed = service_wakes.armed + 1, failures = 0",
                params![scope, name, program, tool, args, due],
            )?;
            Ok(())
        })
        .await
}

/// Take up to `limit` wakes due at `now` outside the held scopes, pushing each
/// one's `due` to `lease` so the timer does not take it again while it runs.
pub(crate) async fn claim(store: &Store, now: i64, lease: i64, limit: i64) -> Result<Vec<Wake>> {
    store
        .run(move |conn| {
            let mut statement = conn.prepare(
                "UPDATE service_wakes SET due = ?2
                  WHERE rowid IN (SELECT rowid FROM service_wakes
                                   WHERE due <= ?1
                                     AND scope NOT IN (SELECT scope FROM held_scopes)
                                   ORDER BY due LIMIT ?3)
                  RETURNING scope, name, program, tool, args, armed",
            )?;
            let rows = statement.query_map(params![now, lease, limit], |row| {
                Ok(Wake {
                    scope: row.get(0)?,
                    name: row.get(1)?,
                    program: row.get(2)?,
                    tool: row.get(3)?,
                    args: row.get(4)?,
                    armed: row.get(5)?,
                })
            })?;
            rows.collect()
        })
        .await
}

/// Settle a fired wake: drop it, or with `retry` set run it again then. Left as
/// it is when it was armed again while it ran.
pub(crate) async fn settle(store: &Store, wake: &Wake, retry: Option<i64>) -> Result<()> {
    let (scope, name, armed) = (wake.scope.clone(), wake.name.clone(), wake.armed);
    store
        .run(move |conn| {
            match retry {
                None => conn.execute(
                    "DELETE FROM service_wakes WHERE scope = ?1 AND name = ?2 AND armed = ?3",
                    params![scope, name, armed],
                )?,
                Some(due) => conn.execute(
                    "UPDATE service_wakes SET due = ?4, failures = failures + 1
                      WHERE scope = ?1 AND name = ?2 AND armed = ?3",
                    params![scope, name, armed, due],
                )?,
            };
            Ok(())
        })
        .await
}

/// The pending wakes that call a tool of `scope/program`, soonest first. A
/// wake that is running is due when its lease ends.
pub(crate) async fn scheduled(store: &Store, scope: &str, program: &str) -> Result<Vec<Scheduled>> {
    let (scope, program) = (scope.to_owned(), program.to_owned());
    store
        .run(move |conn| {
            let mut statement = conn.prepare(
                "SELECT tool, args, due, name, failures FROM service_wakes
                  WHERE scope = ?1 AND program = ?2
                  ORDER BY due",
            )?;
            let rows = statement.query_map(params![scope, program], |row| {
                Ok(Scheduled {
                    tool: row.get(0)?,
                    args: row.get(1)?,
                    due: row.get::<_, i64>(2)? as u64,
                    by: row.get(3)?,
                    failures: row.get::<_, i64>(4)? as u64,
                    last: None,
                })
            })?;
            let mut wakes = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            for wake in &mut wakes {
                let armer = host::join(&scope, &wake.by);
                wake.last = calls::last(conn, &scope, &program, &wake.tool, &armer)?;
            }
            Ok(wakes)
        })
        .await
}

/// Every pending wake, beside when it is due, soonest first.
#[cfg(feature = "test")]
pub(crate) async fn wakes(store: &Store) -> Result<Vec<(Wake, i64)>> {
    store
        .run(|conn| {
            let mut statement = conn.prepare(
                "SELECT scope, name, program, tool, args, armed, due FROM service_wakes
                  ORDER BY due",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    Wake {
                        scope: row.get(0)?,
                        name: row.get(1)?,
                        program: row.get(2)?,
                        tool: row.get(3)?,
                        args: row.get(4)?,
                        armed: row.get(5)?,
                    },
                    row.get(6)?,
                ))
            })?;
            rows.collect()
        })
        .await
}

/// When the next wake outside the held scopes is due, if any is pending.
pub(crate) async fn next(store: &Store) -> Result<Option<i64>> {
    store
        .run(|conn| {
            conn.query_row(
                "SELECT MIN(due) FROM service_wakes
                  WHERE scope NOT IN (SELECT scope FROM held_scopes)",
                [],
                |row| row.get(0),
            )
        })
        .await
}
