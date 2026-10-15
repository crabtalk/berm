//! What a daemon's routes ask of a host: calls from outside and from peers,
//! and reading and setting what each service is.

use crate::{
    Denied, calls, deny,
    env::{Admit, Env},
    host::{self, Host},
    meter, service,
    usage::{self, Kind, Outcome, Source},
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use berm::{Budget, Dry, Meters, Program, Refused};
use berm_api::{
    Logged, Output, Terms, ToolTerms,
    host::{self as wire, Caller, Link, Metered, Reply, Route, Usage, Used},
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
        Ok(host::idl(host::split(service).1, &program))
    }

    /// The IDL of every service in `scope` this host runs, with every tool,
    /// each marked by `public`.
    pub fn described(&self, scope: &str) -> Vec<berm_api::Program> {
        self.list(scope)
            .into_iter()
            .map(|(name, program)| host::described(&name, &program))
            .collect()
    }

    /// `service`'s terms: each public tool's budget, what its owner set or
    /// else this host's, and its price.
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
                        price_micros: set.price_micros,
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

    /// Price a call of `tool` of `service` that ends `done` at `micros` USD
    /// micros, from its next call. Refused past `i64::MAX`.
    pub async fn set_price(&self, service: &str, tool: &str, micros: u64) -> Result<(), Denied> {
        self.exported(service, tool).await?;
        if micros > i64::MAX as u64 {
            return Err(deny(
                StatusCode::BAD_REQUEST,
                format!("a price is at most {} micros", i64::MAX),
            ));
        }
        Ok(self
            .terms
            .set_price(&self.store, service, tool, micros)
            .await?)
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

    /// Run `tool` on `service` from outside, as `caller` when there is one,
    /// counted as its use. `query` holds it to a tool the IDL marks `query`.
    /// Answers with the deprecation headers the tool's answer carries.
    pub async fn call(
        &self,
        service: &str,
        caller: Option<&Caller>,
        tool: &str,
        args: Vec<u8>,
        query: bool,
    ) -> Result<(HeaderMap, Output), Denied> {
        self.run(service, caller, tool, args, query, None).await
    }

    /// [`Host::call`], made dry: nothing the chain changes is kept, its
    /// fetches are answered as `dry` says, and it is not counted as a use. Its
    /// meter is counted.
    pub async fn dry_run(
        &self,
        service: &str,
        caller: Option<&Caller>,
        tool: &str,
        args: Vec<u8>,
        dry: Dry,
    ) -> Result<(HeaderMap, Output), Denied> {
        self.run(service, caller, tool, args, false, Some(Arc::new(dry)))
            .await
    }

    async fn run(
        &self,
        service: &str,
        caller: Option<&Caller>,
        tool: &str,
        args: Vec<u8>,
        query: bool,
        dry: Option<Arc<Dry>>,
    ) -> Result<(HeaderMap, Output), Denied> {
        let (program, _) = self.deployed(service).await?;
        let Some(spec) = program
            .manifest()
            .tools
            .iter()
            .find(|spec| spec.name == tool && spec.public)
        else {
            return Err(deny(
                StatusCode::NOT_FOUND,
                format!("the service has no public tool named {tool:?}"),
            ));
        };
        if query && !spec.query {
            return Err(deny(
                StatusCode::METHOD_NOT_ALLOWED,
                format!("{tool} is not a query; POST it"),
            ));
        }
        let (source, who) = counted(caller);
        let counts = dry.is_none();
        let started = self.calls.start();
        let refused = |denied: Denied| {
            if counts {
                self.usage
                    .record(service, Kind::Tool, tool, source, who, Outcome::Refused);
                self.calls.record(
                    service,
                    started,
                    tool,
                    source,
                    who,
                    Outcome::Refused,
                    None,
                    0,
                );
            }
            Err(denied)
        };
        if !program.reached_by_accounts(tool) {
            return refused(deny(
                StatusCode::FORBIDDEN,
                format!("{tool} takes calls from services only"),
            ));
        }
        if let Err(denied) = self.admit.admit(service, tool, spec.query, caller).await {
            return refused(denied);
        }
        let mut headers = HeaderMap::new();
        let (scope, name) = host::split(service);
        if spec.deprecated
            && let Some((since, sunset)) =
                usage::deprecation(&self.store, scope, name, tool).await?
        {
            headers.insert(
                HeaderName::from_static("deprecation"),
                HeaderValue::from_str(&format!("@{since}")).expect("a number is a header value"),
            );
            if let Some(at) = sunset.and_then(|at| chrono::DateTime::from_timestamp(at, 0)) {
                headers.insert(
                    HeaderName::from_static("sunset"),
                    HeaderValue::from_str(&at.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
                        .expect("a date is a header value"),
                );
            }
        }
        let Ok(_slot) = self.gate.try_acquire() else {
            return Err(deny(
                StatusCode::SERVICE_UNAVAILABLE,
                "every call slot is taken; retry",
            ));
        };
        let invoked = self
            .invoke(
                &program,
                started.id,
                caller.map(|caller| caller.id.as_str()),
                caller
                    .and_then(|caller| caller.user.as_ref())
                    .map(host::user),
                tool,
                args,
                dry,
            )
            .await
            .map_err(|e| deny(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
        if counts {
            let outcome = invoked.outcome();
            self.usage
                .record(service, Kind::Tool, tool, source, who, outcome);
            let price = match outcome {
                Outcome::Done => self.terms.get(service, tool).price_micros,
                _ => 0,
            };
            self.calls.record(
                service,
                started,
                tool,
                source,
                who,
                outcome,
                Some(invoked.meter),
                price,
            );
        }
        headers.insert(
            HeaderName::from_static(wire::METER),
            HeaderValue::from_str(&serde_json::to_string(&invoked.meter).expect("a meter is JSON"))
                .expect("JSON of numbers is a header value"),
        );
        Ok((headers, invoked.result.into()))
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

    /// What `service` is scheduled to run.
    pub async fn schedule(&self, service: &str) -> Result<wire::Schedule, Denied> {
        self.deployed(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        Ok(wire::Schedule {
            wakes: service::scheduled(&self.store, scope, name).await?,
            held: self.holds.holds(scope),
        })
    }

    /// `service`'s runs on `page`, newest first, and its `Link` header. At
    /// most `page.limit`, held to [`MAX_PAGE`].
    pub async fn calls(
        &self,
        service: &str,
        mut page: wire::CallPage,
    ) -> Result<(Vec<wire::Call>, Option<String>), Denied> {
        self.deployed(service).await?;
        self.flush().await?;
        let (scope, name) = host::split(service);
        page.limit = page.limit.clamp(1, MAX_PAGE);
        page.page = page.page.max(1);
        let (calls, total) = calls::read(&self.store, scope, name, page.clone()).await?;
        Ok((calls, page.link(total)))
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
    async fn deployed(&self, service: &str) -> Result<(Arc<Program>, i64), Denied> {
        let missing = || deny(StatusCode::NOT_FOUND, "no such service");
        let (scope, name) = host::split(service);
        let commits = service::commits(&self.store, scope, name)
            .await?
            .ok_or_else(missing)?;
        let program = self.get(service).ok_or_else(missing)?;
        Ok((program, commits))
    }
}

/// How a call from outside is counted: by its app, by its account, or as
/// anonymous without a caller.
fn counted(caller: Option<&Caller>) -> (Source, &str) {
    match caller {
        Some(Caller { app: Some(app), .. }) => (Source::App, app),
        Some(caller) => (Source::Account, &caller.id),
        None => (Source::Anonymous, ""),
    }
}
