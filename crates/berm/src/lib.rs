//! berm — the runtime of harnesses.
//!
//! A harness is one hash-pinned RV64 ELF. berm compiles it once, holds it by
//! the name it answers to, and instantiates it per invocation under rvtime:
//! arguments are pulled in through host calls, the result is read back out of
//! guest memory, and nothing survives the call.
//!
//! ```ignore
//! let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
//! berm.deploy("example", &elf)?;
//! let result = berm.call("example", "echo", br#"{"query":"hi"}"#.to_vec())?;
//! ```
//!
//! What is deployed is reachable, by name, from any other harness deployed
//! beside it — which is the one system harness berm serves itself, because it
//! is the only one that needs nothing but the set berm already holds.
//!
//! Everything else a harness reaches is a [`System`] the embedder passed in,
//! and that list is the linker it is instantiated with: a call to anything else
//! traps because nothing is registered for it, not because a check said no.
//! berm ships none. What a filesystem is bounded by, what shape a command's
//! result takes, where bytes persist — each is a decision about a host, and
//! berm has no host.

use anyhow::Result;
pub use berm_api::{Manifest, ToolSpec};
pub use harness::{Harness, Invocation, Refused};
pub use rvtime::{Config, Engine};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, Weak},
};

pub mod abi;
pub mod call;
mod depth;
mod harness;
mod watchdog;
pub mod wire;

// Reached by the host expansion, and re-exported for the reason `berm-lang`
// re-exports serde: an embedder depends on this crate and nothing else, and
// cannot pick a version the generated code disagrees with.
pub use anyhow;
pub use berm_codegen::hosts;

/// What a system harness does: request bytes in, result bytes out. An `Err`
/// reaches the guest as a failure message on the same wire as a result.
pub type Call = Arc<dyn Fn(&Callsite<'_>, &[u8]) -> Result<Vec<u8>> + Send + Sync>;

/// Which harness is asking, and how deep the chain that reached it already is.
///
/// The runtime knows both at the moment of the call, so a system harness is
/// handed them rather than made to infer them: identity would otherwise be a
/// closure built per deployed harness, and depth a thread-local read from
/// inside the guest's own stack.
pub struct Callsite<'a> {
    /// What this harness was deployed as.
    pub harness: &'a str,
    /// How many guests are already on this thread's stack, this one included.
    pub depth: u32,
}

/// The harnesses a host is running.
///
/// Held behind an `Arc`: the system harnesses berm serves reach back into it,
/// so it hands them a [`Weak`] of itself rather than a table copied per deploy.
pub struct Berm {
    engine: Engine,
    /// Every deployed harness, by the name it answers to. A `std` lock because
    /// it is read from inside a guest's host call, where an async one cannot go.
    harnesses: RwLock<BTreeMap<String, Arc<Harness>>>,
    /// See [`call::DEFAULT_CALL_DEPTH`].
    depth: u32,
    /// What the embedder gives every harness, beside what berm serves itself.
    system: Vec<System>,
    /// This runtime, as the system harnesses berm serves hold it. `Weak`
    /// because they are reachable *from* it — a deployed harness owns the
    /// linker that owns them — and an `Arc` would be a cycle that never drops.
    me: Weak<Self>,
}

impl Berm {
    /// A runtime with nothing deployed, giving every harness `system`.
    pub fn new(engine: &Engine, depth: u32, system: Vec<System>) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            engine: engine.clone(),
            harnesses: RwLock::new(BTreeMap::new()),
            depth,
            system,
            me: me.clone(),
        })
    }

    /// Compile `elf` and make its tools reachable as `name`.
    ///
    /// Compiling here rather than on first call means a broken image is refused
    /// by the deploy that introduced it, not on a model's turn.
    pub fn deploy(&self, name: &str, elf: &[u8]) -> Result<Arc<Harness>> {
        let mut system = self.system.clone();
        system.push(call::system(self.me.clone(), self.depth));

        let harness = Arc::new(Harness::load(&self.engine, elf, name, &system)?);
        self.harnesses
            .write()
            .expect("deployed harnesses")
            .insert(name.to_owned(), harness.clone());
        Ok(harness)
    }

    /// Forget one. `false` if nothing answered to that name.
    pub fn remove(&self, name: &str) -> bool {
        self.harnesses
            .write()
            .expect("deployed harnesses")
            .remove(name)
            .is_some()
    }

    pub fn get(&self, name: &str) -> Option<Arc<Harness>> {
        self.harnesses
            .read()
            .expect("deployed harnesses")
            .get(name)
            .cloned()
    }

    /// Every deployed harness, as a snapshot the caller can hold.
    pub fn list(&self) -> Vec<Arc<Harness>> {
        self.harnesses
            .read()
            .expect("deployed harnesses")
            .values()
            .cloned()
            .collect()
    }

    /// Run one tool on one harness.
    ///
    /// The outer `Result` is the host's — no such harness, no such tool, a
    /// trap. The inner one is the harness's own reported failure, which is a
    /// tool result rather than an error.
    pub fn call(
        &self,
        harness: &str,
        tool: &str,
        args: impl Into<Vec<u8>>,
    ) -> Result<Result<String, String>> {
        let Some(harness) = self.get(harness) else {
            anyhow::bail!("no harness named {harness:?} is deployed");
        };
        harness.call(tool, args)
    }
}

/// One thing a harness may reach.
///
/// A system harness absent from the list a harness was deployed with is absent
/// from its linker, and that absence is the enforcement — there is no check to
/// write and none to forget. Whatever bounds it is whatever it closes over.
///
/// The name is hashed to the number the guest puts in `a7`, and berm neither
/// reserves a namespace nor recognises one: every name in the list belongs to
/// the embedder that wrote it.
#[derive(Clone)]
pub struct System {
    /// What the guest calls it, e.g. `crabtalk.fs.read`.
    pub name: String,
    /// What it does.
    pub call: Call,
}
