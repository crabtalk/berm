//! Who may call a service's public tools, as its owner set at runtime:
//! `service_callers`, held in memory so the resolver can read it.

use crate::{host::unkey, store::Store};
use anyhow::Result;
use berm_api::host::Allowed;
use parking_lot::RwLock;
use rusqlite::params;
use std::collections::HashMap;

/// Every tool's allowlist, by service key.
#[derive(Default)]
pub(crate) struct Allowlists(RwLock<HashMap<String, Vec<Allowed>>>);

impl Allowlists {
    /// Whether `kind`/`caller` may call `tool` of the service keyed `key`.
    /// A tool with no rule, and no rule for `*`, takes everyone.
    pub(crate) fn allows(&self, key: &str, tool: &str, kind: &str, caller: &str) -> bool {
        let lists = self.0.read();
        let Some(rules) = lists.get(key) else {
            return true;
        };
        let governing: Vec<&Allowed> = match rules.iter().any(|rule| rule.tool == tool) {
            true => rules.iter().filter(|rule| rule.tool == tool).collect(),
            false => rules.iter().filter(|rule| rule.tool == "*").collect(),
        };
        governing.is_empty()
            || governing
                .iter()
                .any(|rule| rule.kind == kind && rule.caller == caller)
    }

    /// Read the service keyed `key` again.
    pub(crate) async fn load(&self, store: &Store, key: &str) -> Result<()> {
        let (owner_id, name) = unkey(key);
        let rules = read(store, owner_id, name).await?;
        let mut lists = self.0.write();
        match rules.is_empty() {
            true => lists.remove(key),
            false => lists.insert(key.to_owned(), rules),
        };
        Ok(())
    }
}

/// `owner_id/name`'s rules.
pub(crate) async fn read(store: &Store, owner_id: &str, name: &str) -> Result<Vec<Allowed>> {
    let (owner_id, name) = (owner_id.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            let mut statement = conn.prepare(
                "SELECT tool, kind, caller FROM service_callers
                  WHERE owner_id = ?1 AND name = ?2 ORDER BY tool, kind, caller",
            )?;
            let rows = statement.query_map(params![owner_id, name], |row| {
                Ok(Allowed {
                    tool: row.get(0)?,
                    kind: row.get(1)?,
                    caller: row.get(2)?,
                })
            })?;
            rows.collect()
        })
        .await
}

/// Replace `owner_id/name`'s rules.
pub(crate) async fn replace(
    store: &Store,
    owner_id: &str,
    name: &str,
    rules: Vec<Allowed>,
) -> Result<()> {
    let (owner_id, name) = (owner_id.to_owned(), name.to_owned());
    store
        .run(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM service_callers WHERE owner_id = ?1 AND name = ?2",
                params![owner_id, name],
            )?;
            for rule in &rules {
                tx.execute(
                    "INSERT INTO service_callers (owner_id, name, tool, kind, caller)
                     VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT DO NOTHING",
                    params![owner_id, name, rule.tool, rule.kind, rule.caller],
                )?;
            }
            tx.commit()
        })
        .await
}
