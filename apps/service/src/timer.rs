//! Time as a source of invocations.
//!
//! A harness asks to have a tool called in so many milliseconds and returns.
//! When the time comes the tool runs as a fresh invocation, holding nothing
//! from the one that armed it — what has to cross the gap goes in
//! `berm.get`/`berm.set`, and how wide the gap really was is `berm.now`.
//!
//! One wake per harness that arms it. Arming again replaces what was pending,
//! so the count of these is bounded by the deployed set and no harness can fan
//! out. A harness wanting several timers keeps them in its own keys and arms
//! for the earliest.
//!
//! The target may be any deployed harness, the way `berm.call` reaches any of
//! them. The *slot* belongs to whoever armed it, so pointing one at `a.b`
//! leaves whatever `a` armed for itself alone.

use crate::{Service, utils};
use anyhow::{Context, Result, bail};
use berm::{Callsite, System, abi, wire};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{runtime::Handle, task::JoinHandle};

/// What is written down about one pending wake.
#[derive(Serialize, Deserialize, Clone)]
struct Wake {
    /// Milliseconds since the Unix epoch. Absolute on disk: a delay would have
    /// to be measured against a moment the file does not record, and a restart
    /// is exactly when that moment is gone.
    at: u64,
    harness: String,
    tool: String,
    args: String,
}

/// One pending wake per harness, and the task holding each.
#[derive(Default)]
pub(crate) struct Wakes {
    armed: Mutex<HashMap<String, JoinHandle<()>>>,
}

/// `berm.call.after`, against `service`.
pub(crate) fn system(service: Weak<Service>, runtime: Handle) -> System {
    System {
        name: abi::CALL_AFTER.to_owned(),
        call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
            let fields = wire::fields(request)?;
            let after: u64 = wire::text(&fields, 0, "delay")?
                .parse()
                .context("a delay is milliseconds")?;
            let wake = Wake {
                at: now().saturating_add(after),
                harness: wire::text(&fields, 1, "harness")?.to_owned(),
                tool: wire::text(&fields, 2, "tool")?.to_owned(),
                args: wire::text(&fields, 3, "arguments")?.to_owned(),
            };

            let Some(service) = service.upgrade() else {
                bail!("the service is shutting down, so nothing was armed");
            };
            service.arm(at.harness, wake, &runtime)
        }),
    }
}

impl Service {
    /// Hold `wake` for `owner`, replacing whatever it had pending.
    fn arm(self: &Arc<Self>, owner: &str, wake: Wake, runtime: &Handle) -> Result<Vec<u8>> {
        // Checked here, where the harness that armed it is still running and
        // can act on the answer. At firing time nobody is listening.
        let Some(target) = self.get(&wake.harness) else {
            bail!("no harness named {:?} is deployed", wake.harness);
        };
        if !target
            .manifest()
            .tools
            .iter()
            .any(|spec| spec.name == wake.tool)
        {
            bail!(
                "harness {:?} exports no tool named {:?}",
                wake.harness,
                wake.tool
            );
        }

        utils::write(&self.wake_record(owner), &serde_json::to_vec(&wake)?)
            .context("failed to write down a wake")?;
        self.hold(owner.to_owned(), wake, runtime);
        Ok(Vec::new())
    }

    /// Put a wake on a task, replacing the one already there.
    fn hold(self: &Arc<Self>, owner: String, wake: Wake, runtime: &Handle) {
        let service = Arc::downgrade(self);
        let task = {
            let owner = owner.clone();
            runtime.spawn(async move {
                // Saturating: a wake that came due while the process was down
                // has nothing left to wait for and fires late, which the
                // harness can see for itself against `berm.now`.
                tokio::time::sleep(Duration::from_millis(wake.at.saturating_sub(now()))).await;
                if let Some(service) = service.upgrade() {
                    // Given up before the tool runs, so a wake it arms for
                    // itself replaces nothing and survives.
                    service.retire(&owner);
                    service
                        .dispatch(&wake.harness, &wake.tool, wake.args.into_bytes())
                        .await;
                }
            })
        };

        if let Ok(mut armed) = self.wakes.armed.lock()
            && let Some(replaced) = armed.insert(owner, task)
        {
            replaced.abort();
        }
    }

    /// Give up a slot from inside the task that is firing it.
    ///
    /// Dropping the handle detaches. Aborting would cancel the caller — the
    /// handle names the task running this line, and the invocation it is on
    /// its way to would be dropped at the next await.
    fn retire(&self, owner: &str) {
        if let Ok(mut armed) = self.wakes.armed.lock() {
            armed.remove(owner);
        }
        self.erase_wake(owner);
    }

    /// Drop what `owner` had pending and stop whatever was waiting on it, for
    /// when the harness that armed it goes away.
    pub(crate) fn forget_wake(&self, owner: &str) {
        if let Ok(mut armed) = self.wakes.armed.lock()
            && let Some(task) = armed.remove(owner)
        {
            task.abort();
        }
        self.erase_wake(owner);
    }

    fn erase_wake(&self, owner: &str) {
        if let Err(error) = std::fs::remove_file(self.wake_record(owner))
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(owner, "failed to forget a wake: {error}");
        }
    }

    /// Take up every wake that was pending when this process last stopped.
    pub(crate) async fn rearm(self: &Arc<Self>) -> Result<()> {
        let directory = self.root.join("wakes");
        if !directory.is_dir() {
            return Ok(());
        }

        let runtime = Handle::current();
        let mut entries = tokio::fs::read_dir(&directory)
            .await
            .context("failed to read the wake directory")?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(owner) = path
                .extension()
                .filter(|extension| *extension == "json")
                .and_then(|_| path.file_stem())
                .and_then(|stem| stem.to_str())
            else {
                continue;
            };

            match tokio::fs::read(&path).await.map(|bytes| {
                serde_json::from_slice::<Wake>(&bytes).context("unreadable wake record")
            }) {
                Ok(Ok(wake)) => {
                    tracing::info!(owner, at = wake.at, "rearming");
                    self.hold(owner.to_owned(), wake, &runtime);
                }
                Ok(Err(error)) => tracing::error!(owner, "{error:#}"),
                Err(error) => tracing::error!(owner, "failed to read a wake: {error}"),
            }
        }
        Ok(())
    }

    fn wake_record(&self, owner: &str) -> PathBuf {
        self.root.join("wakes").join(format!("{owner}.json"))
    }
}

/// Milliseconds since the Unix epoch, as `berm.now` reads the same clock.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}
