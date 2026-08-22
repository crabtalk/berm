//! bermd — deploys harnesses and serves their tools over MCP.
//!
//! An invocation is ephemeral: berm instantiates a harness per call and nothing
//! survives it. What the service exists to hold is everything around that — the
//! deployed set, the modules compiled from it, and the engine's code cache. A
//! deployed harness is therefore compiled once, at deploy, and every later
//! invocation pays only instantiation.
//!
//! Every deployed harness appears on one MCP endpoint, its tools namespaced
//! `{harness}.{tool}`.

use anyhow::{Context, Result};
use berm::{Config, Engine};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::{RwLock, broadcast},
};

pub use harness::Deployed;

mod api;
mod harness;
mod mcp;

/// How many deploys may go unread by a session before it misses one. A missed
/// notification costs a stale tool list until the next change, not a wrong one.
const CHANGE_BACKLOG: usize = 16;

pub struct Service {
    root: PathBuf,
    engine: Engine,
    deployed: RwLock<BTreeMap<String, Arc<Deployed>>>,
    /// Fires when the deployed set changes. Connected MCP sessions turn this
    /// into `notifications/tools/list_changed`, because the tool set mutates
    /// under clients that are already holding a list.
    changed: broadcast::Sender<()>,
}

impl Service {
    /// Open `root`, restoring whatever was deployed before this process.
    pub async fn new(root: PathBuf) -> Result<Arc<Self>> {
        let mut config = Config::new();
        config.cache_dir(root.join("cache"));

        let service = Arc::new(Self {
            engine: Engine::new(&config).context("failed to start the compiler")?,
            deployed: RwLock::new(BTreeMap::new()),
            changed: broadcast::channel(CHANGE_BACKLOG).0,
            root,
        });
        service.restore().await?;
        Ok(service)
    }

    /// Serve on an already-bound listener.
    ///
    /// Binding is the caller's because its failure — the port is taken, by a
    /// second berm holding the same root — is the one startup error that has to
    /// be known *before* anything claims to be serving.
    pub async fn serve(self: Arc<Self>, listener: TcpListener) -> Result<()> {
        axum::serve(listener, api::router(self))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .context("server failed")
    }

    /// Every deployed harness, as a snapshot the caller can hold across awaits.
    pub async fn list(&self) -> Vec<Arc<Deployed>> {
        self.deployed.read().await.values().cloned().collect()
    }

    pub async fn get(&self, name: &str) -> Option<Arc<Deployed>> {
        self.deployed.read().await.get(name).cloned()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }
}
