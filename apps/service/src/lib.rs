//! bermd — deploys programs and serves their tools over MCP.
//!
//! A thin daemon over [`Host`]: one local owner owns every program, and one
//! local account calls them. Every deployed program appears on one MCP
//! endpoint, its tools namespaced `{program}.{tool}`.

use anyhow::{Context, Result};
use berm::Program;
use berm_host::{Config, Host};
use std::{ops::Deref, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, sync::broadcast};

pub use env::Local;
pub use fetch::Fetch;
pub use images::Stored;
pub use peers::Alone;
pub use secrets::Grants;

mod api;
mod deps;
mod env;
mod fetch;
mod images;
mod mcp;
mod peers;
mod program;
mod secrets;

/// How many deploys may go unread by a session before it misses one. A missed
/// notification costs a stale tool list until the next change, not a wrong one.
const CHANGE_BACKLOG: usize = 16;

/// The owner every program is deployed under, and the account every call is
/// made as.
pub const LOCAL: &str = "local";

pub struct Service {
    host: Host<Local>,
    /// Where uploaded images are kept for the host to pull.
    images: Stored,
    /// What each program may have substituted into its requests.
    secrets: Grants,
    /// Fires when the deployed set changes. Connected MCP sessions turn this
    /// into `notifications/tools/list_changed`.
    changed: broadcast::Sender<()>,
}

/// A deployed program, by the name it is deployed under.
#[derive(Clone)]
pub struct Deployed {
    pub name: Arc<str>,
    pub program: Arc<Program>,
}

impl Deref for Deployed {
    type Target = Program;

    fn deref(&self) -> &Program {
        &self.program
    }
}

impl Service {
    /// Open `root`, restoring whatever was deployed before this process.
    /// Chains of programs calling programs nest at most `depth` deep.
    pub async fn new(root: PathBuf, depth: u32) -> Result<Arc<Self>> {
        let images = Stored::open(&root);
        let secrets = Grants::open(&root);
        let config = Config {
            depth,
            cache_dir: Some(root.join("cache")),
            databases: root.join("databases"),
            ..Config::default()
        };
        let host = Host::open(
            &root.join("host.db"),
            &config,
            Fetch::new()?,
            images.clone(),
            Alone,
            secrets.clone(),
        )
        .await
        .context("failed to open the host")?;
        host.boot();
        Ok(Arc::new(Self {
            host,
            images,
            secrets,
            changed: broadcast::channel(CHANGE_BACKLOG).0,
        }))
    }

    /// Serve on an already-bound listener until `shutdown` resolves.
    ///
    /// Binding is the caller's because its failure — the port is taken, by a
    /// second berm holding the same root — is the one startup error that has to
    /// be known *before* anything claims to be serving.
    ///
    /// Shutdown is the caller's: waiting on `SIGINT` here would install a
    /// handler over the whole process, and tokio never puts the default back.
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

    /// Every deployed program, by name, as a snapshot the caller can hold
    /// across awaits.
    pub fn list(&self) -> Vec<Deployed> {
        let mut deployed: Vec<Deployed> = self
            .host
            .list(LOCAL)
            .into_iter()
            .map(|(name, program)| Deployed {
                name: name.into(),
                program,
            })
            .collect();
        deployed.sort_by(|a, b| a.name.cmp(&b.name));
        deployed
    }

    pub fn get(&self, name: &str) -> Option<Deployed> {
        let program = self.host.get(LOCAL, name)?;
        Some(Deployed {
            name: name.into(),
            program,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }
}
