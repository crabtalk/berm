//! The services this host runs: one [`Berm`] holding every deployed service,
//! under the name its daemon gave it. A name's part before its last `/` is its
//! scope, and a service reaches services by name within its own.

use crate::{
    Denied, calls,
    commit::{self, Commits},
    config::Config,
    cron, deny,
    env::{Clock, Edges, Env, Images, Observe, Peers},
    fetch,
    hold::Holds,
    logs,
    meter::{self, Meters as Pending},
    outside::Layers,
    routes::Routes,
    service,
    store::Store,
    terms::Terms,
    usage::{self, Counts, Kind, Outcome, Source},
    wake,
};
use anyhow::{Context, Result, bail};
use axum::http::StatusCode;
use berm::{
    Berm, Called, Calls, Callsite, Dry, Ended, Engine, Log, Meter, Meters, Now, Program, Records,
    Storage, User,
    database::Databases,
    syscall::{
        emit,
        sql::{self, Statement},
        store as state,
    },
};
use berm_api::{
    Budget, Manifest, Surface,
    host::{self, Deploy, Reply, Used},
};
use parking_lot::Mutex;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock, Weak,
        atomic::{AtomicI32, Ordering},
    },
    time::Instant,
};
use tokio::sync::{Notify, Semaphore};

/// Every service, compiled and callable.
pub struct Host<E: Env> {
    /// Where the host keeps its state. A daemon may keep tables of its own
    /// here, named apart from the host's.
    pub store: Store,
    pub(crate) berm: Arc<Berm>,
    /// Chains in flight. Each holds up to `depth + 1` guests, and a guest past
    /// `Config::instances` fails to instantiate.
    pub(crate) gate: Arc<Semaphore>,
    /// Wakes the timer.
    pub(crate) waking: Arc<Notify>,
    /// Uses not yet written.
    pub(crate) usage: Arc<Counts>,
    /// Runs not yet written.
    pub(crate) calls: Arc<calls::Pending>,
    /// Logs not yet written.
    pub(crate) logs: Arc<logs::Pending>,
    /// Meters not yet written.
    pub(crate) meters: Arc<Pending>,
    /// Where the databases of the services that keep one live.
    pub(crate) databases_dir: PathBuf,
    /// The last day stored bytes were sampled, in days since the Unix epoch.
    sampled: Arc<AtomicI32>,
    /// The databases of the services that keep one.
    pub(crate) databases: Arc<Databases>,
    /// Where a chain's changes are committed.
    pub(crate) commits: Commits,
    /// Where the services this host does not run live.
    pub(crate) routes: Arc<Routes>,
    /// The scopes held.
    pub(crate) holds: Arc<Holds>,
    pub(crate) peers: Arc<E::Peers>,
    images: Arc<E::Images>,
    pub(crate) secrets: Arc<E::Secrets>,
    /// [`Edges::layers`].
    pub(crate) layers: Layers,
    /// The most one invocation may use, and what a tool's owner has not set
    /// takes.
    pub budget: Budget,
    /// Each tool's budget its owner set.
    pub(crate) terms: Arc<Terms>,
    /// Held by a deploy of the service keyed by its key.
    deploying: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    /// [`Edges::clock`].
    pub(crate) now: Now,
}

impl<E: Env> Clone for Host<E> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            berm: self.berm.clone(),
            gate: self.gate.clone(),
            waking: self.waking.clone(),
            usage: self.usage.clone(),
            calls: self.calls.clone(),
            logs: self.logs.clone(),
            meters: self.meters.clone(),
            databases_dir: self.databases_dir.clone(),
            sampled: self.sampled.clone(),
            databases: self.databases.clone(),
            commits: self.commits.clone(),
            routes: self.routes.clone(),
            holds: self.holds.clone(),
            peers: self.peers.clone(),
            images: self.images.clone(),
            secrets: self.secrets.clone(),
            layers: self.layers.clone(),
            budget: self.budget,
            terms: self.terms.clone(),
            deploying: self.deploying.clone(),
            now: self.now.clone(),
        }
    }
}

impl<E: Env> Host<E> {
    /// A host with nothing deployed, keeping its state in `state` and sized by
    /// `config`. It serves `berm.get` and `berm.set` over `service_state` or
    /// the service's own database, `berm.call.after` over `service_wakes`,
    /// `berm.sql` over the databases kept under `config.databases`,
    /// `berm.http.fetch` through `edges.fetch` and, when given one,
    /// `berm.http.proxy` through `edges.proxy`, both with `edges.secrets`.
    /// Images come from `edges.images`, a service it does not run is reached
    /// through `edges.peers`, a call from outside goes through `edges.layers`,
    /// and `edges.observe` is told of each invocation.
    pub async fn open(state: &Path, config: &Config, edges: Edges<E>) -> Result<Self> {
        let Edges {
            fetch,
            proxy,
            images,
            peers,
            secrets,
            layers,
            observe,
            clock,
        } = edges;
        let clock = Arc::new(clock);
        let now: Now = Arc::new(move || clock.now());
        let store = Store::open(state)?;
        let routes = Arc::new(Routes::open(store.clone()).await?);
        let holds = Arc::new(Holds::open(store.clone()).await?);
        let peers = Arc::new(peers);
        let engine = Engine::new(&berm::Config {
            cache_dir: config.cache_dir.clone(),
            instances: config.instances,
            fuel: config.fuel,
            rows: config.rows,
            ..Default::default()
        })?;
        let secrets = Arc::new(secrets);
        let mut syscalls = vec![fetch::syscall::<E>(fetch, secrets.clone(), false)];
        if let Some(proxy) = proxy {
            syscalls.push(fetch::syscall::<E>(proxy, secrets.clone(), true));
        }
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
        let commits = Commits::new(store.clone(), databases.clone());
        let (reading, writing) = (kept.clone(), kept.clone());
        let (store_reading, commits_reading) = (store.clone(), commits.clone());
        syscalls.extend(state::programs(
            move |at: Callsite, key: String| {
                let (store, kept) = (store_reading.clone(), reading(&at.program));
                let set = at
                    .dry
                    .as_ref()
                    .and_then(|_| commits_reading.kept(at.chain, &at.program, &key));
                async move {
                    if let Some(database) = kept {
                        return database?.get(at.chain, key).await;
                    }
                    if set.is_some() {
                        return Ok(set);
                    }
                    let (scope, name) = split(&at.program);
                    service::read(&store, scope, name, key).await
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
        let emitted = Arc::new(logs::Pending::new(now.clone()));
        let taking = emitted.clone();
        syscalls.push(emit::programs(move |at: Callsite, log: Log| {
            let taken = taking.take(&at, log);
            async move { Ok(taken) }
        }));
        syscalls.push(sql::programs(move |at: Callsite, statement: Statement| {
            let kept = kept(&at.program);
            async move {
                let Some(database) = kept else {
                    bail!("this service keeps no database");
                };
                let database = database?;
                let answer = match at.query {
                    true => database.read(at.chain, statement).await?,
                    false => database.exec(at.chain, statement).await?,
                };
                at.meter(Meter {
                    read: answer.read,
                    written: answer.written,
                    ..Meter::default()
                });
                Ok(answer.json)
            }
        }));
        let waking = Arc::new(Notify::new());
        syscalls.push(wake::syscall(
            now.clone(),
            store.clone(),
            runtime.clone(),
            routes.clone(),
            waking.clone(),
        ));
        let usage = Arc::new(Counts::new(now.clone()));
        let ran = Arc::new(calls::Pending::new(
            now.clone(),
            calls::newest(&store).await?,
        ));
        let meters = Arc::new(Pending::new(now.clone()));
        let (reporting, reported) = (usage.clone(), deployed);
        let terms = Arc::new(Terms::default());
        terms.load(&store).await?;
        let budgeting = terms.clone();
        let (reaching, routing) = (peers.clone(), routes.clone());
        let calls = Calls {
            depth: config.depth,
            resolve: {
                let holds = holds.clone();
                Arc::new(move |at: &Callsite, target: &str, _: &str| {
                    let key = resolve(at, target)?;
                    if holds.holds(split(&key).0) {
                        bail!("{target} is held");
                    }
                    Ok(key)
                })
            },
            // A call to a service on a peer is counted there.
            called: Arc::new(
                move |at: &Callsite, deployed: &str, tool: &str, called: Called| {
                    if at.dry.is_some() || reported(deployed).is_none() {
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
            // A callee is told the calling service's name; it is in the same
            // scope.
            named: Arc::new(|program: &str| split(program).1.to_owned()),
            invoked: {
                let running = emitted.clone();
                Arc::new(move |at: &Callsite, tool: &str, used: Meter| {
                    let call = at.dry.is_none().then(|| running.run(at.chain)).flatten();
                    observe.invoked(&at.program, tool, call, &used);
                })
            },
            enter: commit::enter(commits.clone()),
            budget: Arc::new(move |service: &str, tool: &str| budgeting.get(service, tool).budget),
            now: now.clone(),
            remote: Arc::new(move |at, deployed, tool, args, link| {
                let peer = routing.peer(deployed)?;
                let (peers, from) = (reaching.clone(), at.program.to_string());
                let (service, tool) = (deployed.to_owned(), tool.to_owned());
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
                    client: link.client.map(|client| client.to_string()),
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
                    dry: link.dry.as_deref().cloned(),
                };
                let at = at.clone();
                Some(Box::pin(async move {
                    let Reply {
                        result,
                        denied,
                        meters,
                    } = peers.link(&peer, &service, &tool, args, link).await?;
                    for (service, used) in meters {
                        at.meter_of(&service, used);
                    }
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
            waking,
            usage,
            calls: ran,
            logs: emitted,
            meters,
            databases_dir: config.databases.clone(),
            sampled: Arc::default(),
            databases,
            commits,
            routes,
            holds,
            peers,
            images: Arc::new(images),
            secrets,
            layers,
            budget: Budget {
                instructions: config.fuel,
                rows: config.rows,
            },
            terms,
            deploying: Arc::default(),
            now,
        })
    }

    /// Load every service this host runs off the request path, fire due wakes,
    /// and write counted uses.
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

    /// Load every service as `services` names it.
    async fn reconcile(&self) {
        let rows = match service::all(&self.store).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("reading services failed: {e:#}");
                return;
            }
        };
        for row in rows {
            let key = join(&row.scope, &row.name);
            match self.load(row).await {
                Ok(program) => {
                    tracing::debug!(service = key, digest = %program.digest, "loaded");
                }
                Err(e) => tracing::error!(service = key, "loading failed: {e:#}"),
            }
        }
    }

    /// The program a `services` row names, pulled and compiled unless it
    /// already runs that digest.
    async fn load(&self, row: service::Row) -> Result<Arc<Program>> {
        let key = join(&row.scope, &row.name);
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
            if self.berm.get(&used.service).is_some() {
                local.push(used);
            } else if let Some(peer) = self.routes.peer(&used.service) {
                remote.entry(peer).or_default().push(used);
            }
        }
        usage::write(&self.store, local).await?;
        for (peer, used) in remote {
            if let Err(e) = self.peers.ship(&peer, &used).await {
                tracing::warn!(peer, "handing over service usage failed: {e:#}");
            }
        }
        calls::write(&self.store, self.calls.take(), (self.now)()).await?;
        logs::write(&self.store, self.logs.take_all(), (self.now)()).await?;
        meter::write(&self.store, self.meters.take()).await?;
        let today = self.today();
        if self.sampled.swap(today, Ordering::Relaxed) != today {
            for program in self.berm.list() {
                meter::sample(
                    &self.store,
                    &program.name,
                    &self.databases_dir.join(&*program.name),
                    today,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// The program `service` runs.
    pub fn get(&self, service: &str) -> Option<Arc<Program>> {
        self.berm.get(service)
    }

    /// Every service in `scope` this host runs, by its name within it.
    pub fn list(&self, scope: &str) -> Vec<(String, Arc<Program>)> {
        self.berm
            .list()
            .into_iter()
            .filter_map(|program| {
                let (within, name) = split(&program.name);
                (within == scope).then(|| (name.to_owned(), program.clone()))
            })
            .collect()
    }

    /// Stop running `service` and drop what it committed, its wake and its
    /// usage. `false` when it was not deployed.
    // TODO: drop the database of a service that kept one; its file stays
    // under `Config::databases` and a deploy under the same name opens it.
    pub async fn remove(&self, service: &str) -> Result<bool> {
        let lock = self
            .deploying
            .lock()
            .entry(service.to_owned())
            .or_default()
            .clone();
        let _deploying = lock.lock().await;
        let (scope, name) = split(service);
        let removed = service::remove(&self.store, scope, name).await?;
        self.berm.remove(service)?;
        self.terms.forget(service);
        Ok(removed)
    }

    /// Run the index entry `deploy` names as `service`. Refused when it
    /// needs a syscall this host does not serve, when it makes a tool public
    /// that the program it replaces did not and `public` does not confirm it,
    /// or, unless `force`, when it breaks a public tool or event of the program
    /// it replaces that a caller used, with a call that was done, in the last
    /// [`usage::WINDOW`] days.
    pub async fn deploy(&self, service: &str, deploy: Deploy) -> Result<Arc<Program>, Denied> {
        let Deploy {
            reference,
            digest,
            public,
            force,
            crons,
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
        let key = service.to_owned();
        let (scope, name) = split(service);
        let lock = self
            .deploying
            .lock()
            .entry(key.clone())
            .or_default()
            .clone();
        let _deploying = lock.lock().await;
        let current = match service::row(&self.store, scope, name).await? {
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
        let cron_tools: Vec<String> = manifest
            .tools
            .iter()
            .filter(|spec| spec.cron)
            .map(|spec| spec.name.clone())
            .collect();
        if let Some(crons) = &crons {
            for (tool, expression) in crons {
                if !cron_tools.contains(tool) {
                    return Err(deny(
                        StatusCode::BAD_REQUEST,
                        format!("`crons` names {tool:?}, which is not a cron tool of this image"),
                    ));
                }
                cron::parse(expression)
                    .map_err(|e| deny(StatusCode::BAD_REQUEST, format!("{tool}: {e:#}")))?;
            }
            if !force {
                let undone: Vec<String> = service::edited(&self.store, scope, name)
                    .await?
                    .into_iter()
                    .filter(|(tool, cron, _)| {
                        cron_tools.contains(tool) && crons.get(tool) != cron.as_ref()
                    })
                    .map(|(tool, cron, _)| {
                        format!(
                            "{tool}: the deploy sets {}, its owner set {}",
                            crons.get(&tool).map_or("none", String::as_str),
                            cron.as_deref().unwrap_or("none")
                        )
                    })
                    .collect();
                if !undone.is_empty() {
                    return Err(deny(
                        StatusCode::CONFLICT,
                        format!(
                            "this deploy undoes the crons its owner set since the last deploy: {}. Take the owner's crons into the deploy's config, or deploy with `force` to override",
                            undone.join(" / ")
                        ),
                    ));
                }
            }
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
                let callers = usage::read(
                    &self.store,
                    scope,
                    name,
                    Some(of),
                    usage::seconds(&self.now),
                )
                .await?;
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
            scope,
            name,
            &reference,
            &digest,
            &manifest.version,
            deprecated,
            cron_tools,
            crons,
            (self.now)() as i64,
        )
        .await?;
        self.waking.notify_one();
        Ok(program)
    }

    /// Run `tool` on `program` as the first link of a chain, as call `call`,
    /// and commit what it wrote unless `dry` is set. A writing call into a
    /// service with a database commits that database when it returns. What the
    /// chain emits is kept beside `call` whatever it returns.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn invoke(
        &self,
        program: &Program,
        call: u64,
        caller: Option<&str>,
        user: Option<User>,
        client: Option<Arc<str>>,
        tool: &str,
        args: Vec<u8>,
        dry: Option<Arc<Dry>>,
    ) -> Result<Invoked> {
        let meters = Arc::new(Meters::default());
        let keeps = dry.is_none();
        let chain = berm::chain();
        self.logs.started(chain, call);
        let outcome = program
            .call_as(
                caller.map(Into::into),
                user,
                client,
                tool,
                args,
                dry,
                meters.clone(),
                chain,
            )
            .await;
        self.logs.ended(chain);
        let meter = self.metered(meters.take());
        let outcome = outcome?;
        if outcome.result.is_err() || !keeps {
            return Ok(Invoked {
                result: outcome.result,
                denied: outcome.denied,
                meter,
            });
        }
        self.commits.commit(&outcome.changes).await?;
        Ok(Invoked {
            result: outcome.result,
            denied: false,
            meter,
        })
    }

    /// Count a chain's `meters` against the services this host runs, and
    /// answer their sum over every service the chain reached.
    pub(crate) fn metered(&self, meters: BTreeMap<Arc<str>, Meter>) -> Meter {
        let mut total = Meter::default();
        for (service, used) in meters {
            if self.berm.get(&service).is_some() {
                self.meters.record(&service, used);
            }
            total += used;
        }
        total
    }

    /// Days since the Unix epoch, as the clock tells it.
    pub(crate) fn today(&self) -> i32 {
        (usage::seconds(&self.now) / 86_400) as i32
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
    /// What the chain used, over every service it reached.
    pub meter: Meter,
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

/// The service named `name` in `scope`.
pub(crate) fn join(scope: &str, name: &str) -> String {
    match scope {
        "" => name.to_owned(),
        scope => format!("{scope}/{name}"),
    }
}

/// The scope `service` is in, and its name within it: `a/b` is `b` in `a`,
/// and `b` is `b` in the empty scope.
pub(crate) fn split(service: &str) -> (&str, &str) {
    service.rsplit_once('/').unwrap_or(("", service))
}

/// A `berm.call` target: the calling service itself or one its manifest
/// declares in `programs`, by name in the caller's scope.
fn resolve(at: &Callsite, target: &str) -> Result<String> {
    if target.contains('/') {
        bail!("a service calls services in its own scope, by name");
    }
    let (scope, name) = split(&at.program);
    if target != name && !at.deps.programs.iter().any(|declared| declared == target) {
        bail!("{name} does not declare {target:?} in its programs");
    }
    Ok(join(scope, target))
}

/// The runtime's user for one the daemon handed in.
pub(crate) fn user(user: &host::User) -> User {
    User {
        issuer: user.issuer.as_str().into(),
        subject: user.subject.as_str().into(),
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
