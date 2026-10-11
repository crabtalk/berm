//! Where each service this host does not run lives, as its daemon last set it.

use crate::{host, store::Store};
use anyhow::Result;
use berm_api::host::Route;
use parking_lot::RwLock;
use rusqlite::params;
use std::collections::HashMap;

/// Each service's peer, by service key.
pub(crate) struct Routes {
    store: Store,
    table: RwLock<HashMap<String, String>>,
}

impl Routes {
    /// The routes `store` holds from the last time they were set.
    pub(crate) async fn open(store: Store) -> Result<Self> {
        let table = store
            .run(|conn| {
                let mut statement = conn.prepare("SELECT owner_id, name, peer FROM routes")?;
                let rows = statement.query_map([], |row| {
                    let (owner_id, name): (String, String) = (row.get(0)?, row.get(1)?);
                    Ok((host::key(&owner_id, &name), row.get(2)?))
                })?;
                rows.collect::<rusqlite::Result<HashMap<String, String>>>()
            })
            .await?;
        Ok(Self {
            store,
            table: RwLock::new(table),
        })
    }

    /// The peer the service keyed `key` runs on.
    pub(crate) fn peer(&self, key: &str) -> Option<String> {
        self.table.read().get(key).cloned()
    }

    /// Hold `routes` in place of what was held.
    pub(crate) async fn replace(&self, routes: Vec<Route>) -> Result<()> {
        let table = routes
            .iter()
            .map(|route| (host::key(&route.owner_id, &route.name), route.peer.clone()))
            .collect();
        self.store
            .run(move |conn| {
                let tx = conn.transaction()?;
                tx.execute("DELETE FROM routes", [])?;
                for route in &routes {
                    tx.execute(
                        "INSERT INTO routes (owner_id, name, peer) VALUES (?1, ?2, ?3)",
                        params![route.owner_id, route.name, route.peer],
                    )?;
                }
                tx.commit()
            })
            .await?;
        *self.table.write() = table;
        Ok(())
    }
}
