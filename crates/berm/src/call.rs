//! `berm.call` — one harness reaching another.
//!
//! The one system harness berm serves itself. It needs nothing a host would
//! have to decide: the set of deployed harnesses is what berm already is, and
//! the target is named in the request rather than wired at deploy, so an image
//! works against whatever it was deployed beside.

use crate::{Berm, Callsite, Refused, System, abi, wire};
use anyhow::bail;
use std::sync::{Arc, Weak};

/// How deep a chain of harnesses calling harnesses may go before the next call
/// is refused. Zero turns composition off.
///
/// Not a bound on the native stack, which a nesting level costs ~720 bytes of
/// and would allow thousands: it bounds how far a mechanical composition can
/// run away from the turn that asked for it, and how much guest address space
/// one chain reserves — 64 MiB a level.
pub const DEFAULT_CALL_DEPTH: u32 = 4;

/// Serve `berm.call` against `runtime`.
pub(crate) fn system(runtime: Weak<Berm>, limit: u32) -> System {
    System {
        name: abi::CALL.to_owned(),
        call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
            let fields = wire::fields(request)?;
            let harness = wire::text(&fields, 0, "harness")?;
            let tool = wire::text(&fields, 1, "tool")?;
            let args = wire::text(&fields, 2, "arguments")?;

            if at.depth > limit {
                return Err(Refused(format!(
                    "call depth {limit} reached before {harness}.{tool}; a harness cannot nest deeper"
                ))
                .into());
            }

            let Some(runtime) = runtime.upgrade() else {
                return Err(Refused(format!(
                    "the runtime is shutting down, so {harness}.{tool} cannot run"
                ))
                .into());
            };

            // `try_read` rather than a blocking one: this runs inside the
            // nounwind boundary a system harness is called across, and a
            // blocking acquire here would deadlock against the deploy that
            // holds the write lock. Contention means a deploy is mid-flight,
            // which is worth reporting rather than dying of.
            let Ok(harnesses) = runtime.harnesses.try_read() else {
                return Err(Refused(format!(
                    "the deployed set is being written; {harness}.{tool} was not reached"
                ))
                .into());
            };
            // Cloned out and the guard dropped before the guest below runs:
            // holding it across a nested call would block the next deploy for
            // as long as that call takes.
            let target = harnesses.get(harness).cloned();
            drop(harnesses);

            let Some(target) = target else {
                return Err(Refused(format!("no harness named {harness:?} is deployed")).into());
            };

            // Synchronous on purpose: this thread is already the one that
            // entered the calling guest, and going elsewhere would cost a
            // thread per level of nesting.
            match target.call(tool, args.as_bytes().to_vec())? {
                Ok(result) => Ok(result.into_bytes()),
                // The target ran and said no. Not a `Refused`: the caller is
                // told the difference, and may act on it.
                Err(failure) => bail!(failure),
            }
        }),
    }
}
