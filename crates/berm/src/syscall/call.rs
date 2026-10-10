//! `berm.call` — one program reaching another.
//!
//! berm serves it against the set of deployed programs it already holds. The
//! target is named in the request rather than wired at deploy, so an image
//! works against whatever it was deployed beside; which deployed program a name
//! reaches is the embedder's [`Calls::resolve`].

use crate::{
    Berm, Caller, Callsite, Changes, Program, Refused, Syscall, abi, journal::Journal, wire,
};
use anyhow::{Result, bail};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
};

/// How deep a chain of programs calling programs may go before the next call
/// is refused. Zero turns composition off.
///
/// Not a bound on the native stack, which a nesting level costs ~720 bytes of
/// and would allow thousands: it bounds how far a mechanical composition can
/// run away from the turn that asked for it, and how much guest address space
/// one chain reserves — 64 MiB a level.
pub const DEFAULT_CALL_DEPTH: u32 = 4;

/// The deployed name a call's target reaches, asked with the calling site and
/// the tool called. An `Err` refuses the call.
pub type Resolve = Arc<dyn Fn(&Callsite, &str, &str) -> Result<String> + Send + Sync>;

/// What a called program is told the program calling it is named, given the
/// name that one was deployed as.
pub type Named = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// Whether a nested call into `program`'s `tool` commits on its own, handed
/// whether the call is a query. `Some` opens what it commits through; `None`
/// leaves its writes and logs to the chain.
pub type Own = Arc<dyn Fn(&Arc<Program>, &str, bool) -> Option<Opening> + Send + Sync>;

/// A nested call's own commit, being opened.
pub type Opening = Pin<Box<dyn Future<Output = Result<Box<dyn Commit>>> + Send>>;

/// What a nested call that commits on its own commits through. Dropped
/// without [`Commit::commit`], it commits nothing.
pub trait Commit: Send {
    /// Commit what the call and every call it made wrote and emitted.
    fn commit(
        self: Box<Self>,
        changes: Changes,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

/// How `berm.call` reaches programs.
#[derive(Clone)]
pub struct Calls {
    /// See [`DEFAULT_CALL_DEPTH`].
    pub depth: u32,
    pub resolve: Resolve,
    pub named: Named,
    pub own: Own,
}

impl Calls {
    /// A target reaches the program deployed under that name, and is told the
    /// caller's deployed name.
    pub fn by_name(depth: u32) -> Self {
        Self {
            depth,
            resolve: Arc::new(|_, target, _| Ok(target.to_owned())),
            named: Arc::new(str::to_owned),
            own: Arc::new(|_, _, _| None),
        }
    }
}

/// Serve `berm.call` against `runtime`.
pub(crate) fn syscalls(runtime: Weak<Berm>, calls: Calls) -> Syscall {
    let Calls {
        depth: limit,
        resolve,
        named,
        own,
    } = calls;
    Syscall {
        name: abi::CALL.to_owned(),
        call: Arc::new(move |at: Callsite, request: Vec<u8>| {
            let runtime = runtime.clone();
            let (resolve, named, own) = (resolve.clone(), named.clone(), own.clone());
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
                let deployed = resolve(&at, program, tool)
                    .map_err(|error| Refused(format!("{program}.{tool}: {error:#}")))?;
                let target = {
                    let Ok(programs) = runtime.programs.try_read() else {
                        return Err(Refused(format!(
                            "the deployed set is being written; {program}.{tool} was not reached"
                        ))
                        .into());
                    };
                    programs.get(&deployed).cloned()
                };

                let Some(target) = target else {
                    let missing = match deployed == program {
                        true => format!("no program named {program:?} is deployed"),
                        false => format!("{program:?} reaches {deployed:?}, which is not deployed"),
                    };
                    return Err(Refused(missing).into());
                };
                if !target.public(tool) {
                    return Err(
                        Refused(format!("{program} has no public tool named {tool:?}")).into(),
                    );
                }

                let caller = Some(Caller::Program(named(&at.program).into()));
                let stack: Arc<[Arc<str>]> = at
                    .stack
                    .iter()
                    .cloned()
                    .chain([target.name.clone()])
                    .collect();
                let query = at.query
                    || target
                        .manifest()
                        .tools
                        .iter()
                        .any(|spec| spec.name == tool && spec.query);

                // A target that commits on its own runs on a journal of its
                // own, committed when it returns ok, whatever the caller does
                // after. One already running in this chain is not entered
                // again.
                if let Some(opening) = own(&target, tool, query) {
                    if at.stack.contains(&target.name) {
                        return Err(Refused(format!(
                            "{program} is already running in this chain, so {program}.{tool} was not entered again"
                        ))
                        .into());
                    }
                    let session = opening.await?;
                    let journal = Arc::new(Journal::default());
                    let invoked = target
                        .invoke(
                            tool,
                            args.as_bytes().to_vec(),
                            caller,
                            at.depth + 1,
                            at.query,
                            at.deadline,
                            journal.clone(),
                            stack,
                        )
                        .await;
                    return match invoked {
                        Ok(Ok(result)) => {
                            session.commit(journal.take()).await?;
                            Ok(result.into_bytes())
                        }
                        Ok(Err(failure)) => bail!(failure),
                        Err(error) => Err(error),
                    };
                }

                // A failed target takes what it held with it; the caller
                // goes on from where it was.
                let mark = at.journal.mark();
                let invoked = target
                    .invoke(
                        tool,
                        args.as_bytes().to_vec(),
                        caller,
                        at.depth + 1,
                        at.query,
                        at.deadline,
                        at.journal.clone(),
                        stack,
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
