//! berm — the runtime of programs.
//!
//! A program is one hash-pinned image. berm compiles it once, holds it by the
//! name it answers to, and instantiates it per invocation: arguments are
//! pulled in through syscalls, the result is handed back through one, and
//! nothing survives the call.
//!
//! ```ignore
//! let berm = Berm::new(&engine, syscall::call::DEFAULT_CALL_DEPTH, vec![]);
//! berm.deploy("example", &wasm)?;
//! let result = berm.call("example", "echo", br#"{"query":"hi"}"#.to_vec())?;
//! ```
//!
//! An image is WebAssembly, run under wasmtime, or — experimentally — a
//! statically linked RV64 ELF run under rvtime. Which one a deploy reaches is
//! read off its first four bytes, and the two answer the same ABI: same
//! syscall names, same framing, same exports.
//!
//! What is deployed is reachable, by name, from any other program deployed
//! beside it — which is the one syscall berm serves itself, because it
//! is the only one that needs nothing but the set berm already holds.
//!
//! Everything else a program reaches is a [`Syscall`] the embedder passed in,
//! and that list is the linker it is instantiated with: a call to anything else
//! traps because nothing is registered for it, not because a check said no.
//! berm ships none. What a filesystem is bounded by, what shape a command's
//! result takes, where bytes persist — each is a decision about a host, and
//! berm has no host.

use anyhow::Result;
pub use backend::{Config, Engine};
pub use berm_api::{Manifest, ToolSpec};
pub use program::{Invocation, Program, Refused};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, Weak},
};
pub use storage::{Records, Storage};

pub mod abi;
mod backend;
mod bound;
mod program;
pub mod storage;
pub mod syscall;
pub mod wire;

// Reached by the host expansion, and re-exported for the reason `berm-lang`
// re-exports serde: an embedder depends on this crate and nothing else, and
// cannot pick a version the generated code disagrees with.
pub use anyhow;
pub use berm_codegen::hosts;

/// What a syscall does: request bytes in, result bytes out. An `Err`
/// reaches the guest as a failure message on the same wire as a result.
pub type Call = Arc<dyn Fn(&Callsite<'_>, &[u8]) -> Result<Vec<u8>> + Send + Sync>;

/// Which program is asking, and how deep the chain that reached it already is.
///
/// The runtime knows both at the moment of the call, so a syscall is
/// handed them rather than made to infer them: identity would otherwise be a
/// closure built per deployed program, and depth a thread-local read from
/// inside the guest's own stack.
pub struct Callsite<'a> {
    /// What this program was deployed as.
    pub program: &'a str,
    /// How many guests are already on this thread's stack, this one included.
    pub depth: u32,
}

/// The programs a host is running.
///
/// Held behind an `Arc`: the syscalls berm serves reach back into it,
/// so it hands them a [`Weak`] of itself rather than a table copied per deploy.
pub struct Berm {
    engine: Engine,
    /// Every deployed program, by the name it answers to. A `std` lock because
    /// it is read from inside a guest's host call, where an async one cannot go.
    programs: RwLock<BTreeMap<String, Arc<Program>>>,
    /// See [`syscall::call::DEFAULT_CALL_DEPTH`].
    depth: u32,
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
        depth: u32,
        syscalls: Vec<Syscall>,
        storage: Arc<dyn Storage>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            engine: engine.clone(),
            programs: RwLock::new(BTreeMap::new()),
            depth,
            syscalls,
            storage,
            me: me.clone(),
        })
    }

    /// Where this runtime's records live, for whatever else a host keeps
    /// beside the images — the connections and wakes it drives itself.
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

    /// Compile an image against the syscalls this runtime serves.
    fn load(&self, name: &str, image: &[u8]) -> Result<Arc<Program>> {
        let mut syscalls = self.syscalls.clone();
        syscalls.push(syscall::call::syscalls(self.me.clone(), self.depth));
        Ok(Arc::new(Program::load(
            &self.engine,
            image,
            name,
            &syscalls,
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
    /// The outer `Result` is the host's — no such program, no such tool, a
    /// trap. The inner one is the program's own reported failure, which is a
    /// tool result rather than an error.
    pub fn call(
        &self,
        program: &str,
        tool: &str,
        args: impl Into<Vec<u8>>,
    ) -> Result<Result<String, String>> {
        let Some(program) = self.get(program) else {
            anyhow::bail!("no program named {program:?} is deployed");
        };
        program.call(tool, args)
    }
}

/// One thing a program may reach.
///
/// A syscall absent from the list a program was deployed with is absent
/// from its linker, and that absence is the enforcement — there is no check to
/// write and none to forget. Whatever bounds it is whatever it closes over.
///
/// The name is hashed to the number the guest puts in `a7`, and berm neither
/// reserves a namespace nor recognises one: every name in the list belongs to
/// the embedder that wrote it.
#[derive(Clone)]
pub struct Syscall {
    /// What the guest calls it, e.g. `crabtalk.fs.read`.
    pub name: String,
    /// What it does.
    pub call: Call,
}
