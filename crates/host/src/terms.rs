//! What each tool's owner set at runtime, its budget, in `service_terms`,
//! held in memory so every invocation can read it.

use crate::{host, store::Store};
use anyhow::Result;
use berm::Budget;
use parking_lot::RwLock;
use rusqlite::params;
use std::collections::{BTreeMap, HashMap};

/// What an owner set for one tool.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Set {
    /// `None` takes the host's.
    pub budget: Option<Budget>,
}

/// Every tool's terms its owner set, by service.
#[derive(Default)]
pub(crate) struct Terms(RwLock<HashMap<String, BTreeMap<String, Set>>>);

impl Terms {
    /// Read every term `store` holds, in place of what was held.
    pub(crate) async fn load(&self, store: &Store) -> Result<()> {
        let rows = store
            .run(|conn| {
                let mut statement = conn
                    .prepare("SELECT scope, name, tool, instructions, rows FROM service_terms")?;
                let rows = statement.query_map([], |row| {
                    let (scope, name): (String, String) = (row.get(0)?, row.get(1)?);
                    let (instructions, rows): (Option<i64>, Option<i64>) =
                        (row.get(3)?, row.get(4)?);
                    Ok((
                        host::join(&scope, &name),
                        row.get::<_, String>(2)?,
                        Set {
                            budget: instructions.zip(rows).map(|(instructions, rows)| Budget {
                                instructions: instructions as u64,
                                rows: rows as u64,
                            }),
                        },
                    ))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?;
        let mut terms: HashMap<String, BTreeMap<String, Set>> = HashMap::new();
        for (service, tool, set) in rows {
            terms.entry(service).or_default().insert(tool, set);
        }
        *self.0.write() = terms;
        Ok(())
    }

    /// What the owner of `service` set for `tool`.
    pub(crate) fn get(&self, service: &str, tool: &str) -> Set {
        self.0
            .read()
            .get(service)
            .and_then(|tools| tools.get(tool))
            .copied()
            .unwrap_or_default()
    }

    /// Set `tool` of `service` to `budget`.
    pub(crate) async fn set_budget(
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
                    "INSERT INTO service_terms (scope, name, tool, instructions, rows)
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
            .entry(tool.to_owned())
            .or_default()
            .budget = Some(budget);
        Ok(())
    }

    /// Forget `service`'s terms; its rows go with the service.
    pub(crate) fn forget(&self, service: &str) {
        self.0.write().remove(service);
    }
}
