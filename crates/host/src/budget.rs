//! What each tool's invocations may use, as its owner set it at runtime:
//! `service_budgets`, held in memory so every invocation can read it.

use crate::{host, store::Store};
use anyhow::Result;
use berm::Budget;
use parking_lot::RwLock;
use rusqlite::params;
use std::collections::{BTreeMap, HashMap};

/// Every tool's budget its owner set, by service.
#[derive(Default)]
pub(crate) struct Budgets(RwLock<HashMap<String, BTreeMap<String, Budget>>>);

impl Budgets {
    /// Read every budget `store` holds, in place of what was held.
    pub(crate) async fn load(&self, store: &Store) -> Result<()> {
        let rows = store
            .run(|conn| {
                let mut statement = conn
                    .prepare("SELECT scope, name, tool, instructions, rows FROM service_budgets")?;
                let rows = statement.query_map([], |row| {
                    let (scope, name): (String, String) = (row.get(0)?, row.get(1)?);
                    Ok((
                        host::join(&scope, &name),
                        row.get::<_, String>(2)?,
                        Budget {
                            instructions: row.get::<_, i64>(3)? as u64,
                            rows: row.get::<_, i64>(4)? as u64,
                        },
                    ))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?;
        let mut budgets: HashMap<String, BTreeMap<String, Budget>> = HashMap::new();
        for (service, tool, budget) in rows {
            budgets.entry(service).or_default().insert(tool, budget);
        }
        *self.0.write() = budgets;
        Ok(())
    }

    /// The budget `tool` of `service` was set, if it was.
    pub(crate) fn get(&self, service: &str, tool: &str) -> Option<Budget> {
        self.0.read().get(service)?.get(tool).copied()
    }

    /// Set `tool` of `service` to `budget`.
    pub(crate) async fn set(
        &self,
        store: &Store,
        service: &str,
        tool: &str,
        budget: Budget,
    ) -> Result<()> {
        let (scope, name) = host::split(service);
        let row = [scope, name, tool].map(str::to_owned);
        store
            .run(move |conn| {
                let [scope, name, tool] = row;
                conn.execute(
                    "INSERT INTO service_budgets (scope, name, tool, instructions, rows)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT (scope, name, tool) DO UPDATE
                        SET instructions = excluded.instructions, rows = excluded.rows",
                    params![
                        scope,
                        name,
                        tool,
                        budget.instructions as i64,
                        budget.rows as i64
                    ],
                )?;
                Ok(())
            })
            .await?;
        self.0
            .write()
            .entry(service.to_owned())
            .or_default()
            .insert(tool.to_owned(), budget);
        Ok(())
    }

    /// Forget `service`'s budgets; its rows go with the service.
    pub(crate) fn forget(&self, service: &str) {
        self.0.write().remove(service);
    }
}
