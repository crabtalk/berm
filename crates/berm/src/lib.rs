//! berm — the runtime of programs.
//!
//! A program is one hash-pinned image. berm compiles it once, holds it by the
//! name it answers to, and instantiates it per invocation: arguments are
//! pulled in through syscalls, the result is handed back through one, and
//! nothing survives the call.
//!
//! ```ignore
//! let calls = Calls::by_name(syscall::call::DEFAULT_CALL_DEPTH);
//! let berm = Berm::new(&engine, calls, vec![], storage::Memory::new());
//! berm.deploy("example", &wasm)?;
//! let outcome = berm.call(None, "example", "echo", br#"{"query":"hi"}"#.to_vec()).await?;
//! ```
//!
//! An image is a WebAssembly module, run under wasmtime.
//!
//! What is deployed is reachable from any other program deployed beside it,
//! through `berm.call`, which berm serves against the set it already holds.
//! Which deployed program a call's target names is the embedder's
//! [`Calls::resolve`].
//!
//! Everything else a program reaches is a [`Syscall`] the embedder passed in,
//! and that list is the linker it is instantiated with: a call to anything else
//! traps because nothing is registered for it, not because a check said no.
//! The standard ones — `berm.get`/`berm.set`, `berm.emit`, `berm.call.after`,
//! `berm.http.fetch` — are built with [`syscall`]'s constructors, which read
//! and check the request and hand the work to the embedder's closures.
//!
//! What `berm.set` and `berm.emit` leave to berm is held in a journal and
//! handed back from [`Program::call`] as [`Changes`], for the host to commit as
//! one. Every invocation runs inside a [`Scope`] the host opens through
//! [`Calls::enter`].

use anyhow::Result;
pub use berm_api::{Budget, Deps, Dry, Logged, MAX_LOG, Manifest, Meter, ToolSpec};
pub use engine::{Config, Engine};
use journal::Journal;
pub use journal::{Changes, Log, Write};
use meter::Own;
pub use meter::{Meters, Running};
pub use program::{Ended, Invocation, Link, Outcome, Program, Refused, chain, digest};
use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{Arc, RwLock, Weak},
    time::Instant,
};
pub use storage::{Records, Storage};
pub use syscall::call::{
    Budgeted, Called, Calls, Enter, Invoked, Named, Now, Opening, Reaching, Remote, Reported,
    Resolve, Scope,
};

pub mod abi;
mod bound;
#[cfg(feature = "sqlite")]
pub mod database;
mod engine;
mod journal;
mod meter;
mod program;
pub mod storage;
pub mod syscall;
pub mod wire;

// Reached by the host expansion, and re-exported for the reason `berm-lang`
// re-exports serde: an embedder depends on this crate and nothing else, and
// cannot pick a version the generated code disagrees with.
pub use anyhow;
pub use berm_codegen::hosts;

/// What a syscall answers with, once it has. An `Err` reaches the guest as a
/// failure message on the same wire as a result.
pub type Reply = Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>>;

/// What a syscall does: request bytes in, result bytes out.
///
/// The guest is suspended until the reply resolves, and its thread is free
/// meanwhile.
pub type Call = Arc<dyn Fn(Callsite, Vec<u8>) -> Reply + Send + Sync>;

/// Which program is asking, on whose behalf, and the chain that reached it.
#[derive(Clone, Debug)]
pub struct Callsite {
    /// What this program was deployed as.
    pub program: Arc<str>,
    /// What this program's manifest declares it reaches for.
    pub deps: Arc<Deps>,
    /// Who made this call: whoever the embedder named at the chain's first
    /// call, or the program that called this one. `None` when the host
    /// started the chain on the program's own behalf.
    pub caller: Option<Caller>,
    /// The end user the embedder vouched for at the chain's first call.
    /// Carried unchanged into every nested call.
    pub user: Option<User>,
    /// The outside client, as the per-day pseudonymous key the embedder
    /// supplied at the chain's first call. Carried unchanged into every nested
    /// call.
    pub client: Option<Arc<str>>,
    /// How many guests the chain holds, this one included.
    pub depth: u32,
    /// The programs this call sits under, outermost first, this one last.
    pub stack: Arc<[Arc<str>]>,
    /// Whether this invocation is of a `query` tool, or sits under one. A
    /// query is refused `berm.set`, `berm.emit` and `berm.call.after`.
    pub query: bool,
    /// Set when the chain is dry: carried into every nested call, and nothing
    /// the chain changes is kept.
    pub dry: Option<Arc<Dry>>,
    /// Which chain this is, unique within the process.
    pub chain: u64,
    /// When the chain has to be done by. A guest suspended in a syscall cannot
    /// notice it until the reply resolves, so a syscall bounds its own wait
    /// inside this.
    pub deadline: Instant,
    /// What the chain has written and emitted so far.
    pub(crate) journal: Arc<Journal>,
    /// What the chain has used so far, by program.
    pub(crate) meters: Arc<Meters>,
    /// What this invocation used itself, its rows against its budget.
    pub(crate) own: Arc<Own>,
}

impl Callsite {
    /// Count `used` against this program in the chain, its rows against this
    /// invocation's budget.
    pub fn meter(&self, used: Meter) {
        self.own.add(used);
        self.meters.add(&self.program, used);
    }

    /// Refused once this invocation has used its budget of rows.
    pub fn rows_left(&self) -> Result<()> {
        self.own.left()
    }

    /// Count `used` against `program`, run elsewhere for this chain.
    pub fn meter_of(&self, program: &str, used: Meter) {
        self.meters.add(program, used);
    }
}

/// Who made a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// Named by the embedder at the chain's first call.
    Outside(Arc<str>),
    /// The program that called through `berm.call`, as [`Calls::named`] names
    /// it.
    Program(Arc<str>),
}

/// An end user, as the embedder verified them: who vouched for them, and who
/// they are to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub issuer: Arc<str>,
    pub subject: Arc<str>,
}

/// The programs a host is running.
///
/// Held behind an `Arc`: the syscalls berm serves reach back into it,
/// so it hands them a [`Weak`] of itself rather than a table copied per deploy.
pub struct Berm {
    engine: Engine,
    /// Every deployed program, by the name it answers to. Never held across an
    /// await.
    programs: RwLock<BTreeMap<String, Arc<Program>>>,
    /// How `berm.call` reaches programs.
    calls: Calls,
    /// What the embedder gives every program, beside what berm serves itself.
    syscalls: Vec<Syscall>,
    /// Where this runtime's own records live. Held as the trait: berm decides
    /// what is worth writing down, a host decides where it goes.
    storage: Arc<dyn Storage>,
    /// This runtime, as the syscalls berm serves hold it. `Weak`
    /// because they are reachable *from* it — a deployed program owns the
    /// linker that owns them — and an `Arc` would be a cycle that never drops.
    me: Weak<Self>,
}

impl Berm {
    /// A runtime with nothing deployed, giving every program `syscalls` and
    /// writing what it must remember to `storage`.
    ///
    /// [`storage::Memory`] is the one to pass when nothing should outlive the
    /// process.
    pub fn new(
        engine: &Engine,
        calls: Calls,
        syscalls: Vec<Syscall>,
        storage: Arc<dyn Storage>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            engine: engine.clone(),
            programs: RwLock::new(BTreeMap::new()),
            calls,
            syscalls,
            storage,
            me: me.clone(),
        })
    }

    /// Where this runtime's records live, for whatever else a host keeps
    /// beside the images — the wakes it drives itself.
    pub fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }

    /// Compile `image` and make its tools reachable as `name`.
    ///
    /// Compiling here rather than on first call means a broken image is refused
    /// by the deploy that introduced it, not on a model's turn. Written down
    /// before it is published, so a tool that is served is one a restart brings
    /// back.
    pub fn deploy(&self, name: &str, image: &[u8]) -> Result<Arc<Program>> {
        let program = self.load(name, image)?;
        self.storage.put(Records::Programs, name, image)?;
        self.programs
            .write()
            .expect("deployed programs")
            .insert(name.to_owned(), program.clone());
        Ok(program)
    }

    /// Bring back every image deployed before this process.
    ///
    /// One that will not load is reported and skipped: a single bad record is
    /// not a reason to come up with none of them.
    pub fn restore(&self) -> Result<()> {
        for (name, image) in self.storage.list(Records::Programs)? {
            match self.load(&name, &image) {
                Ok(program) => {
                    tracing::info!(name, digest = %program.digest, "restored");
                    self.programs
                        .write()
                        .expect("deployed programs")
                        .insert(name, program);
                }
                Err(error) => tracing::error!(name, "{error:#}"),
            }
        }
        Ok(())
    }

    /// Whether a program deployed here can make the syscall `name`.
    pub fn serves(&self, name: &str) -> bool {
        name == abi::CALL
            || syscall::is_builtin(abi::hash(name))
            || self.syscalls.iter().any(|syscall| syscall.name == name)
    }

    /// Compile an image against the syscalls this runtime serves.
    fn load(&self, name: &str, image: &[u8]) -> Result<Arc<Program>> {
        let mut syscalls = self.syscalls.clone();
        syscalls.push(syscall::call::syscalls(self.me.clone(), self.calls.clone()));
        Ok(Arc::new(Program::load(
            &self.engine,
            image,
            name,
            &syscalls,
            &self.calls,
        )?))
    }

    /// Forget one and drop its image. `false` if nothing answered to that name.
    pub fn remove(&self, name: &str) -> Result<bool> {
        if self
            .programs
            .write()
            .expect("deployed programs")
            .remove(name)
            .is_none()
        {
            return Ok(false);
        }
        self.storage.remove(Records::Programs, name)?;
        Ok(true)
    }

    pub fn get(&self, name: &str) -> Option<Arc<Program>> {
        self.programs
            .read()
            .expect("deployed programs")
            .get(name)
            .cloned()
    }

    /// Every deployed program, as a snapshot the caller can hold.
    pub fn list(&self) -> Vec<Arc<Program>> {
        self.programs
            .read()
            .expect("deployed programs")
            .values()
            .cloned()
            .collect()
    }

    /// Run one tool on one program.
    ///
    /// `Err` is the host's — no such program, no such tool, a trap. The
    /// program's own reported failure is [`Outcome::result`].
    pub async fn call(
        &self,
        caller: Option<Arc<str>>,
        program: &str,
        tool: &str,
        args: impl Into<Vec<u8>>,
    ) -> Result<Outcome> {
        let Some(program) = self.get(program) else {
            anyhow::bail!("no program named {program:?} is deployed");
        };
        program.call(caller, tool, args).await
    }
}

/// One thing a program may reach.
///
/// A syscall absent from the list a program was deployed with is absent
/// from its linker, and that absence is the enforcement — there is no check to
/// write and none to forget. Whatever bounds it is whatever it closes over.
///
/// The name is hashed to the number the guest passes to `berm.syscall`, and
/// berm neither reserves a namespace nor recognises one: every name in the list
/// belongs to the embedder that wrote it.
#[derive(Clone)]
pub struct Syscall {
    /// What the guest calls it, e.g. `berm.http.fetch`.
    pub name: String,
    /// What it does.
    pub call: Call,
}
