//! berm-registry — an index of published harnesses.
//!
//! A registry holds the bytes; this holds the list, because no registry API
//! will tell you who published a harness. An entry is a reference plus what the
//! image said it was at that digest, so finding a harness never means pulling
//! one, and the index never stores anything a registry could have been asked.
//!
//! The index itself is a git repository. This process holds it in memory and
//! holds the credential to append to it; it owns no other state, so losing it
//! costs a restart.

use anyhow::{Context, Result};
use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;

pub use api::router;
pub use entry::Entry;
pub use store::Store;

mod api;
mod entry;
mod publish;
mod store;

/// Who is publishing, according to whoever mounted [`router`].
///
/// The index authenticates nobody: a service that mounts it validates its own
/// credential and inserts this, the way cloud already hands `sync` and `agent`
/// their callers. Absent means the index is running open, which is what the
/// standalone binary does.
#[derive(Clone)]
pub struct Caller(pub String);

pub struct Index {
    /// Harness repository — `ghcr.io/clearloop/fs` — to its versions, in the
    /// order they were published.
    entries: RwLock<BTreeMap<String, Vec<Entry>>>,
    store: Store,
}

impl Index {
    /// Read the index once. There is no other load: a restart is the reload,
    /// which is what lets this process own nothing worth backing up.
    pub async fn new(store: Store) -> Result<Arc<Self>> {
        let entries = store.load().await.context("failed to read the index")?;
        tracing::info!(harnesses = entries.len(), "index loaded");
        Ok(Arc::new(Self {
            entries: RwLock::new(entries),
            store,
        }))
    }

    pub async fn serve(self: Arc<Self>, addr: SocketAddr) -> Result<()> {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("failed to bind {addr}"))?;

        tracing::info!("listening on http://{addr}");
        axum::serve(listener, api::router(self))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .context("server failed")
    }

    /// Everything matching `term`, or everything when it is empty.
    pub async fn search(&self, term: &str) -> Vec<Entry> {
        self.entries
            .read()
            .await
            .values()
            .flatten()
            .filter(|entry| term.is_empty() || entry.matches(term))
            .cloned()
            .collect()
    }

    /// One harness's versions, oldest first.
    pub async fn versions(&self, key: &str) -> Option<Vec<Entry>> {
        self.entries.read().await.get(key).cloned()
    }
}
