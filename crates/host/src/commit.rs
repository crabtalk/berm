//! Committing what a call wrote. Every invocation commits its own service's
//! writes when it returns ok: a service keeping a database wrote into it as it
//! ran, and the rest goes to the host's store. The chain's first call hands
//! its writes to the host, which commits them.

use crate::{host::split, service, store::Store};
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

/// Where commits go.
#[derive(Clone)]
pub(crate) struct Commits {
    store: Store,
    databases: Arc<Databases>,
    /// What each dry chain's links hold, by chain, until its outermost link on
    /// this host ends.
    kept: Arc<parking_lot::Mutex<HashMap<u64, Kept>>>,
}

/// What one dry chain's ended links hold on this host.
#[derive(Default)]
struct Kept {
    held: Vec<Held>,
    /// The last value a link set under each service's key, for services
    /// without a database.
    writes: HashMap<(String, String), Vec<u8>>,
}

impl Commits {
    pub(crate) fn new(store: Store, databases: Arc<Databases>) -> Self {
        Self {
            store,
            databases,
            kept: Arc::default(),
        }
    }

    /// What an ended link of dry `chain` last set under `program`'s `key`.
    pub(crate) fn kept(&self, chain: u64, program: &str, key: &str) -> Option<Vec<u8>> {
        let kept = self.kept.lock();
        kept.get(&chain)?
            .writes
            .get(&(program.to_owned(), key.to_owned()))
            .cloned()
    }

    /// Commit `changes`' writes, each service's in one transaction.
    pub(crate) async fn commit(&self, changes: &Changes) -> Result<()> {
        if changes.writes.is_empty() {
            return Ok(());
        }
        let mut shares: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
        for write in &changes.writes {
            shares
                .entry(write.program.to_string())
                .or_default()
                .push((write.key.clone(), write.value.clone()));
        }
        self.store
            .run(move |conn| {
                let tx = conn.transaction()?;
                for (key, writes) in &shares {
                    let (scope, name) = split(key);
                    service::commit(&tx, scope, name, writes)?;
                }
                tx.commit()
            })
            .await
    }
}

/// Each invocation holds what [`Databases::enter`] gives it, and its own
/// journal. One that took a service's writer commits that database when it
/// returns ok; unless it is the chain's first, the rest of what it wrote is
/// committed then.
///
/// In a dry chain nothing is committed: each link's hold and writes are kept
/// until the chain's outermost link on this host ends, then every one is
/// rolled back.
pub(crate) fn enter(commits: Commits) -> Enter {
    Arc::new(move |program: &Program, at: &Callsite| {
        let (commits, entering) = (commits.clone(), commits.databases.enter(program, at));
        let first = at.depth == 1;
        let keep = at.dry.as_ref().and_then(|_| {
            let mut kept = commits.kept.lock();
            (!kept.contains_key(&at.chain)).then(|| {
                kept.insert(at.chain, Kept::default());
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
    /// The chain's first call, whose changes the host commits.
    first: bool,
    /// Set in a dry chain: its number, and for its outermost link on this
    /// host, what drops every hold the chain kept.
    dry: Option<(u64, Option<Keep>)>,
}

/// Drops every hold a dry chain kept, which rolls each back.
struct Keep {
    kept: Arc<parking_lot::Mutex<HashMap<u64, Kept>>>,
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
                let Some(keep) = keep else {
                    if let Some(kept) = commits.kept.lock().get_mut(&chain) {
                        kept.held.push(held);
                        for write in changes.writes {
                            kept.writes
                                .insert((write.program.to_string(), write.key), write.value);
                        }
                    }
                    return Ok(Changes::default());
                };
                drop((keep, held));
                return Ok(match first {
                    true => changes,
                    false => Changes::default(),
                });
            }
            let took = held.took().map(str::to_owned);
            held.commit().await?;
            if let Some(key) = took {
                let (scope, name) = split(&key);
                let (scope, name) = (scope.to_owned(), name.to_owned());
                commits
                    .store
                    .run(move |conn| {
                        let tx = conn.transaction()?;
                        service::commit(&tx, &scope, &name, &[])?;
                        tx.commit()
                    })
                    .await?;
            }
            if first {
                return Ok(changes);
            }
            commits.commit(&changes).await?;
            Ok(Changes::default())
        })
    }
}
