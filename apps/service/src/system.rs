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

use crate::Service;
use anyhow::{Result, bail};
use berm::{Harness, Refused, wire};
use std::{cell::Cell, sync::Weak};

thread_local! {
    /// How many harnesses are already on this thread's stack.
    ///
    /// A nested call runs synchronously on the thread that entered the outer
    /// guest, so the depth is the thread's rather than anything the closure
    /// could be handed — `berm::Call` sees only the request bytes.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Counts one level for as long as it lives.
struct Level;

impl Level {
    /// `None` when the chain is already as deep as it may go.
    fn enter(limit: u32) -> Option<Self> {
        let depth = DEPTH.get();
        if depth >= limit {
            return None;
        }
        DEPTH.set(depth + 1);
        Some(Self)
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        DEPTH.set(DEPTH.get() - 1);
    }
}

impl Service {
    /// What every deployed harness is given.
    pub(crate) fn system(&self) -> Vec<Harness> {
        let (service, limit) = (self.me.clone(), self.depth);
        vec![Harness {
            name: berm::abi::CALL.to_owned(),
            call: std::sync::Arc::new(move |request: &[u8]| call(&service, limit, request)),
        }]
    }
}

/// Run one tool on another deployed harness.
///
/// A [`Refused`] means nothing ran — the chain is too deep, or nothing answers
/// to that name. Anything else is the target's own report, forwarded so the
/// caller can tell the two apart.
///
/// Nothing in this function may panic. It is called from inside compiled guest
/// code across an `extern "C"` boundary, where an unwind aborts the process
/// rather than failing the call.
fn call(service: &Weak<Service>, limit: u32, request: &[u8]) -> Result<Vec<u8>> {
    let fields = wire::fields(request)?;
    let harness = wire::text(&fields, 0, "harness")?;
    let tool = wire::text(&fields, 1, "tool")?;
    let args = wire::text(&fields, 2, "arguments")?;

    let Some(_level) = Level::enter(limit) else {
        return Err(Refused(format!(
            "call depth {limit} reached before {harness}.{tool}; a harness cannot nest deeper"
        ))
        .into());
    };

    let Some(service) = service.upgrade() else {
        return Err(Refused(format!(
            "the service is shutting down, so {harness}.{tool} cannot run"
        ))
        .into());
    };

    // `try_read` rather than a blocking one: this runs inside the nounwind
    // boundary above, and tokio's blocking acquire panics outright when it is
    // reached from a runtime thread. Contention here means a deploy is
    // mid-flight, which is worth reporting rather than dying of.
    let deployed = {
        let Ok(deployed) = service.deployed.try_read() else {
            return Err(Refused(format!(
                "the deployed set is being written; {harness}.{tool} was not reached"
            ))
            .into());
        };
        // Cloned out and the guard dropped here, before the guest below runs:
        // holding it across a nested call would block the next deploy for as
        // long as that call takes.
        deployed.get(harness).cloned()
    };

    let Some(deployed) = deployed else {
        return Err(Refused(format!("no harness named {harness:?} is deployed")).into());
    };

    // Synchronous on purpose. This thread is already the one `Service::call`
    // handed to `spawn_blocking`, and going back through the runtime would cost
    // a blocking thread per level of nesting.
    match deployed.berm.call(tool, args.as_bytes().to_vec())? {
        Ok(result) => Ok(result.into_bytes()),
        // The target ran and said no. Not a `Refused`: the caller is told the
        // difference, and may act on it.
        Err(failure) => bail!(failure),
    }
}
