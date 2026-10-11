//! What a daemon's routes ask of a host: calls from outside and from peers,
//! and reading and setting what each service is.

use crate::{
    Denied, callers, deny,
    env::Env,
    host::{self, Host},
    service,
    usage::{self, Kind, Outcome, Source},
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use berm::{Program, Refused};
use berm_api::{
    Logged, Output,
    host::{Allowed, Caller, Link, Reply, Route, Usage, Used},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// The most logs one page answers.
const MAX_PAGE: i64 = 1000;

impl<E: Env> Host<E> {
    /// `owner_id/name`'s IDL.
    pub async fn idl(&self, owner_id: &str, name: &str) -> Result<berm_api::Program, Denied> {
        let (program, _) = self.deployed(owner_id, name).await?;
        Ok(host::idl(name, &program))
    }

    /// `owner_id/name`'s commit count.
    pub async fn commits(&self, owner_id: &str, name: &str) -> Result<i64, Denied> {
        let (_, commits) = self.deployed(owner_id, name).await?;
        Ok(commits)
    }

    /// Run `tool` on `owner_id/name` for `caller`, counted as its use. `query`
    /// holds it to a tool the IDL marks `query`. Answers with the deprecation
    /// headers the tool's answer carries.
    pub async fn call(
        &self,
        owner_id: &str,
        name: &str,
        caller: &Caller,
        tool: &str,
        args: Vec<u8>,
        query: bool,
    ) -> Result<(HeaderMap, Output), Denied> {
        let (program, _) = self.deployed(owner_id, name).await?;
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
        self.admit(caller, &program, tool, spec.query)?;
        let mut headers = HeaderMap::new();
        if spec.deprecated
            && let Some((since, sunset)) =
                usage::deprecation(&self.store, owner_id, name, tool).await?
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
                Some(&caller.id),
                caller.user.as_ref().map(host::user),
                tool,
                args,
                None,
            )
            .await
            .map_err(|e| deny(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
        let (source, who) = counted(caller);
        self.usage.record(
            &program.name,
            Kind::Tool,
            tool,
            source,
            who,
            invoked.outcome(),
        );
        Ok((headers, invoked.result.into()))
    }

    /// Set or clear a deprecated tool's sunset.
    pub async fn sunset(
        &self,
        owner_id: &str,
        name: &str,
        tool: &str,
        at: Option<i64>,
    ) -> Result<(), Denied> {
        if !usage::sunset(&self.store, owner_id, name, tool, at).await? {
            return Err(deny(
                StatusCode::CONFLICT,
                format!("{tool} is not a deprecated tool of this service"),
            ));
        }
        Ok(())
    }

    /// Who may call `owner_id/name`.
    pub async fn callers(&self, owner_id: &str, name: &str) -> Result<Vec<Allowed>, Denied> {
        Ok(callers::read(&self.store, owner_id, name).await?)
    }

    /// Replace who may call `owner_id/name`.
    pub async fn allow(
        &self,
        owner_id: &str,
        name: &str,
        rules: Vec<Allowed>,
    ) -> Result<(), Denied> {
        let program = self
            .get(owner_id, name)
            .ok_or_else(|| deny(StatusCode::NOT_FOUND, "no such service"))?;
        for rule in &rules {
            if !["account", "app", "service"].contains(&rule.kind.as_str()) {
                return Err(deny(
                    StatusCode::BAD_REQUEST,
                    format!("{:?} is not `account`, `app` or `service`", rule.kind),
                ));
            }
            if rule.tool != "*" && !program.public(&rule.tool) {
                return Err(deny(
                    StatusCode::BAD_REQUEST,
                    format!("{:?} is not a public tool of this service", rule.tool),
                ));
            }
        }
        callers::replace(&self.store, owner_id, name, rules).await?;
        self.allowlists
            .load(&self.store, &host::key(owner_id, name))
            .await?;
        Ok(())
    }

    /// Who uses `owner_id/name`. An account caller is named by its id.
    pub async fn usage(&self, owner_id: &str, name: &str) -> Result<Vec<Usage>, Denied> {
        self.flush().await?;
        let mut used = usage::read(&self.store, owner_id, name, None).await?;
        if let Some(program) = self.get(owner_id, name) {
            for row in used.iter_mut().filter(|row| row.kind == "tool") {
                row.public = Some(program.public(&row.item));
            }
        }
        Ok(used)
    }

    /// `owner_id/name`'s logs, oldest first, after the sequence number
    /// `after`, those carrying `topic` when one is given. At most `limit`, held
    /// to [`MAX_PAGE`].
    pub async fn logs(
        &self,
        owner_id: &str,
        name: &str,
        topic: Option<String>,
        after: i64,
        limit: i64,
    ) -> Result<Vec<Logged>, Denied> {
        self.deployed(owner_id, name).await?;
        Ok(service::logs(
            &self.store,
            owner_id,
            name,
            topic,
            after,
            limit.clamp(1, MAX_PAGE),
        )
        .await?)
    }

    /// A call from a service on a peer, run as `link` and committed here. A
    /// refusal is `403` with its reason.
    pub async fn link(
        &self,
        owner_id: &str,
        name: &str,
        tool: &str,
        args: Vec<u8>,
        link: Link,
    ) -> Result<Reply, Denied> {
        let program = self.get(owner_id, name).ok_or_else(|| {
            deny(
                StatusCode::NOT_FOUND,
                format!("no program named {name:?} is deployed"),
            )
        })?;
        let refused = |message: String| {
            self.usage.record(
                &program.name,
                Kind::Tool,
                tool,
                Source::Service,
                &link.from,
                Outcome::Refused,
            );
            Err(deny(StatusCode::FORBIDDEN, message))
        };
        if !program.public(tool) {
            return refused(format!("{name} has no public tool named {tool:?}"));
        }
        if let Some(caller) = &link.caller {
            if !program.reached_by_programs(tool) {
                return refused(format!("{name}.{tool} takes no calls from programs"));
            }
            if !self
                .allowlists
                .allows(&program.name, tool, "service", caller)
            {
                return refused(format!(
                    "{name}.{tool} does not take calls from this service"
                ));
            }
        }
        let _slot = self.gate.acquire().await.map_err(anyhow::Error::from)?;
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
                    deadline: Instant::now() + Duration::from_millis(link.timeout),
                    stack: link
                        .stack
                        .iter()
                        .map(|program| Arc::from(program.as_str()))
                        .collect(),
                },
                tool,
                args,
            )
            .await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => match error.downcast::<Refused>() {
                Ok(Refused(message)) => return refused(message),
                Err(error) => return Err(error.into()),
            },
        };
        if outcome.result.is_ok() {
            self.commits.commit(&outcome.changes, None).await?;
        }
        let ended = match (&outcome.result, outcome.denied) {
            (Ok(_), _) => Outcome::Done,
            (Err(_), true) => Outcome::Denied,
            (Err(_), false) => Outcome::Failed,
        };
        self.usage.record(
            &program.name,
            Kind::Tool,
            tool,
            Source::Service,
            &link.from,
            ended,
        );
        Ok(Reply {
            result: outcome.result,
            denied: outcome.denied,
        })
    }

    /// Hold `routes` in place of the routes held.
    pub async fn route(&self, routes: Vec<Route>) -> Result<(), Denied> {
        Ok(self.routes.replace(routes).await?)
    }

    /// Add uses a peer counted of services this host runs.
    pub async fn record(&self, used: Vec<Used>) -> Result<(), Denied> {
        Ok(usage::write(&self.store, used).await?)
    }

    /// Refuse an outside caller before the tool runs, for its manifest's
    /// kinds, an app's anonymous call to a tool that is not a query, the
    /// owner's allowlist, or its calls this minute. Counted as refused.
    fn admit(
        &self,
        caller: &Caller,
        program: &Program,
        tool: &str,
        query: bool,
    ) -> Result<(), Denied> {
        let (source, who) = counted(caller);
        let refused = |status: StatusCode, message: String| {
            self.usage.record(
                &program.name,
                Kind::Tool,
                tool,
                source,
                who,
                Outcome::Refused,
            );
            Err(deny(status, message))
        };
        if !program.reached_by_accounts(tool) {
            return refused(
                StatusCode::FORBIDDEN,
                format!("{tool} takes calls from services only"),
            );
        }
        if caller.app.is_some() && caller.user.is_none() && !query {
            return refused(
                StatusCode::FORBIDDEN,
                format!("an anonymous call reaches query tools only, and {tool} is not one"),
            );
        }
        let kind = match source {
            Source::App => "app",
            _ => "account",
        };
        if !self.allowlists.allows(&program.name, tool, kind, who) {
            return refused(
                StatusCode::FORBIDDEN,
                format!("this {kind} may not call {tool}"),
            );
        }
        let (counter, limit) = match (&caller.app, &caller.user) {
            (Some(app), Some(user)) => (
                format!("{app} {} {}", user.issuer, user.subject),
                self.limits.caller,
            ),
            (Some(app), None) => (
                format!("{app} anonymous {}", caller.client.as_deref().unwrap_or("")),
                self.limits.anonymous,
            ),
            (None, _) => (caller.id.clone(), self.limits.caller),
        };
        if !self.counter.take(&counter, limit) {
            return refused(
                StatusCode::TOO_MANY_REQUESTS,
                "too many calls this minute; retry in the next".to_owned(),
            );
        }
        Ok(())
    }

    /// The program `owner_id/name` runs now, and its commit count.
    async fn deployed(&self, owner_id: &str, name: &str) -> Result<(Arc<Program>, i64), Denied> {
        let missing = || deny(StatusCode::NOT_FOUND, "no such service");
        let commits = service::commits(&self.store, owner_id, name)
            .await?
            .ok_or_else(missing)?;
        let program = self.get(owner_id, name).ok_or_else(missing)?;
        Ok((program, commits))
    }
}

/// How a caller from outside is counted: by its app, or by its account.
fn counted(caller: &Caller) -> (Source, &str) {
    match &caller.app {
        Some(app) => (Source::App, app),
        None => (Source::Account, &caller.id),
    }
}
