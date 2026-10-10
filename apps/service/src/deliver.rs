//! Logs as a source of invocations.
//!
//! A program subscribed with `#[logs(...)]` has its tool called with each
//! program's logs it has not had yet, in batches, oldest first. Its position
//! advances once its own call is committed, so a crash in between delivers a
//! batch again rather than skipping it.

use crate::{Service, logs};
use anyhow::{Result, bail};
use berm::{Delivery, Logged, Subscription};

/// Logs in one delivery.
const BATCH: usize = 64;

impl Service {
    /// Run a pass over every subscription, after whichever pass is running.
    pub(crate) fn deliver(&self) {
        let Some(service) = self.me.upgrade() else {
            return;
        };
        tokio::spawn(async move {
            let _turn = service.delivering.lock().await;
            for subscriber in service.list() {
                for subscription in &subscriber.manifest().deps.logs {
                    if let Err(error) = service.catch_up(&subscriber.name, subscription).await {
                        tracing::warn!(
                            subscriber = &*subscriber.name,
                            program = subscription.program,
                            "{error:#}"
                        );
                    }
                }
            }
        });
    }

    /// Deliver what `subscriber` has not had of `subscription`, stopping at the
    /// first delivery that fails.
    async fn catch_up(&self, subscriber: &str, subscription: &Subscription) -> Result<()> {
        let program = &subscription.program;
        let position = logs::position(&self.root, subscriber, program)?;
        let pending = logs::after(&self.root, program, position)?;
        let Some(last) = pending.last().map(|log| log.seq) else {
            return Ok(());
        };
        let matching: Vec<Logged> = pending
            .into_iter()
            .filter(|log| subscription.matches(&log.topics))
            .collect();
        for batch in matching.chunks(BATCH) {
            let delivery = Delivery {
                program: program.clone(),
                logs: batch.to_vec(),
            };
            let args = serde_json::to_vec(&delivery)?;
            if let Err(failure) = self.call(subscriber, &subscription.tool, args).await? {
                bail!(
                    "{subscriber}.{} failed a delivery: {failure}",
                    subscription.tool
                );
            }
            let seq = batch.last().map_or(position, |log| log.seq);
            logs::advance(&self.root, subscriber, program, seq)?;
        }
        logs::advance(&self.root, subscriber, program, last)
    }
}
