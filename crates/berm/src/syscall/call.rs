//! `berm.call` — one program reaching another.
//!
//! berm serves it against the set of deployed programs it already holds. The
//! target is named in the request rather than wired at deploy, so an image
//! works against whatever it was deployed beside; which deployed program a name
//! reaches is the embedder's [`Calls::resolve`].

use crate::{
    Berm, Budget, Caller, Callsite, Changes, Ended, Link, Meter, Program, Refused, Syscall, abi,
    wire,
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

/// What an invocation of `program` runs inside, asked with the invocation's
/// site before it runs: the chain's first call and every nested one.
pub type Enter = Arc<dyn Fn(&Program, &Callsite) -> Opening + Send + Sync>;

/// The budget of one invocation of `tool` of the program deployed as
/// `program`. `None` takes the engine's [`crate::Config`].
pub type Budgeted = Arc<dyn Fn(&str, &str) -> Option<Budget> + Send + Sync>;

/// What `berm.now` answers: milliseconds since the Unix epoch. Called inside a
/// syscall, where an unwind aborts the process.
pub type Now = Arc<dyn Fn() -> u64 + Send + Sync>;

/// How a nested call to a deployed program's tool ended, as a host is told it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Called {
    Done,
    Failed,
    /// The tool refused its caller, through `berm.deny`.
    Denied,
    /// berm refused it before the tool ran.
    Refused,
}

/// Told of every nested call that reached a deployed program: the calling
/// site, the deployed name, the tool, and how it ended.
pub type Reported = Arc<dyn Fn(&Callsite, &str, &str, Called) + Send + Sync>;

/// Told of every invocation once it ran, trapped or not: its site, the tool,
/// and what it used itself, the programs it called left out.
pub type Invoked = Arc<dyn Fn(&Callsite, &str, Meter) + Send + Sync>;

/// Reaches a program deployed outside this runtime: asked with the calling
/// site, the deployed name, the tool, its arguments, and the link the target
/// runs as. `None` when nothing is deployed under that name elsewhere either.
/// The target's runtime commits what it changes; nothing lands in the
/// caller's journal.
pub type Remote =
    Arc<dyn Fn(&Callsite, &str, &str, Vec<u8>, Link) -> Option<Reaching> + Send + Sync>;

/// A call to a program outside this runtime, under way.
pub type Reaching = Pin<Box<dyn Future<Output = Result<Ended>> + Send>>;

/// A [`Scope`], being opened. An `Err` fails the invocation before it runs.
pub type Opening = Pin<Box<dyn Future<Output = Result<Box<dyn Scope>>> + Send>>;

/// What one invocation runs inside, as the host opened it. Dropped without
/// [`Scope::close`], it undoes what it holds.
pub trait Scope: Send {
    /// Whether the invocation holds its writes and logs on a journal of its
    /// own, handed to [`Scope::close`].
    fn own(&self) -> bool;

    /// The invocation returned ok. Handed what its own journal holds, empty
    /// without one, and answers what is left for the journal of the call that
    /// made it, or for the host when it is the chain's first.
    fn close(
        self: Box<Self>,
        changes: Changes,
    ) -> Pin<Box<dyn Future<Output = Result<Changes>> + Send>>;
}

/// The scope [`Calls::by_name`] opens: nothing held, on the caller's journal.
struct Unheld;

impl Scope for Unheld {
    fn own(&self) -> bool {
        false
    }

    fn close(
        self: Box<Self>,
        changes: Changes,
    ) -> Pin<Box<dyn Future<Output = Result<Changes>> + Send>> {
        Box::pin(std::future::ready(Ok(changes)))
    }
}

/// How `berm.call` reaches programs.
#[derive(Clone)]
pub struct Calls {
    /// See [`DEFAULT_CALL_DEPTH`].
    pub depth: u32,
    pub resolve: Resolve,
    pub named: Named,
    pub enter: Enter,
    pub budget: Budgeted,
    pub now: Now,
    pub called: Reported,
    pub invoked: Invoked,
    pub remote: Remote,
}

impl Calls {
    /// A target reaches the program deployed under that name, and is told the
    /// caller's deployed name.
    pub fn by_name(depth: u32) -> Self {
        Self {
            depth,
            resolve: Arc::new(|_, target, _| Ok(target.to_owned())),
            named: Arc::new(str::to_owned),
            enter: Arc::new(|_, _| Box::pin(async { Ok(Box::new(Unheld) as Box<dyn Scope>) })),
            budget: Arc::new(|_, _| None),
            now: Arc::new(system_now),
            called: Arc::new(|_, _, _, _| {}),
            invoked: Arc::new(|_, _, _| {}),
            remote: Arc::new(|_, _, _, _, _| None),
        }
    }
}

/// The system clock, saturating at the epoch.
pub fn system_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// Serve `berm.call` against `runtime`.
pub(crate) fn syscalls(runtime: Weak<Berm>, calls: Calls) -> Syscall {
    let Calls {
        depth: limit,
        resolve,
        named,
        enter: _,
        budget: _,
        now: _,
        called,
        invoked: _,
        remote,
    } = calls;
    Syscall {
        name: abi::CALL.to_owned(),
        call: Arc::new(move |at: Callsite, request: Vec<u8>| {
            let runtime = runtime.clone();
            let (resolve, named, called, remote) = (
                resolve.clone(),
                named.clone(),
                called.clone(),
                remote.clone(),
            );
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

                let caller = Some(Caller::Program(named(&at.program).into()));
                let Some(target) = target else {
                    let link = Link {
                        caller,
                        user: at.user.clone(),
                        client: at.client.clone(),
                        depth: at.depth + 1,
                        query: at.query,
                        dry: at.dry.clone(),
                        deadline: at.deadline,
                        stack: at
                            .stack
                            .iter()
                            .cloned()
                            .chain([Arc::from(deployed.as_str())])
                            .collect(),
                    };
                    let Some(reaching) =
                        remote(&at, &deployed, tool, args.as_bytes().to_vec(), link)
                    else {
                        let missing = match deployed == program {
                            true => format!("no program named {program:?} is deployed"),
                            false => {
                                format!("{program:?} reaches {deployed:?}, which is not deployed")
                            }
                        };
                        return Err(Refused(missing).into());
                    };
                    let invoked = reaching.await;
                    called(&at, &deployed, tool, ended(&invoked));
                    return answer(invoked);
                };
                let refuse = |message: String| -> Result<Vec<u8>> {
                    called(&at, &deployed, tool, Called::Refused);
                    Err(Refused(message).into())
                };
                if !target.public(tool) {
                    return refuse(format!("{program} has no public tool named {tool:?}"));
                }
                if !target.reached_by_programs(tool) {
                    return refuse(format!("{program}.{tool} takes no calls from programs"));
                }

                let stack: Arc<[Arc<str>]> = at
                    .stack
                    .iter()
                    .cloned()
                    .chain([target.name.clone()])
                    .collect();
                let invoked = target
                    .invoke(
                        tool,
                        args.as_bytes().to_vec(),
                        caller,
                        at.user.clone(),
                        at.client.clone(),
                        at.depth + 1,
                        at.query,
                        at.dry.clone(),
                        at.deadline,
                        at.chain,
                        at.journal.clone(),
                        at.meters.clone(),
                        stack,
                    )
                    .await;
                called(&at, &deployed, tool, ended(&invoked));
                answer(invoked)
            })
        }),
    }
}

/// What the calling guest is handed back.
fn answer(invoked: Result<Ended>) -> Result<Vec<u8>> {
    match invoked {
        Ok(Ended {
            result: Ok(result), ..
        }) => Ok(result.into_bytes()),
        // The target ran and said no. Not a `Refused`: the caller is told the
        // difference, and may act on it.
        Ok(Ended {
            result: Err(failure),
            ..
        }) => bail!(failure),
        Err(error) => Err(error),
    }
}

/// How an invocation ended, as [`Reported`] is told it.
fn ended(invoked: &Result<Ended>) -> Called {
    match invoked {
        Ok(Ended { result: Ok(_), .. }) => Called::Done,
        Ok(Ended { denied: true, .. }) => Called::Denied,
        _ => Called::Failed,
    }
}
