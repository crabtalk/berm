//! The services this host runs: one [`Berm`] holding every deployed service,
//! under [`key`].

use crate::{
    Denied,
    callers::Allowlists,
    commit::{self, Commits},
    config::Config,
    deliver::Deliveries,
    deny,
    env::{Env, Images, Peers},
    fetch,
    limit::{Counter, Limits},
    routes::Routes,
    service,
    store::Store,
    usage::{self, Counts, Kind, Outcome, Source},
    wake,
};
use anyhow::{Context, Result, bail};
use axum::http::StatusCode;
use berm::{
    Berm, Called, Calls, Callsite, Ended, Engine, Log, Program, Records, Storage, User,
    database::Databases,
    syscall::{
        emit,
        sql::{self, Statement},
        store as state,
    },
};
use berm_api::{
    Deps, Manifest, Surface,
    host::{self, Deploy, Reply, Used},
};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, OnceLock, Weak},
    time::Instant,
};
use tokio::sync::{Notify, Semaphore};

/// Every service, compiled and callable.
pub struct Host<E: Env> {
    pub(crate) store: Store,
    pub(crate) berm: Arc<Berm>,
    /// Chains in flight. Each holds up to `depth + 1` guests, and a guest past
    /// `Config::instances` fails to instantiate.
    pub(crate) gate: Arc<Semaphore>,
    /// Each subscription's delivery.
    pub(crate) deliveries: Arc<Deliveries>,
    /// Wakes the timer.
    pub(crate) waking: Arc<Notify>,
    /// Uses not yet written.
    pub(crate) usage: Arc<Counts>,
    /// The databases of the services that keep one.
    databases: Arc<Databases>,
    /// Where a chain's changes are committed.
    pub(crate) commits: Commits,
    /// Who may call each service's public tools, as its owner set.
    pub(crate) allowlists: Arc<Allowlists>,
    pub(crate) limits: Limits,
    /// This minute's calls per caller.
    pub(crate) counter: Arc<Counter>,
    /// Where the services this host does not run live.
    pub(crate) routes: Arc<Routes>,
    pub(crate) peers: Arc<E::Peers>,
    images: Arc<E::Images>,
    /// Held by a deploy of the service keyed by its key.
    deploying: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl<E: Env> Clone for Host<E> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            berm: self.berm.clone(),
            gate: self.gate.clone(),
            deliveries: self.deliveries.clone(),
            waking: self.waking.clone(),
            usage: self.usage.clone(),
            databases: self.databases.clone(),
            commits: self.commits.clone(),
            allowlists: self.allowlists.clone(),
            limits: self.limits,
            counter: self.counter.clone(),
            routes: self.routes.clone(),
            peers: self.peers.clone(),
            images: self.images.clone(),
            deploying: self.deploying.clone(),
        }
    }
}

impl<E: Env> Host<E> {
    /// A host with nothing deployed, keeping its state in `state` and sized by
    /// `config`. It serves `berm.get` and `berm.set` over `service_state` or
    /// the service's own database, `berm.call.after` over `service_wakes`,
    /// `berm.sql` over the databases kept under `config.databases`, and
    /// `berm.http.fetch` through `fetch`, with `secrets`. Images come from
    /// `images`, and a service it does not run is reached through `peers`.
    pub async fn open(
        state: &Path,
        config: &Config,
        fetch: E::Fetch,
        images: E::Images,
        peers: E::Peers,
        secrets: E::Secrets,
    ) -> Result<Self> {
        let store = Store::open(state)?;
        let routes = Arc::new(Routes::open(store.clone()).await?);
        let peers = Arc::new(peers);
        let engine = Engine::new(&berm::Config {
            cache_dir: config.cache_dir.clone(),
            instances: config.instances,
            ..Default::default()
        })?;
        let limits = Limits {
            caller: config.rpm,
            anonymous: config.anonymous_rpm,
        };
        let mut syscalls = vec![fetch::syscall::<E>(fetch, Arc::new(secrets))];
        let runtime: Arc<OnceLock<Weak<Berm>>> = Arc::new(OnceLock::new());
        let databases = Arc::new(Databases::new(config.databases.clone()));
        let deployed = {
            let runtime = runtime.clone();
            move |program: &str| {
                runtime
                    .get()
                    .and_then(Weak::upgrade)
                    .and_then(|berm| berm.get(program))
            }
        };
        let kept = {
            let (databases, deployed) = (databases.clone(), deployed.clone());
            move |program: &str| {
                deployed(program)
                    .filter(|program| keeps(program))
                    .map(|_| databases.get(program))
            }
        };
        let (reading, writing) = (kept.clone(), kept.clone());
        let store_reading = store.clone();
        syscalls.extend(state::programs(
            move |at: Callsite, key: String| {
                let (store, kept) = (store_reading.clone(), reading(&at.program));
                async move {
                    if let Some(database) = kept {
                        return database?.get(at.chain, key).await;
                    }
                    let (owner_id, name) = unkey(&at.program);
                    service::read(&store, owner_id, name, key).await
                }
            },
            move |at: Callsite, key: String, value: Vec<u8>| {
                let kept = writing(&at.program);
                async move {
                    let Some(database) = kept else {
                        return Ok(false);
                    };
                    database?.set(at.chain, key, value).await?;
                    Ok(true)
                }
            },
        ));
        let emitting = kept.clone();
        syscalls.push(emit::programs(move |at: Callsite, log: Log| {
            let kept = emitting(&at.program);
            async move {
                let Some(database) = kept else {
                    return Ok(false);
                };
                database?.emit(at.chain, log).await?;
                Ok(true)
            }
        }));
        syscalls.push(sql::programs(move |at: Callsite, statement: Statement| {
            let kept = kept(&at.program);
            async move {
                let Some(database) = kept else {
                    bail!("this service keeps no database");
                };
                let database = database?;
                match at.query {
                    true => database.read(at.chain, statement).await,
                    false => database.exec(at.chain, statement).await,
                }
            }
        }));
        let waking = Arc::new(Notify::new());
        syscalls.push(wake::syscall(
            store.clone(),
            runtime.clone(),
            routes.clone(),
            waking.clone(),
        ));
        let deliveries = Arc::new(Deliveries::default());
        let commits = Commits::new(store.clone(), databases.clone(), deliveries.clone());
        let usage = Arc::new(Counts::default());
        let allowlists = Arc::new(Allowlists::default());
        let (counting, allowing, resolving) = (usage.clone(), allowlists.clone(), deployed.clone());
        let (reporting, reported) = (usage.clone(), deployed);
        let (reaching, routing) = (peers.clone(), routes.clone());
        let calls = Calls {
            depth: config.depth,
            // A service on a peer checks its own allowlist.
            resolve: Arc::new(move |at: &Callsite, target: &str, tool: &str| {
                let reached = resolve(at, target)?;
                if resolving(&reached).is_some()
                    && !allowing.allows(&reached, tool, "service", unkey(&at.program).1)
                {
                    counting.record(
                        &reached,
                        Kind::Tool,
                        tool,
                        Source::Service,
                        &at.program,
                        Outcome::Refused,
                    );
                    bail!("{target}.{tool} does not take calls from this service");
                }
                Ok(reached)
            }),
            // A call to a service on a peer is counted there.
            called: Arc::new(
                move |at: &Callsite, deployed: &str, tool: &str, called: Called| {
                    if reported(deployed).is_none() {
                        return;
                    }
                    reporting.record(
                        deployed,
                        Kind::Tool,
                        tool,
                        Source::Service,
                        &at.program,
                        called.into(),
                    );
                },
            ),
            // A callee is told the calling service's name; it is of the same
            // owner.
            named: Arc::new(|program: &str| unkey(program).1.to_owned()),
            enter: commit::enter(commits.clone()),
            remote: Arc::new(move |at, deployed, tool, args, link| {
                let peer = routing.peer(deployed)?;
                let (peers, from) = (reaching.clone(), at.program.to_string());
                let (owner_id, name) = unkey(deployed);
                let (owner_id, name, tool) =
                    (owner_id.to_owned(), name.to_owned(), tool.to_owned());
                let link = host::Link {
                    caller: link.caller.map(|caller| match caller {
                        berm::Caller::Outside(name) | berm::Caller::Program(name) => {
                            name.to_string()
                        }
                    }),
                    from,
                    user: link.user.map(|user| host::User {
                        issuer: user.issuer.to_string(),
                        subject: user.subject.to_string(),
                    }),
                    depth: link.depth,
                    query: link.query,
                    timeout: link
                        .deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis() as u64,
                    stack: link
                        .stack
                        .iter()
                        .map(|program| program.to_string())
                        .collect(),
                };
                Some(Box::pin(async move {
                    let Reply { result, denied } = peers
                        .link(&peer, &owner_id, &name, &tool, args, link)
                        .await?;
                    Ok(Ended { result, denied })
                }))
            }),
        };
        let chains = (config.instances / (config.depth + 1)).max(1);
        let berm = Berm::new(&engine, calls, syscalls, Arc::new(Unrecorded));
        let _ = runtime.set(Arc::downgrade(&berm));
        Ok(Self {
            store,
            berm,
            gate: Arc::new(Semaphore::new(chains as usize)),
            deliveries,
            waking,
            usage,
            databases,
            commits,
            allowlists,
            limits,
            counter: Arc::new(Counter::default()),
            routes,
            peers,
            images: Arc::new(images),
            deploying: Arc::default(),
        })
    }

    /// Load every service this host runs off the request path, deliver what
    /// subscribers have not had, fire due wakes, and write counted uses.
    pub fn boot(&self) {
        let flushing = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(FLUSH).await;
                if let Err(e) = flushing.flush().await {
                    tracing::error!("writing service usage failed: {e:#}");
                }
            }
        });
        let loading = self.clone();
        tokio::spawn(async move { loading.reconcile().await });
        let waking = self.clone();
        tokio::spawn(async move { waking.wakes().await });
    }

    /// Load every service as `services` names it, copy what each database's
    /// outbox holds, then deliver.
    async fn reconcile(&self) {
        let rows = match service::all(&self.store).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("reading services failed: {e:#}");
                return;
            }
        };
        for row in rows {
            let key = key(&row.owner_id, &row.name);
            if let Err(e) = self.allowlists.load(&self.store, &key).await {
                tracing::error!(service = key, "reading callers failed: {e:#}");
            }
            match self.load(row).await {
                Ok(program) => {
                    tracing::debug!(service = key, digest = %program.digest, "loaded");
                    if keeps(&program)
                        && let Err(e) = self.commits.ship(&key).await
                    {
                        tracing::error!(service = key, "copying the outbox failed: {e:#}");
                    }
                }
                Err(e) => tracing::error!(service = key, "loading failed: {e:#}"),
            }
        }
        self.subscribe();
    }

    /// The program a `services` row names, pulled and compiled unless it
    /// already runs that digest.
    async fn load(&self, row: service::Row) -> Result<Arc<Program>> {
        let key = key(&row.owner_id, &row.name);
        if let Some(program) = self.berm.get(&key)
            && row.digest.strip_prefix("sha256:") == Some(program.digest.as_str())
        {
            return Ok(program);
        }
        let image = self.images.pull(&row.reference, &row.digest).await?;
        self.compile(key, image).await
    }

    /// Write what was counted: uses of services this host runs here, and the
    /// rest to the peer each runs on.
    pub(crate) async fn flush(&self) -> Result<()> {
        let mut local = Vec::new();
        let mut remote: HashMap<String, Vec<Used>> = HashMap::new();
        for used in self.usage.take() {
            let key = key(&used.owner_id, &used.name);
            if self.berm.get(&key).is_some() {
                local.push(used);
            } else if let Some(peer) = self.routes.peer(&key) {
                remote.entry(peer).or_default().push(used);
            }
        }
        usage::write(&self.store, local).await?;
        for (peer, used) in remote {
            if let Err(e) = self.peers.ship(&peer, &used).await {
                tracing::warn!(peer, "handing over service usage failed: {e:#}");
            }
        }
        Ok(())
    }

    /// The program `owner_id/name` runs.
    pub fn get(&self, owner_id: &str, name: &str) -> Option<Arc<Program>> {
        self.berm.get(&key(owner_id, name))
    }

    /// Run the index entry `deploy` names as `owner_id/name`. Refused when it
    /// needs a syscall this host does not serve, when it makes a tool public
    /// that the program it replaces did not and `public` does not confirm it,
    /// or, unless `force`, when it breaks a public tool or event of the program
    /// it replaces that a caller used, with a call that was done, in the last
    /// [`usage::WINDOW`] days.
    pub async fn deploy(
        &self,
        owner_id: &str,
        name: &str,
        deploy: Deploy,
    ) -> Result<Arc<Program>, Denied> {
        let Deploy {
            reference,
            digest,
            public,
            force,
        } = deploy;
        let image = self
            .images
            .pull(&reference, &digest)
            .await
            .map_err(|e| deny(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
        let manifest = Manifest::from_image(&image)
            .map_err(|e| deny(StatusCode::BAD_REQUEST, format!("{e:#}")))?;

        let unserved: Vec<&str> = manifest
            .deps
            .syscalls
            .keys()
            .map(String::as_str)
            .filter(|syscall| !self.berm.serves(syscall))
            .collect();
        if !unserved.is_empty() {
            return Err(deny(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("this host does not serve {}", unserved.join(", ")),
            ));
        }

        // The row is read under the lock, so the breaks are against what the
        // service runs, and the row is written only once the image has
        // compiled: a refused image leaves the service on what it ran.
        let key = key(owner_id, name);
        let lock = self
            .deploying
            .lock()
            .entry(key.clone())
            .or_default()
            .clone();
        let _deploying = lock.lock().await;
        let current = match service::row(&self.store, owner_id, name).await? {
            Some(row) => Some(
                self.load(row)
                    .await
                    .map_err(|e| deny(StatusCode::BAD_GATEWAY, format!("{e:#}")))?,
            ),
            None => None,
        };
        let published: Vec<&str> = match &current {
            Some(current) => current.manifest().publishes(&manifest),
            None => manifest
                .tools
                .iter()
                .filter(|tool| tool.public)
                .map(|tool| tool.name.as_str())
                .collect(),
        };
        let unconfirmed: Vec<&str> = published
            .into_iter()
            .filter(|tool| !public.iter().any(|confirmed| confirmed == tool))
            .collect();
        if !unconfirmed.is_empty() {
            return Err(deny(
                StatusCode::CONFLICT,
                format!(
                    "this deploy makes {} public: any caller may run them, with arguments of their choosing. Confirm each by listing it in `public`",
                    unconfirmed.join(", ")
                ),
            ));
        }
        if let Some(current) = current
            && !force
        {
            self.flush().await?;
            let mut used = Vec::new();
            for broken in current.manifest().breaks(&manifest) {
                let of = match &broken.surface {
                    Surface::Tool(tool) => (Kind::Tool, tool.as_str(), Outcome::Done),
                    Surface::Event(event) => (Kind::Event, event.as_str(), Outcome::Done),
                };
                let callers = usage::read(&self.store, owner_id, name, Some(of)).await?;
                if callers.is_empty() {
                    continue;
                }
                let callers: Vec<String> = callers
                    .iter()
                    .map(|caller| {
                        let last = chrono::DateTime::from_timestamp(caller.last, 0)
                            .map_or_else(String::new, |at| at.format("%Y-%m-%d").to_string());
                        format!("{} {}, last {last}", caller.source, caller.caller)
                    })
                    .collect();
                used.push(format!("{broken} (used by {})", callers.join("; ")));
            }
            if !used.is_empty() {
                return Err(deny(
                    StatusCode::CONFLICT,
                    format!(
                        "this deploy breaks what callers used in the last {} days: {}. Deploy with `force` to override",
                        usage::WINDOW,
                        used.join(" / ")
                    ),
                ));
            }
        }

        // Migrated before the row and the image change: a failed migration
        // refuses the deploy, and the version it replaces goes on serving.
        if let Some(database) = &manifest.database {
            self.databases
                .get(&key)
                .map_err(|e| deny(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?
                .migrate(database.migrations.clone())
                .await
                .map_err(|e| deny(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
        }
        let deprecated: Vec<String> = manifest
            .tools
            .iter()
            .filter(|spec| spec.deprecated)
            .map(|spec| spec.name.clone())
            .collect();
        let program = self
            .compile(key, image)
            .await
            .map_err(|e| deny(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
        service::install(
            &self.store,
            owner_id,
            name,
            &reference,
            &digest,
            &manifest.version,
            deprecated,
        )
        .await?;
        self.subscribe();
        Ok(program)
    }

    /// Run `tool` on `program` as the first link of a chain, and commit what it
    /// wrote and emitted, with `advance` for a delivery. A writing call into a
    /// service with a database commits that database when it returns.
    pub(crate) async fn invoke(
        &self,
        program: &Program,
        caller: Option<&str>,
        user: Option<User>,
        tool: &str,
        args: Vec<u8>,
        advance: Option<Advance>,
    ) -> Result<Invoked> {
        let outcome = program
            .call_as(caller.map(Into::into), user, tool, args)
            .await?;
        if outcome.result.is_err() {
            return Ok(Invoked {
                result: outcome.result,
                denied: outcome.denied,
                committed: true,
            });
        }
        let committed = self.commits.commit(&outcome.changes, advance).await?;
        Ok(Invoked {
            result: outcome.result,
            denied: false,
            committed,
        })
    }

    /// Compile an image into the runtime, off the async workers.
    async fn compile(&self, key: String, image: Vec<u8>) -> Result<Arc<Program>> {
        let berm = self.berm.clone();
        tokio::task::spawn_blocking(move || berm.deploy(&key, &image))
            .await
            .context("compilation panicked")?
    }
}

/// How a call ended.
pub(crate) struct Invoked {
    pub result: Result<String, String>,
    /// `result` is `Err` because the tool refused its caller.
    pub denied: bool,
    /// `false` when a delivery's position lost to another and nothing was
    /// committed.
    pub committed: bool,
}

impl Invoked {
    pub(crate) fn outcome(&self) -> Outcome {
        match (&self.result, self.denied) {
            (Ok(_), _) => Outcome::Done,
            (Err(_), true) => Outcome::Denied,
            (Err(_), false) => Outcome::Failed,
        }
    }
}

/// Whether `program` keeps a database.
pub(crate) fn keeps(program: &Program) -> bool {
    program.manifest().database.is_some()
}

/// How often counted uses are written.
const FLUSH: std::time::Duration = std::time::Duration::from_secs(60);

/// What a service is deployed in the runtime as. An owner's id never changes
/// and neither does a service's name, so the key outlives every deploy.
pub(crate) fn key(owner_id: &str, name: &str) -> String {
    format!("{owner_id}/{name}")
}

/// A subscriber's position, moving past the logs a delivery read.
pub(crate) struct Advance {
    pub owner_id: String,
    /// The subscriber.
    pub name: String,
    /// The service subscribed to, as the subscriber names it.
    pub program: String,
    pub from: i64,
    pub to: i64,
}

/// The owner id and name a [`key`] is made of.
pub(crate) fn unkey(key: &str) -> (&str, &str) {
    key.split_once('/').unwrap_or((key, ""))
}

/// A `berm.call` target names a service of the caller's own owner.
fn resolve(at: &Callsite, target: &str) -> Result<String> {
    if target.contains('/') {
        bail!("a service calls services of its own owner, by name");
    }
    let Some((owner_id, _)) = at.program.split_once('/') else {
        bail!("{} is not deployed under an owner", at.program);
    };
    Ok(key(owner_id, target))
}

/// The runtime's user for one the daemon verified.
pub(crate) fn user(user: &host::User) -> User {
    User {
        issuer: user.issuer.as_str().into(),
        subject: user.subject.as_str().into(),
    }
}

/// A service's IDL: the JSON `berm idl` writes, with its public tools only.
pub(crate) fn idl(name: &str, program: &Program) -> berm_api::Program {
    let Manifest {
        abi_version: _,
        database: _,
        version,
        tools,
        usage,
        deps,
        events,
    } = program.manifest().clone();
    berm_api::Program {
        name: name.to_owned(),
        version,
        digest: program.digest.clone(),
        usage,
        tools: tools.into_iter().filter(|tool| tool.public).collect(),
        deps,
        events,
        unresolved: Deps::default(),
        breaks: Vec::new(),
    }
}

/// The runtime's records, kept nowhere: `services` and the images are where a
/// deploy is recorded, and [`Host::boot`] restores from them.
struct Unrecorded;

impl Storage for Unrecorded {
    fn put(&self, _: Records, _: &str, _: &[u8]) -> Result<()> {
        Ok(())
    }

    fn remove(&self, _: Records, _: &str) -> Result<bool> {
        Ok(false)
    }

    fn list(&self, _: Records) -> Result<Vec<(String, Vec<u8>)>> {
        Ok(Vec::new())
    }
}
