//! What a daemon's routes ask of a host: calls from peers, and reading and
//! setting what each service is.

use crate::{
    Denied, calls, cron, deny,
    env::{Env, Secrets},
    host::{self, Host},
    meter, service,
    usage::{self, Kind, Outcome, Source},
};
use axum::http::StatusCode;
use berm::{Budget, Meters, Program, Refused};
use berm_api::{
    Deps, Listing, Logged, Terms, ToolTerms,
    host::{self as wire, Link, Metered, Reply, Route, Usage, Used},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// The most logs or calls one page answers.
const MAX_PAGE: i64 = 1000;

impl<E: Env> Host<E> {
    /// `service`'s IDL.
    pub async fn idl(&self, service: &str) -> Result<berm_api::Program, Denied> {
        let (program, _) = self.deployed(service).await?;
        Ok(program.idl(host::split(service).1))
    }

    /// `service` as anyone it is visible to sees it.
    pub async fn listing(&self, service: &str) -> Result<Listing, Denied> {
        let program = self.idl(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        let served = usage::served(&self.store, scope, name, usage::seconds(&self.now)).await?;
        Ok(Listing { program, served })
    }

    /// `service`'s runtime, for its owners, with what it declares that nothing
    /// answers to: services neither deployed here nor routed to a peer, and
    /// secrets not granted it. Secrets that cannot be read grant nothing.
    pub async fn runtime(&self, service: &str) -> Result<berm_api::Runtime, Denied> {
        let (program, _) = self.deployed(service).await?;
        let scope = host::split(service).0;
        let deps = &program.manifest().deps;
        let granted = self.secrets.names(service).await.unwrap_or_default();
        Ok(berm_api::Runtime {
            unresolved: Deps {
                programs: deps
                    .programs
                    .iter()
                    .filter(|name| {
                        let key = host::join(scope, name);
                        self.get(&key).is_none() && self.routes.peer(&key).is_none()
                    })
                    .cloned()
                    .collect(),
                syscalls: Default::default(),
                secrets: deps
                    .secrets
                    .iter()
                    .filter(|name| !granted.contains(name))
                    .cloned()
                    .collect(),
            },
            ..program.runtime()
        })
    }

    /// The IDL of every service in `scope` this host runs.
    pub fn idls(&self, scope: &str) -> Vec<berm_api::Program> {
        self.list(scope)
            .into_iter()
            .map(|(name, program)| program.idl(&name))
            .collect()
    }

    /// `service`'s terms: each public tool's budget, what its owner set or
    /// else this host's.
    pub fn terms(&self, service: &str) -> Result<Terms, Denied> {
        let program = self
            .get(service)
            .ok_or_else(|| deny(StatusCode::NOT_FOUND, "no such service"))?;
        let tools = program
            .manifest()
            .tools
            .iter()
            .filter(|tool| tool.public)
            .map(|tool| {
                let set = self.terms.get(service, &tool.name);
                (
                    tool.name.clone(),
                    ToolTerms {
                        budget: set.budget.unwrap_or(self.budget),
                    },
                )
            })
            .collect();
        Ok(Terms { tools })
    }

    /// Set what one invocation of `tool` of `service` may use, from its next
    /// call. Refused past this host's budget.
    pub async fn set_budget(
        &self,
        service: &str,
        tool: &str,
        budget: Budget,
    ) -> Result<(), Denied> {
        self.exported(service, tool).await?;
        if budget.instructions == 0
            || budget.rows == 0
            || budget.instructions > self.budget.instructions
            || budget.rows > self.budget.rows
        {
            return Err(deny(
                StatusCode::BAD_REQUEST,
                format!(
                    "a budget is between 1 and {} instructions, and 1 and {} rows",
                    self.budget.instructions, self.budget.rows
                ),
            ));
        }
        Ok(self
            .terms
            .set_budget(&self.store, service, tool, budget)
            .await?)
    }

    /// Call cron tool `tool` of `service` when `cron` fires, from its next
    /// firing when it changes; `None` stops it. Kept until a deploy's config
    /// sets another.
    pub async fn set_cron(
        &self,
        service: &str,
        tool: &str,
        cron: Option<String>,
    ) -> Result<(), Denied> {
        self.deployed(service).await?;
        let now = (self.now)() as i64;
        let due = cron
            .as_deref()
            .map(|cron| cron::next(cron, now))
            .transpose()
            .map_err(|e| deny(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
        let (scope, name) = host::split(service);
        if !service::set_cron(&self.store, scope, name, tool, cron, due).await? {
            return Err(deny(
                StatusCode::NOT_FOUND,
                format!("the service has no cron tool named {tool:?}"),
            ));
        }
        self.waking.notify_one();
        Ok(())
    }

    /// Refused unless `service` is deployed and exports `tool`.
    async fn exported(&self, service: &str, tool: &str) -> Result<(), Denied> {
        let (program, _) = self.deployed(service).await?;
        if !program.tools().any(|exported| exported == tool) {
            return Err(deny(
                StatusCode::NOT_FOUND,
                format!("the service has no tool named {tool:?}"),
            ));
        }
        Ok(())
    }

    /// `service`'s commit count.
    pub async fn commits(&self, service: &str) -> Result<i64, Denied> {
        let (_, commits) = self.deployed(service).await?;
        Ok(commits)
    }

    /// Set or clear a deprecated tool's sunset.
    pub async fn sunset(&self, service: &str, tool: &str, at: Option<i64>) -> Result<(), Denied> {
        let (scope, name) = host::split(service);
        if !usage::sunset(&self.store, scope, name, tool, at).await? {
            return Err(deny(
                StatusCode::CONFLICT,
                format!("{tool} is not a deprecated tool of this service"),
            ));
        }
        Ok(())
    }

    /// Who uses `service`.
    pub async fn usage(&self, service: &str) -> Result<Vec<Usage>, Denied> {
        self.flush().await?;
        let (scope, name) = host::split(service);
        let mut used =
            usage::read(&self.store, scope, name, None, usage::seconds(&self.now)).await?;
        if let Some(program) = self.get(service) {
            for row in used.iter_mut().filter(|row| row.kind == "tool") {
                row.public = Some(program.public(&row.item));
            }
        }
        Ok(used)
    }

    /// `service`'s logs on `page`, oldest first, and its `Link` header. At
    /// most `page.limit`, held to [`MAX_PAGE`].
    pub async fn logs(
        &self,
        service: &str,
        mut page: wire::Page,
    ) -> Result<(Vec<Logged>, Option<String>), Denied> {
        self.deployed(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        page.limit = page.limit.clamp(1, MAX_PAGE);
        page.page = page.page.max(1);
        let (logs, total) = service::logs(
            &self.store,
            scope,
            name,
            page.topic.clone(),
            page.call,
            page.after,
            (page.page - 1).saturating_mul(page.limit),
            page.limit,
        )
        .await?;
        Ok((logs, page.link(total)))
    }

    /// `service`'s cron tools.
    pub async fn crons(&self, service: &str) -> Result<wire::Crons, Denied> {
        self.deployed(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        Ok(wire::Crons {
            crons: service::crons(&self.store, scope, name).await?,
            held: self.holds.holds(scope),
        })
    }

    /// The wakes armed to call `service`'s tools that have not run.
    pub async fn armed(&self, service: &str) -> Result<wire::Wakes, Denied> {
        self.deployed(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        Ok(wire::Wakes {
            wakes: service::armed(&self.store, scope, name).await?,
            held: self.holds.holds(scope),
        })
    }

    /// `service`'s runs on `page`, newest first, and its `Link` header. At
    /// most `page.limit`, held to [`MAX_PAGE`].
    pub async fn calls(
        &self,
        service: &str,
        mut page: wire::CallPage,
    ) -> Result<(Vec<wire::Call>, i64, Option<String>), Denied> {
        self.deployed(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        page.limit = page.limit.clamp(1, MAX_PAGE);
        page.page = page.page.max(1);
        let (calls, total) = calls::read(&self.store, scope, name, page.clone()).await?;
        Ok((calls, total, page.link(total)))
    }

    /// A call from a service on a peer, run as `link` and committed here unless
    /// the chain is dry. A refusal is `403` with its reason.
    pub async fn link(
        &self,
        service: &str,
        tool: &str,
        args: Vec<u8>,
        link: Link,
    ) -> Result<Reply, Denied> {
        let program = self.get(service).ok_or_else(|| {
            deny(
                StatusCode::NOT_FOUND,
                format!("no program named {service:?} is deployed"),
            )
        })?;
        let dry = link.dry.is_some();
        let refused = |message: String| {
            if !dry {
                self.usage.record(
                    service,
                    Kind::Tool,
                    tool,
                    Source::Service,
                    &link.from,
                    Outcome::Refused,
                );
            }
            Err(deny(StatusCode::FORBIDDEN, message))
        };
        if self.holds.holds(host::split(service).0) {
            return refused(format!("{service} is held"));
        }
        if !program.public(tool) {
            return refused(format!("{service} has no public tool named {tool:?}"));
        }
        if link.caller.is_some() && !program.reached_by_programs(tool) {
            return refused(format!("{service}.{tool} takes no calls from programs"));
        }
        let _slot = self.gate.acquire().await.map_err(anyhow::Error::from)?;
        let meters = Arc::new(Meters::default());
        let outcome = program
            .link(
                berm::Link {
                    caller: link
                        .caller
                        .as_deref()
                        .map(|caller| berm::Caller::Program(caller.into())),
                    user: link.user.as_ref().map(host::user),
                    client: link.client.as_deref().map(Into::into),
                    depth: link.depth,
                    query: link.query,
                    dry: link.dry.clone().map(Arc::new),
                    deadline: Instant::now() + Duration::from_millis(link.timeout),
                    stack: link
                        .stack
                        .iter()
                        .map(|program| Arc::from(program.as_str()))
                        .collect(),
                },
                tool,
                args,
                meters.clone(),
            )
            .await;
        let used = meters.take();
        self.metered(used.clone());
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => match error.downcast::<Refused>() {
                Ok(Refused(message)) => return refused(message),
                Err(error) => return Err(error.into()),
            },
        };
        if outcome.result.is_ok() && !dry {
            self.commits.commit(&outcome.changes).await?;
        }
        let ended = match (&outcome.result, outcome.denied) {
            (Ok(_), _) => Outcome::Done,
            (Err(_), true) => Outcome::Denied,
            (Err(_), false) => Outcome::Failed,
        };
        if !dry {
            self.usage.record(
                service,
                Kind::Tool,
                tool,
                Source::Service,
                &link.from,
                ended,
            );
        }
        Ok(Reply {
            result: outcome.result,
            denied: outcome.denied,
            meters: used
                .into_iter()
                .map(|(service, used)| (service.to_string(), used))
                .collect(),
        })
    }

    /// `service`'s meters over the last 30 days, oldest first.
    pub async fn meters(&self, service: &str) -> Result<Vec<Metered>, Denied> {
        self.deployed(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        Ok(meter::read(&self.store, scope, name, self.today()).await?)
    }

    /// What `service` is using now.
    pub async fn live(&self, service: &str) -> Result<wire::Live, Denied> {
        let (program, _) = self.deployed(service).await?;
        let (scope, name) = host::split(service);
        let now = usage::seconds(&self.now);
        let day = now.div_euclid(86_400) as i32;
        let (mut all, mut today) = meter::used(&self.store, scope, name, day).await?;
        let (unsaved, unsaved_today) = self.meters.unsaved(service, day);
        all += unsaved;
        today += unsaved_today;
        let elapsed = (now - day as i64 * 86_400).max(1) as f64;
        let count = |total: u64, on: u64| wire::Count {
            total,
            per_second: on as f64 / elapsed,
        };
        let (running, memory) = program.running.now();
        Ok(wire::Live {
            running,
            memory,
            stored: meter::stored(&self.store, service, &self.databases_dir.join(service)).await?,
            instructions: count(all.instructions, today.instructions),
            read: count(all.read, today.read),
            written: count(all.written, today.written),
            proxied: count(all.proxied, today.proxied),
            logged: count(all.logged, today.logged),
        })
    }

    /// Hold `routes` in place of the routes held.
    pub async fn route(&self, routes: Vec<Route>) -> Result<(), Denied> {
        Ok(self.routes.replace(routes).await?)
    }

    /// Hold `scopes` in place of the scopes held. A held scope's services fire
    /// no wakes and take no calls from services or peers; what they stored and
    /// what was due stays, and runs once the scope is released. Calls from
    /// outside are the daemon's to refuse.
    pub async fn hold(&self, scopes: Vec<String>) -> Result<(), Denied> {
        self.holds.replace(scopes.into_iter().collect()).await?;
        self.waking.notify_one();
        Ok(())
    }

    /// The scopes held.
    pub fn held(&self) -> Vec<String> {
        self.holds.list()
    }

    /// Add uses a peer counted of services this host runs.
    pub async fn record(&self, used: Vec<Used>) -> Result<(), Denied> {
        Ok(usage::write(&self.store, used).await?)
    }

    /// The program `service` runs now, and its commit count.
    pub(crate) async fn deployed(&self, service: &str) -> Result<(Arc<Program>, i64), Denied> {
        let missing = || deny(StatusCode::NOT_FOUND, "no such service");
        let (scope, name) = host::split(service);
        let commits = service::commits(&self.store, scope, name)
            .await?
            .ok_or_else(missing)?;
        let program = self.get(service).ok_or_else(missing)?;
        Ok((program, commits))
    }
}
