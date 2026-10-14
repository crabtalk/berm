//! Logs as a source of calls: a service subscribed with `#[logs(...)]` has its
//! tool called with the logs of each service in its scope it subscribes to,
//! oldest first, and its position commits with what that call wrote.
//!
//! Each subscription has a worker of its own, so one subscriber failing or
//! running long holds up no other. A publisher on a peer is read from that
//! peer.

use crate::{
    env::{Env, Peers},
    host::{self, Advance, Host},
    service,
    usage::{Kind, Source},
};
use anyhow::{Result, bail};
use berm::Program;
use berm_api::{Delivery, Logged, Subscription};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::Notify, task::JoinHandle};

/// Logs read per delivery.
const BATCH: i64 = 64;
/// How often a caught-up worker looks again unprompted. A publisher on a peer
/// prompts nothing, so this is how soon its logs arrive.
const POLL: Duration = Duration::from_secs(5);
/// The first wait after a failed pass, doubled per failure up to [`BACKOFF`].
const RETRY: Duration = Duration::from_secs(1);
const BACKOFF: Duration = Duration::from_secs(300);

/// Each subscription's worker, by subscriber key and publisher key.
#[derive(Default)]
pub(crate) struct Deliveries(Mutex<HashMap<(String, String), Worker>>);

struct Worker {
    prompt: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Deliveries {
    /// Prompt the workers of every subscriber in `scope`.
    pub(crate) fn released(&self, scope: &str) {
        for ((subscriber, _), worker) in self.0.lock().iter() {
            if host::split(subscriber).0 == scope {
                worker.prompt.notify_one();
            }
        }
    }

    /// Prompt the workers of every subscription to the service keyed
    /// `publisher`.
    pub(crate) fn published(&self, publisher: &str) {
        for ((_, of), worker) in self.0.lock().iter() {
            if of == publisher {
                worker.prompt.notify_one();
            }
        }
    }
}

impl<E: Env> Host<E> {
    /// Run a worker for every subscription of the services this host runs,
    /// and stop those whose subscription is gone.
    pub(crate) fn subscribe(&self) {
        let mut wanted = HashSet::new();
        for subscriber in self.berm.list() {
            let (scope, _) = host::split(&subscriber.name);
            for subscription in &subscriber.manifest().deps.logs {
                let publisher = host::join(scope, &subscription.program);
                wanted.insert((subscriber.name.to_string(), publisher));
            }
        }
        let mut workers = self.deliveries.0.lock();
        workers.retain(|key, worker| {
            let kept = wanted.contains(key) && !worker.task.is_finished();
            if !kept {
                worker.task.abort();
            }
            kept
        });
        for key in wanted {
            if workers.contains_key(&key) {
                continue;
            }
            let prompt = Arc::new(Notify::new());
            let (subscriber, publisher) = key.clone();
            let task = tokio::spawn(self.clone().work(subscriber, publisher, prompt.clone()));
            workers.insert(key, Worker { prompt, task });
        }
    }

    /// Deliver `publisher`'s logs to `subscriber` while the version it runs
    /// subscribes to them: a pass now, then one on each prompt or after
    /// [`POLL`], and after a failed pass, one once the backoff has run.
    async fn work(self, subscriber: String, publisher: String, prompt: Arc<Notify>) {
        let mut backoff = RETRY;
        let (_, name) = host::split(&publisher);
        loop {
            let Some(program) = self.berm.get(&subscriber) else {
                return;
            };
            let Some(of) = program
                .manifest()
                .deps
                .logs
                .iter()
                .find(|subscription| subscription.program == name)
                .cloned()
            else {
                return;
            };
            let host = self.clone();
            // Its own task, so a panic fails one pass rather than the worker.
            let pass = tokio::spawn(async move { host.catch_up(&program, &of).await }).await;
            let failure = match pass {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(format!("{e:#}")),
                Err(e) => Some(format!("the pass panicked: {e}")),
            };
            match failure {
                None => {
                    backoff = RETRY;
                    tokio::select! {
                        _ = prompt.notified() => {}
                        _ = tokio::time::sleep(POLL) => {}
                    }
                }
                Some(failure) => {
                    tracing::warn!(subscriber, publisher, "delivery stopped: {failure}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF);
                }
            }
        }
    }

    /// Deliver what `subscriber` has not had of `subscription`, stopping at the
    /// first delivery that fails or once its scope is held.
    async fn catch_up(&self, subscriber: &Program, subscription: &Subscription) -> Result<()> {
        let (scope, name) = host::split(&subscriber.name);
        let program = &subscription.program;
        let publisher = host::join(scope, program);
        let mut from = service::position(&self.store, scope, name, program).await?;
        loop {
            if self.holds.holds(scope) {
                return Ok(());
            }
            let pending = self.read(scope, program, &publisher, from).await?;
            let Some(to) = pending.last().map(|log| log.seq as i64) else {
                return Ok(());
            };
            let logs: Vec<_> = pending
                .into_iter()
                .filter(|log| subscription.matches(&log.topics))
                .collect();
            let advance = Advance {
                scope: scope.to_owned(),
                name: name.to_owned(),
                program: program.clone(),
                from,
                to,
            };
            let committed = if logs.is_empty() {
                self.commits
                    .commit(&Default::default(), Some(advance))
                    .await?
            } else {
                let topics: Vec<String> = logs
                    .iter()
                    .flat_map(|log| log.topics.iter().cloned())
                    .collect();
                let delivery = Delivery {
                    program: program.clone(),
                    logs,
                };
                let args = serde_json::to_vec(&delivery)?;
                let _slot = self.gate.acquire().await?;
                if self.holds.holds(scope) {
                    return Ok(());
                }
                let invoked = self
                    .invoke(
                        subscriber,
                        None,
                        None,
                        &subscription.tool,
                        args,
                        Some(advance),
                        None,
                    )
                    .await?;
                let outcome = invoked.outcome();
                for topic in &topics {
                    self.usage.record(
                        &publisher,
                        Kind::Event,
                        topic,
                        Source::Service,
                        &subscriber.name,
                        outcome,
                    );
                }
                if let Err(failure) = invoked.result {
                    bail!("{} failed a delivery: {failure}", subscription.tool);
                }
                invoked.committed
            };
            from = match committed {
                true => to,
                false => service::position(&self.store, scope, name, program).await?,
            };
        }
    }

    /// `program`'s logs after `from`, from this host or the peer it runs on.
    async fn read(
        &self,
        scope: &str,
        program: &str,
        publisher: &str,
        from: i64,
    ) -> Result<Vec<Logged>> {
        if self.berm.get(publisher).is_some() {
            return Ok(service::logs(&self.store, scope, program, None, from, 0, BATCH)
                .await?
                .0);
        }
        let Some(peer) = self.routes.peer(publisher) else {
            bail!("no service named {program:?} is deployed");
        };
        self.peers.logs(&peer, publisher, from, BATCH).await
    }
}
