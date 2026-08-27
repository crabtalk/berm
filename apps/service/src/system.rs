//! The system harnesses bermd serves.
//!
//! One: `berm.call`, which reaches a tool on another deployed harness. Every
//! deployed harness gets it, and the target is named in the request rather than
//! wired at deploy — so an image works against whatever it was deployed beside,
//! and the same image deployed twice under different names is reachable as
//! both.
//!
//! What is deployed is reachable. That is the same reach containers on one
//! network have of each other, and it is bounded the same way: by what the
//! operator chose to run. It says nothing about the world outside, which a
//! harness still reaches only through what its host registered — and bermd
//! registers nothing else.

use crate::{Service, store as disk};
use berm::{Harness, Refused};
use berm_system::{call, store};

impl Service {
    /// What every deployed harness is given.
    pub(crate) fn system(&self) -> Vec<Harness> {
        let service = self.me.clone();
        let mut system = vec![call::harness(self.depth, move |harness, tool, args| {
            let Some(service) = service.upgrade() else {
                return Err(Refused(format!(
                    "the service is shutting down, so {harness}.{tool} cannot run"
                ))
                .into());
            };

            // `try_read` rather than a blocking one: this runs inside the
            // nounwind boundary a system harness is called across, and tokio's
            // blocking acquire panics outright when it is reached from a
            // runtime thread. Contention here means a deploy is mid-flight,
            // which is worth reporting rather than dying of.
            let deployed = {
                let Ok(deployed) = service.deployed.try_read() else {
                    return Err(Refused(format!(
                        "the deployed set is being written; {harness}.{tool} was not reached"
                    ))
                    .into());
                };
                // Cloned out and the guard dropped here, before the guest below
                // runs: holding it across a nested call would block the next
                // deploy for as long as that call takes.
                deployed.get(harness).cloned()
            };

            let Some(deployed) = deployed else {
                return Err(Refused(format!("no harness named {harness:?} is deployed")).into());
            };

            // Synchronous on purpose. This thread is already the one
            // `Service::call` handed to `spawn_blocking`, and going back
            // through the runtime would cost a blocking thread per level of
            // nesting.
            deployed.berm.call(tool, args.as_bytes().to_vec())
        })];

        // The root rather than the service: reading a key needs nothing the
        // service holds, so these outlive a shutdown that `berm.call` cannot.
        let (reading, writing) = (self.root.clone(), self.root.clone());
        system.extend(store::harnesses(
            move |harness, key| disk::read(&reading, harness, key),
            move |harness, key, value| disk::write(&writing, harness, key, value),
        ));
        system
    }
}
