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
use berm::{Berm, Config, Engine, Harness};
use std::{path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, runtime::Handle, sync::broadcast};

mod api;
mod deps;
mod files;
mod harness;
mod mcp;
mod socket;
mod source;
mod store;
mod timer;
mod utils;

pub use socket::Policy;

/// How many deploys may go unread by a session before it misses one. A missed
/// notification costs a stale tool list until the next change, not a wrong one.
const CHANGE_BACKLOG: usize = 16;

pub struct Service {
    /// The harnesses this service is running, and the records behind them.
    /// berm holds both; what is left here is everything it has no opinion
    /// about — who is told when the set changes, and what a guest may dial.
    berm: Arc<Berm>,
    /// Fires when the deployed set changes. Connected MCP sessions turn this
    /// into `notifications/tools/list_changed`, because the tool set mutates
    /// under clients that are already holding a list.
    changed: broadcast::Sender<()>,
    /// Connections harnesses have open, and the source of every invocation
    /// they start.
    pub(crate) sockets: socket::Sockets,
    /// What a guest may reach the network for, held here because reopening a
    /// connection after a restart reads the same bounds the dial did.
    pub(crate) policy: Policy,
    /// What each harness has asked to be called back for.
    pub(crate) wakes: timer::Wakes,
}

impl Service {
    /// Open `root`, restoring whatever was deployed before this process.
    pub async fn new(root: PathBuf, depth: u32, policy: Policy) -> Result<Arc<Self>> {
        let mut config = Config::new();
        config.cache_dir(root.join("cache"));
        let engine = Engine::new(&config).context("failed to start the compiler")?;

        // Cyclic for the reason `Berm` is: the socket doors reach back into
        // the service holding the table they write to.
        let runtime = Handle::current();
        let service = Arc::new_cyclic(|me| {
            let mut system = store::system(&root);
            system.extend(socket::system(me.clone(), runtime.clone()));
            system.push(timer::system(me.clone(), runtime));
            Self {
                berm: Berm::new(&engine, depth, system, Arc::new(files::Files::open(&root))),
                changed: broadcast::channel(CHANGE_BACKLOG).0,
                sockets: socket::Sockets::default(),
                wakes: timer::Wakes::default(),
                policy,
            }
        });
        // Images first: a connection's events name a harness, which has to be
        // deployed before the first frame lands on it.
        let restoring = service.clone();
        tokio::task::spawn_blocking(move || restoring.berm.restore())
            .await
            .context("restoring panicked")??;
        service.reopen().await?;
        service.rearm().await?;
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
    pub fn list(&self) -> Vec<Arc<Harness>> {
        self.berm.list()
    }

    pub fn get(&self, name: &str) -> Option<Arc<Harness>> {
        self.berm.get(name)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }
}
