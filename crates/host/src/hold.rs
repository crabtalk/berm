//! The scopes this host holds, as its daemon last set them: a held scope's
//! services fire no wakes and take no calls from services.

use crate::store::Store;
use anyhow::Result;
use parking_lot::RwLock;
use rusqlite::params;
use std::collections::BTreeSet;

pub(crate) struct Holds {
    store: Store,
    held: RwLock<BTreeSet<String>>,
}

impl Holds {
    /// The scopes `store` holds from the last time they were set.
    pub(crate) async fn open(store: Store) -> Result<Self> {
        let held = store
            .run(|conn| {
                let mut statement = conn.prepare("SELECT scope FROM held_scopes")?;
                let rows = statement.query_map([], |row| row.get(0))?;
                rows.collect::<rusqlite::Result<BTreeSet<String>>>()
            })
            .await?;
        Ok(Self {
            store,
            held: RwLock::new(held),
        })
    }

    /// Whether `scope` is held.
    pub(crate) fn holds(&self, scope: &str) -> bool {
        self.held.read().contains(scope)
    }

    /// Every held scope.
    pub(crate) fn list(&self) -> Vec<String> {
        self.held.read().iter().cloned().collect()
    }

    /// Hold `scopes` in place of what was held. Answers the scopes released.
    pub(crate) async fn replace(&self, scopes: BTreeSet<String>) -> Result<Vec<String>> {
        let rows: Vec<String> = scopes.iter().cloned().collect();
        self.store
            .run(move |conn| {
                let tx = conn.transaction()?;
                tx.execute("DELETE FROM held_scopes", [])?;
                for scope in &rows {
                    tx.execute(
                        "INSERT INTO held_scopes (scope) VALUES (?1)",
                        params![scope],
                    )?;
                }
                tx.commit()
            })
            .await?;
        let mut held = self.held.write();
        let released = held.difference(&scopes).cloned().collect();
        *held = scopes;
        Ok(released)
    }
}
