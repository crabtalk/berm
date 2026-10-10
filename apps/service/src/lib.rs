//! bermd — deploys programs and serves their tools over MCP.
//!
//! An invocation is ephemeral: berm instantiates a program per call and nothing
//! survives it. What the service exists to hold is everything around that — the
//! deployed set, the modules compiled from it, and the engine's code cache. A
//! deployed program is therefore compiled once, at deploy, and every later
//! invocation pays only instantiation.
//!
//! Every deployed program appears on one MCP endpoint, its tools namespaced
//! `{program}.{tool}`.

use anyhow::{Context, Result};
use berm::{Berm, Calls, Config, Engine, Program};
use std::{
    path::PathBuf,
    sync::{Arc, Weak},
};
use tokio::{
    net::TcpListener,
    runtime::Handle,
    sync::{Mutex, broadcast},
};

mod api;
mod deliver;
mod deps;
mod files;
mod http;
mod logs;
mod mcp;
mod program;
mod secrets;
mod source;
mod store;
mod timer;
mod utils;

/// How many deploys may go unread by a session before it misses one. A missed
/// notification costs a stale tool list until the next change, not a wrong one.
const CHANGE_BACKLOG: usize = 16;

pub struct Service {
    /// The programs this service is running, and the records behind them.
    /// berm holds both; what is left here is everything it has no opinion
    /// about — who is told when the set changes.
    berm: Arc<Berm>,
    /// Fires when the deployed set changes. Connected MCP sessions turn this
    /// into `notifications/tools/list_changed`, because the tool set mutates
    /// under clients that are already holding a list.
    changed: broadcast::Sender<()>,
    /// What each program has asked to be called back for.
    pub(crate) wakes: timer::Wakes,
    /// What each program may have substituted into its requests.
    pub(crate) secrets: secrets::Secrets,
    /// Where each chain's writes and logs are committed.
    root: PathBuf,
    /// Held for one delivery pass, so a subscriber is never handed the same
    /// logs by two at once.
    delivering: Mutex<()>,
    /// This service, as a delivery pass outliving the call that started it
    /// holds it.
    me: Weak<Self>,
}

impl Service {
    /// Open `root`, restoring whatever was deployed before this process.
    pub async fn new(root: PathBuf, depth: u32) -> Result<Arc<Self>> {
        let engine = Engine::new(&Config {
            cache_dir: Some(root.join("cache")),
            ..Config::default()
        })
        .context("failed to start the compiler")?;

        // Cyclic for the reason `Berm` is: the timer door reaches back into
        // the service holding the table it writes to.
        let runtime = Handle::current();
        let secrets = secrets::Secrets::open(&root);
        let fetch = http::syscall(secrets.clone())?;
        let service = Arc::new_cyclic(|me| {
            let mut syscalls = store::syscalls(&root);
            syscalls.push(timer::syscalls(me.clone(), runtime));
            syscalls.push(fetch);
            Self {
                berm: Berm::new(
                    &engine,
                    Calls::by_name(depth),
                    syscalls,
                    Arc::new(files::Files::open(&root)),
                ),
                changed: broadcast::channel(CHANGE_BACKLOG).0,
                wakes: timer::Wakes::default(),
                secrets,
                root: root.clone(),
                delivering: Mutex::new(()),
                me: me.clone(),
            }
        });
        // Images first: a wake names a program, which has to be deployed
        // before it fires.
        let restoring = service.clone();
        tokio::task::spawn_blocking(move || restoring.berm.restore())
            .await
            .context("restoring panicked")??;
        service.rearm().await?;
        service.deliver();
        Ok(service)
    }

    /// Serve on an already-bound listener until `shutdown` resolves.
    ///
    /// Binding is the caller's because its failure — the port is taken, by a
    /// second berm holding the same root — is the one startup error that has to
    /// be known *before* anything claims to be serving.
    ///
    /// Shutdown is the caller's for a sharper reason: waiting on `SIGINT` here
    /// would install a handler over the whole process, and tokio never puts the
    /// default back. A daemon wants exactly that; the workbench embedding this
    /// serves a window too, and would have had its endpoint quietly torn down
    /// by a Ctrl+C its window never heard about.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> Result<()> {
        axum::serve(listener, api::router(self))
            .with_graceful_shutdown(shutdown)
            .await
            .context("server failed")
    }

    /// Every deployed program, as a snapshot the caller can hold across awaits.
    pub fn list(&self) -> Vec<Arc<Program>> {
        self.berm.list()
    }

    pub fn get(&self, name: &str) -> Option<Arc<Program>> {
        self.berm.get(name)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }
}
