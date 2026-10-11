//! Time as a source of calls: `berm.call.after` holds one wake per service in
//! `service_wakes`, and the timer calls each when it is due.

use crate::{
    env::{Env, Peers},
    host::{self, Host},
    routes::Routes,
    service,
    store::Store,
    usage::{Kind, Source},
};
use anyhow::{Result, bail};
use berm::{Berm, Callsite, Syscall, syscall::after};
use berm_api::host::{Link, Reply};
use std::{
    sync::{Arc, OnceLock, Weak},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;

/// Milliseconds a claimed wake is held before the timer may take it again.
const LEASE: i64 = 5 * 60_000;
/// Milliseconds after a failed wake before it fires again.
const RETRY: i64 = 60_000;
/// Wakes claimed at once.
const CLAIM: i64 = 32;
/// Milliseconds a wake fired on a peer has to finish in.
const TIMEOUT: u64 = 60_000;

/// `berm.call.after`: hold the wake for the service arming it, once its target
/// is one of its own tools or a public tool of a deployed service of the same
/// owner. A target on a peer is checked there when the wake fires.
pub(crate) fn syscall(
    store: Store,
    runtime: Arc<OnceLock<Weak<Berm>>>,
    routes: Arc<Routes>,
    waking: Arc<Notify>,
) -> Syscall {
    after::programs(move |at: Callsite, wake: after::Wake| {
        let (store, runtime, routes, waking) = (
            store.clone(),
            runtime.clone(),
            routes.clone(),
            waking.clone(),
        );
        async move {
            if wake.program.contains('/') {
                bail!("a service wakes services of its own owner, by name");
            }
            let (owner_id, name) = host::unkey(&at.program);
            let key = host::key(owner_id, &wake.program);
            let target = runtime
                .get()
                .and_then(Weak::upgrade)
                .and_then(|berm| berm.get(&key));
            let own = wake.program == name;
            let wakeable = match target {
                Some(target) => target
                    .manifest()
                    .tools
                    .iter()
                    .any(|spec| spec.name == wake.tool && (own || spec.public)),
                None if routes.peer(&key).is_some() => true,
                None => bail!("no service named {:?} is deployed", wake.program),
            };
            if !wakeable {
                bail!(
                    "{} has no tool named {:?} this service may wake",
                    wake.program,
                    wake.tool
                );
            }
            let due = now().saturating_add(i64::try_from(wake.delay).unwrap_or(i64::MAX));
            service::arm(
                &store,
                owner_id,
                name,
                &wake.program,
                &wake.tool,
                &wake.args,
                due,
            )
            .await?;
            waking.notify_one();
            Ok(())
        }
    })
}

impl<E: Env> Host<E> {
    /// Fire due wakes until the process ends, sleeping until the next is due or
    /// one is armed.
    pub(crate) async fn wakes(&self) {
        loop {
            match service::claim(&self.store, now(), now() + LEASE, CLAIM).await {
                Ok(due) => {
                    for wake in due {
                        let host = self.clone();
                        tokio::spawn(async move { host.fire(wake).await });
                    }
                }
                Err(e) => tracing::error!("claiming wakes failed: {e:#}"),
            }
            let wait = match service::next(&self.store).await {
                Ok(Some(due)) => Some((due - now()).max(0)),
                Ok(None) => None,
                Err(e) => {
                    tracing::error!("reading wakes failed: {e:#}");
                    Some(RETRY)
                }
            };
            match wait {
                Some(wait) => tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(wait as u64)) => {}
                    _ = self.waking.notified() => {}
                },
                None => self.waking.notified().await,
            }
        }
    }

    /// Call one wake's tool with no caller and commit what it did, here or on
    /// the peer its target runs on. A wake that fails fires again after
    /// [`RETRY`].
    async fn fire(&self, wake: service::Wake) {
        let fired = async {
            let key = host::key(&wake.owner_id, &wake.program);
            let from = host::key(&wake.owner_id, &wake.name);
            let Some(target) = self.berm.get(&key) else {
                let Some(peer) = self.routes.peer(&key) else {
                    bail!("no service named {:?} is deployed", wake.program);
                };
                let link = Link {
                    caller: None,
                    from,
                    user: None,
                    depth: 1,
                    query: false,
                    timeout: TIMEOUT,
                    stack: vec![key],
                };
                let Reply { result, .. } = self
                    .peers
                    .link(
                        &peer,
                        &wake.owner_id,
                        &wake.program,
                        &wake.tool,
                        wake.args.clone().into_bytes(),
                        link,
                    )
                    .await?;
                if let Err(failure) = result {
                    bail!("{}: {failure}", wake.tool);
                }
                return Ok(());
            };
            let _slot = self.gate.acquire().await?;
            let invoked = self
                .invoke(
                    &target,
                    None,
                    None,
                    &wake.tool,
                    wake.args.clone().into_bytes(),
                    None,
                )
                .await?;
            self.usage.record(
                &target.name,
                Kind::Tool,
                &wake.tool,
                Source::Service,
                &from,
                invoked.outcome(),
            );
            if let Err(failure) = invoked.result {
                bail!("{}: {failure}", wake.tool);
            }
            Result::<()>::Ok(())
        };
        let retry = match fired.await {
            Ok(()) => None,
            Err(e) => {
                tracing::warn!(
                    service = wake.name,
                    program = wake.program,
                    "wake failed: {e:#}"
                );
                Some(now() + RETRY)
            }
        };
        if let Err(e) = service::settle(&self.store, &wake, retry).await {
            tracing::error!(service = wake.name, "settling a wake failed: {e:#}");
        }
    }
}

/// Milliseconds since the Unix epoch.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}
