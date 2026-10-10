//! `berm.call` — one program reaching another.
//!
//! The one syscall berm serves itself. It needs nothing a host would
//! have to decide: the set of deployed programs is what berm already is, and
//! the target is named in the request rather than wired at deploy, so an image
//! works against whatever it was deployed beside.

use crate::{Berm, Callsite, Refused, Syscall, abi, wire};
use anyhow::bail;
use std::sync::{Arc, Weak};

/// How deep a chain of programs calling programs may go before the next call
/// is refused. Zero turns composition off.
///
/// Not a bound on the native stack, which a nesting level costs ~720 bytes of
/// and would allow thousands: it bounds how far a mechanical composition can
/// run away from the turn that asked for it, and how much guest address space
/// one chain reserves — 64 MiB a level.
pub const DEFAULT_CALL_DEPTH: u32 = 4;

/// Serve `berm.call` against `runtime`.
pub(crate) fn syscalls(runtime: Weak<Berm>, limit: u32) -> Syscall {
    Syscall {
        name: abi::CALL.to_owned(),
        call: Arc::new(move |at: Callsite, request: Vec<u8>| {
            let runtime = runtime.clone();
            Box::pin(async move {
                let fields = wire::fields(&request)?;
                let program = wire::text(&fields, 0, "program")?;
                let tool = wire::text(&fields, 1, "tool")?;
                let args = wire::text(&fields, 2, "arguments")?;

                if at.depth > limit {
                    return Err(Refused(format!(
                        "call depth {limit} reached before {program}.{tool}; a program cannot nest deeper"
                    ))
                    .into());
                }

                let Some(runtime) = runtime.upgrade() else {
                    return Err(Refused(format!(
                        "the runtime is shutting down, so {program}.{tool} cannot run"
                    ))
                    .into());
                };

                // `try_read` rather than a blocking one: a deploy holding the
                // write lock is mid-flight, which is worth reporting rather
                // than waiting on from inside a guest. Cloned out and the guard
                // dropped before the guest below runs: holding it across a
                // nested call would block the next deploy for as long as that
                // call takes.
                let target = {
                    let Ok(programs) = runtime.programs.try_read() else {
                        return Err(Refused(format!(
                            "the deployed set is being written; {program}.{tool} was not reached"
                        ))
                        .into());
                    };
                    programs.get(program).cloned()
                };

                let Some(target) = target else {
                    return Err(Refused(format!("no program named {program:?} is deployed")).into());
                };

                // A failed target takes what it held with it; the caller
                // goes on from where it was.
                let mark = at.journal.mark();
                let invoked = target
                    .invoke(
                        tool,
                        args.as_bytes().to_vec(),
                        at.caller.clone(),
                        at.depth + 1,
                        at.deadline,
                        at.journal.clone(),
                    )
                    .await;
                match invoked {
                    Ok(Ok(result)) => Ok(result.into_bytes()),
                    // The target ran and said no. Not a `Refused`: the caller
                    // is told the difference, and may act on it.
                    Ok(Err(failure)) => {
                        at.journal.revert(mark);
                        bail!(failure)
                    }
                    Err(error) => {
                        at.journal.revert(mark);
                        Err(error)
                    }
                }
            })
        }),
    }
}
