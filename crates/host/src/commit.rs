//! Committing what a call wrote and emitted. Every invocation commits its own
//! service's share when it returns ok: a service keeping a database wrote into
//! it as it ran, and the rest goes to the host's store. The chain's first
//! call hands its share to the host, which commits it with a delivery's
//! position.

use crate::{
    deliver::Deliveries,
    host::{Advance, split},
    service::{self, Share},
    store::Store,
};
use anyhow::Result;
use berm::{
    Callsite, Changes, Enter, Program, Scope,
    database::{Databases, Held},
};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::Mutex;

/// Logs copied from an outbox per transaction.
const SHIP: i64 = 256;

/// Where commits go.
#[derive(Clone)]
pub(crate) struct Commits {
    store: Store,
    databases: Arc<Databases>,
    /// Held while an outbox is copied to `service_logs`.
    shipping: Arc<Mutex<()>>,
    /// Told which services committed logs.
    deliveries: Arc<Deliveries>,
    /// What each dry chain's links hold, by chain, until its outermost link on
    /// this host ends.
    kept: Arc<parking_lot::Mutex<HashMap<u64, Vec<Held>>>>,
}

impl Commits {
    pub(crate) fn new(
        store: Store,
        databases: Arc<Databases>,
        deliveries: Arc<Deliveries>,
    ) -> Self {
        Self {
            store,
            databases,
            shipping: Arc::new(Mutex::new(())),
            deliveries,
            kept: Arc::default(),
        }
    }

    /// Copy what waits in the outbox of the database keyed `key` to
    /// `service_logs`, numbered there, and drop it from the outbox. A crash in
    /// between copies those logs again. `false` when the outbox was empty.
    pub(crate) async fn ship(&self, key: &str) -> Result<bool> {
        let _shipping = self.shipping.lock().await;
        let database = self.databases.get(key)?;
        let (scope, name) = split(key);
        let mut shipped = false;
        loop {
            let waiting = database.outbox(SHIP).await?;
            let Some(last) = waiting.last().map(|log| log.id) else {
                break;
            };
            let share = Share {
                writes: Vec::new(),
                logs: waiting
                    .into_iter()
                    .map(|log| (log.topics, log.data))
                    .collect(),
            };
            let (scope, name) = (scope.to_owned(), name.to_owned());
            self.store
                .run(move |conn| {
                    let tx = conn.transaction()?;
                    service::commit(&tx, &scope, &name, &share)?;
                    tx.commit()
                })
                .await?;
            database.shipped(last).await?;
            shipped = true;
        }
        if shipped {
            self.deliveries.published(key);
        }
        Ok(shipped)
    }

    /// Commit `changes` and, for a delivery, the subscriber's position, in one
    /// transaction. `false`, with nothing committed, when the position no
    /// longer stood where the delivery read from.
    pub(crate) async fn commit(&self, changes: &Changes, advance: Option<Advance>) -> Result<bool> {
        if changes.writes.is_empty() && changes.logs.is_empty() && advance.is_none() {
            return Ok(true);
        }
        let mut shares: BTreeMap<String, Share> = BTreeMap::new();
        for write in &changes.writes {
            shares
                .entry(write.program.to_string())
                .or_default()
                .writes
                .push((write.key.clone(), write.value.clone()));
        }
        for log in &changes.logs {
            shares
                .entry(log.program.to_string())
                .or_default()
                .logs
                .push((log.topics.clone(), log.data.clone()));
        }
        let published: Vec<String> = shares
            .iter()
            .filter(|(_, share)| !share.logs.is_empty())
            .map(|(key, _)| key.clone())
            .collect();
        let committed = self
            .store
            .run(move |conn| {
                let tx = conn.transaction()?;
                if let Some(Advance {
                    scope,
                    name,
                    program,
                    from,
                    to,
                }) = advance
                    && !service::advance(&tx, &scope, &name, &program, from, to)?
                {
                    return Ok(false);
                }
                for (key, share) in &shares {
                    let (scope, name) = split(key);
                    service::commit(&tx, scope, name, share)?;
                }
                tx.commit()?;
                Ok(true)
            })
            .await?;
        if committed {
            for key in &published {
                self.deliveries.published(key);
            }
        }
        Ok(committed)
    }
}

/// Each invocation holds what [`Databases::enter`] gives it, and its own
/// journal. One that took a service's writer commits that database when it
/// returns ok, and its logs are shipped; unless it is the chain's first, the
/// rest of what it wrote and emitted is committed then.
///
/// In a dry chain nothing is committed: each link's hold is kept until the
/// chain's outermost link on this host ends, then every one is rolled back.
// TODO: a service without a database reads `berm.get` from what was
// committed, so in a dry chain it does not read what an earlier link of the
// chain set.
pub(crate) fn enter(commits: Commits) -> Enter {
    Arc::new(move |program: &Program, at: &Callsite| {
        let (commits, entering) = (commits.clone(), commits.databases.enter(program, at));
        let first = at.depth == 1;
        let keep = at.dry.as_ref().and_then(|_| {
            let mut kept = commits.kept.lock();
            (!kept.contains_key(&at.chain)).then(|| {
                kept.insert(at.chain, Vec::new());
                Keep {
                    kept: commits.kept.clone(),
                    chain: at.chain,
                }
            })
        });
        let (dry, chain) = (at.dry.is_some(), at.chain);
        Box::pin(async move {
            let held = entering.await?;
            Ok(Box::new(Entered {
                commits,
                held,
                first,
                dry: dry.then_some((chain, keep)),
            }) as Box<dyn Scope>)
        })
    })
}

/// One invocation's hold.
struct Entered {
    commits: Commits,
    held: Held,
    /// The chain's first call, whose changes the host commits with a delivery's
    /// position.
    first: bool,
    /// Set in a dry chain: its number, and for its outermost link on this
    /// host, what drops every hold the chain kept.
    dry: Option<(u64, Option<Keep>)>,
}

/// Drops every hold a dry chain kept, which rolls each back.
struct Keep {
    kept: Arc<parking_lot::Mutex<HashMap<u64, Vec<Held>>>>,
    chain: u64,
}

impl Drop for Keep {
    fn drop(&mut self) {
        let kept = self.kept.lock().remove(&self.chain);
        drop(kept);
    }
}

impl Scope for Entered {
    fn own(&self) -> bool {
        true
    }

    fn close(
        self: Box<Self>,
        changes: Changes,
    ) -> Pin<Box<dyn Future<Output = Result<Changes>> + Send>> {
        Box::pin(async move {
            let Entered {
                commits,
                held,
                first,
                dry,
            } = *self;
            if let Some((chain, keep)) = dry {
                match keep {
                    Some(keep) => drop((keep, held)),
                    None => {
                        if let Some(kept) = commits.kept.lock().get_mut(&chain) {
                            kept.push(held);
                        }
                    }
                }
                return Ok(match first {
                    true => changes,
                    false => Changes::default(),
                });
            }
            let took = held.took().map(str::to_owned);
            held.commit().await?;
            if let Some(key) = took
                && !commits.ship(&key).await?
            {
                let (scope, name) = split(&key);
                let (scope, name) = (scope.to_owned(), name.to_owned());
                commits
                    .store
                    .run(move |conn| {
                        let tx = conn.transaction()?;
                        service::commit(&tx, &scope, &name, &Share::default())?;
                        tx.commit()
                    })
                    .await?;
            }
            if first {
                return Ok(changes);
            }
            commits.commit(&changes, None).await?;
            Ok(Changes::default())
        })
    }
}
