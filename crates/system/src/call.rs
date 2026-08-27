//! `berm.call` — one harness reaching another.
//!
//! berm names it and serves it for nobody: a `Berm` is one harness with nothing
//! to dispatch to, so a host running more than one is what registers it. The
//! target is named in the request rather than wired at load, so an image works
//! against whatever it was deployed beside.

use anyhow::{Result, bail};
use berm::{Callsite, Harness, Refused, abi, wire};
use std::sync::Arc;

/// How deep a chain of harnesses calling harnesses may go before the next call
/// is refused. Zero turns composition off.
///
/// Not a bound on the native stack, which a nesting level costs ~720 bytes of
/// and would allow thousands: it bounds how far a mechanical composition can
/// run away from the turn that asked for it, and how much guest address space
/// one chain reserves — 64 MiB a level.
pub const DEFAULT_CALL_DEPTH: u32 = 4;

/// Serve `berm.call`, resolving every name through `dispatch`.
///
/// `dispatch` is handed the harness, the tool and the argument blob — what the
/// guest passed `berm_lang::call` — and answers on berm's two levels: an outer
/// `Err` for a call that never ran, carrying [`Refused`] when the guest should
/// be able to tell that apart, and an inner one for a target that ran and said
/// no. It may not panic, being reached from compiled guest code across an
/// `extern "C"` boundary where an unwind aborts the process, and it may not
/// still hold a lock when it enters the target, whose own calls arrive back
/// here.
pub fn harness(
    limit: u32,
    dispatch: impl Fn(&str, &str, &str) -> Result<Result<String, String>> + Send + Sync + 'static,
) -> Harness {
    Harness {
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

            match dispatch(harness, tool, args)? {
                Ok(result) => Ok(result.into_bytes()),
                // The target ran and said no. Not a `Refused`: the caller is
                // told the difference, and may act on it.
                Err(failure) => bail!(failure),
            }
        }),
    }
}
