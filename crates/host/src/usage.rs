//! Who calls each service's tools and takes its events: counted in memory per
//! day and written in batches to `service_usage` on the host the service runs
//! on.

use crate::{host, store::Store};
use anyhow::Result;
use berm::Called;
use berm_api::host::{Usage, Used};
use parking_lot::Mutex;
use rusqlite::{OptionalExtension, params};
use std::{
    collections::HashMap,
    mem,
    time::{SystemTime, UNIX_EPOCH},
};

/// Days of usage a deploy and `/usage` look back over.
pub(crate) const WINDOW: i32 = 30;

/// What was used.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Tool,
    Event,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Tool => "tool",
            Kind::Event => "event",
        }
    }
}

/// Who used it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    /// A service of the same owner, by its key.
    Service,
    /// An account, by its member id.
    Account,
    /// An app a browser called through, by its id.
    App,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Service => "service",
            Source::Account => "account",
            Source::App => "app",
        }
    }
}

/// How a use ended.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Outcome {
    Done,
    Failed,
    /// The tool refused its caller.
    Denied,
    /// The host refused it before the tool ran.
    Refused,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Done => "done",
            Outcome::Failed => "failed",
            Outcome::Denied => "denied",
            Outcome::Refused => "refused",
        }
    }
}

impl From<Called> for Outcome {
    fn from(called: Called) -> Self {
        match called {
            Called::Done => Outcome::Done,
            Called::Failed => Outcome::Failed,
            Called::Denied => Outcome::Denied,
            Called::Refused => Outcome::Refused,
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
struct Key {
    service: String,
    kind: Kind,
    item: String,
    source: Source,
    caller: String,
    outcome: Outcome,
    day: i32,
}

/// Counts not yet written: calls and the last call's epoch seconds.
#[derive(Default)]
pub(crate) struct Counts(Mutex<HashMap<Key, (i64, i64)>>);

impl Counts {
    /// Count one use of `item` on the service keyed `service`, ended as
    /// `outcome`.
    pub(crate) fn record(
        &self,
        service: &str,
        kind: Kind,
        item: &str,
        source: Source,
        caller: &str,
        outcome: Outcome,
    ) {
        let now = now();
        let key = Key {
            service: service.to_owned(),
            kind,
            item: item.to_owned(),
            source,
            caller: caller.to_owned(),
            outcome,
            day: (now / 86_400) as i32,
        };
        let mut pending = self.0.lock();
        let (calls, last) = pending.entry(key).or_default();
        *calls += 1;
        *last = now;
    }

    /// Take what is pending, as rows.
    pub(crate) fn take(&self) -> Vec<Used> {
        mem::take(&mut *self.0.lock())
            .into_iter()
            .map(|(key, (calls, last))| {
                let (owner_id, name) = host::unkey(&key.service);
                Used {
                    owner_id: owner_id.to_owned(),
                    name: name.to_owned(),
                    kind: key.kind.as_str().to_owned(),
                    item: key.item,
                    source: key.source.as_str().to_owned(),
                    caller: key.caller,
                    outcome: key.outcome.as_str().to_owned(),
                    day: key.day,
                    calls,
                    last,
                }
            })
            .collect()
    }
}

/// Add `used` to what is recorded. Uses of a service this host does not run
/// are dropped.
pub(crate) async fn write(store: &Store, used: Vec<Used>) -> Result<()> {
    if used.is_empty() {
        return Ok(());
    }
    store
        .run(move |conn| {
            let tx = conn.transaction()?;
            for used in &used {
                tx.execute(
                    "INSERT INTO service_usage
                            (owner_id, name, kind, item, source, caller, outcome, day, calls, last)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10
                      WHERE EXISTS (SELECT 1 FROM services WHERE owner_id = ?1 AND name = ?2)
                     ON CONFLICT (owner_id, name, kind, item, source, caller, outcome, day)
                     DO UPDATE SET calls = service_usage.calls + excluded.calls,
                                   last = MAX(service_usage.last, excluded.last)",
                    params![
                        used.owner_id,
                        used.name,
                        used.kind,
                        used.item,
                        used.source,
                        used.caller,
                        used.outcome,
                        used.day,
                        used.calls,
                        used.last
                    ],
                )?;
            }
            tx.commit()
        })
        .await
}

/// `owner_id/name`'s usage over the last [`WINDOW`] days, one row per item,
/// caller and outcome, of one item and outcome when `of` names them. A service
/// caller is named by its name, an account by its id.
pub(crate) async fn read(
    store: &Store,
    owner_id: &str,
    name: &str,
    of: Option<(Kind, &str, Outcome)>,
) -> Result<Vec<Usage>> {
    let (kind, item, outcome) = of.map_or((None, None, None), |(kind, item, outcome)| {
        (
            Some(kind.as_str()),
            Some(item.to_owned()),
            Some(outcome.as_str()),
        )
    });
    let (owner_id, name) = (owner_id.to_owned(), name.to_owned());
    let since = (now() / 86_400) as i32 - WINDOW;
    store
        .run(move |conn| {
            let mut statement = conn.prepare(
                "SELECT kind, item, source, caller, outcome, SUM(calls), MAX(last)
                   FROM service_usage
                  WHERE owner_id = ?1 AND name = ?2 AND day > ?3
                    AND (?4 IS NULL OR (kind = ?4 AND item = ?5 AND outcome = ?6))
                  GROUP BY kind, item, source, caller, outcome
                  ORDER BY kind, item, MAX(last) DESC",
            )?;
            let rows = statement.query_map(
                params![owner_id, name, since, kind, item, outcome],
                |row| {
                    let source: String = row.get(2)?;
                    let caller: String = row.get(3)?;
                    let caller = match source.as_str() {
                        "service" => host::unkey(&caller).1.to_owned(),
                        _ => caller,
                    };
                    Ok(Usage {
                        kind: row.get(0)?,
                        item: row.get(1)?,
                        source,
                        caller,
                        outcome: row.get(4)?,
                        calls: row.get(5)?,
                        last: row.get(6)?,
                        public: None,
                    })
                },
            )?;
            rows.collect()
        })
        .await
}

/// When `tool` was first deployed deprecated, and its sunset if one is set.
pub(crate) async fn deprecation(
    store: &Store,
    owner_id: &str,
    name: &str,
    tool: &str,
) -> Result<Option<(i64, Option<i64>)>> {
    let row = [owner_id, name, tool].map(str::to_owned);
    store
        .run(move |conn| {
            let [owner_id, name, tool] = row;
            conn.query_row(
                "SELECT since, sunset FROM service_deprecations
                  WHERE owner_id = ?1 AND name = ?2 AND tool = ?3",
                params![owner_id, name, tool],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
        })
        .await
}

/// Set or clear a deprecated tool's sunset. `false` when the tool is not
/// deprecated.
pub(crate) async fn sunset(
    store: &Store,
    owner_id: &str,
    name: &str,
    tool: &str,
    at: Option<i64>,
) -> Result<bool> {
    let row = [owner_id, name, tool].map(str::to_owned);
    store
        .run(move |conn| {
            let [owner_id, name, tool] = row;
            let set = conn.execute(
                "UPDATE service_deprecations SET sunset = ?4
                  WHERE owner_id = ?1 AND name = ?2 AND tool = ?3",
                params![owner_id, name, tool, at],
            )?;
            Ok(set == 1)
        })
        .await
}

/// Seconds since the Unix epoch.
pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}
