//! One compiled program, and the state of one invocation of it.

use crate::{
    Callsite, Syscall, abi,
    bound::{depth, watchdog},
};
use anyhow::{Context, Result, bail};
use berm_api::Manifest;
use rvtime::{Caller, Engine, Linker, Module, Store, TypedFunc};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

/// A guest entry point: takes nothing, returns a pointer and a length.
type Export = TypedFunc<(), (u64, u64)>;

/// One compiled program: an image, its exports, and the syscalls it
/// was linked against. Compilation is paid once per ELF; every invocation gets
/// a fresh [`Store`] so no guest state crosses between calls.
pub struct Program {
    /// What this program was deployed as, as every syscall call
    /// reports it. An `Arc` because it is cloned into each invocation.
    pub name: Arc<str>,
    /// sha256 of the ELF. Redeploying different bytes under the same name is a
    /// different program, and this is what says so.
    pub digest: String,
    engine: Engine,
    module: Module,
    linker: Linker<Invocation>,
    /// Read from the ELF at load, without running anything.
    manifest: Manifest,
    /// Resolved once at load. A [`TypedFunc`] belongs to the module rather
    /// than to a store, so these stay valid for every invocation.
    tools: BTreeMap<String, Export>,
}

impl Program {
    /// Compile `elf` and resolve its exports, giving it `syscalls`. The engine's
    /// code cache makes a second load of the same bytes cheap across processes
    /// as well as within one.
    pub(crate) fn load(
        engine: &Engine,
        elf: &[u8],
        name: impl Into<Arc<str>>,
        syscalls: &[Syscall],
    ) -> Result<Self> {
        let module = Module::new(engine, elf).context("failed to compile program")?;
        let mut linker = Linker::new(engine);

        linker.func_wrap(abi::HOST_LOG, |caller: Caller<'_, Invocation>, ptr, len| {
            let bytes = caller.read(ptr, len)?;
            tracing::info!(target: "program", "{}", String::from_utf8_lossy(bytes));
            Ok(0u64)
        })?;

        linker.func_wrap(abi::HOST_ARG_LEN, |caller: Caller<'_, Invocation>| {
            Ok(caller.data().args.len() as u64)
        })?;

        // Returns the blob's full length rather than what fit, so a guest with
        // too small a buffer can tell it was truncated instead of acting on
        // half a request.
        linker.func_wrap(
            abi::HOST_ARG_READ,
            |mut caller: Caller<'_, Invocation>, ptr, capacity| {
                let length = caller.data().args.len();
                let args = caller.data().args[..length.min(capacity as usize)].to_vec();
                caller.write(ptr, &args)?;
                Ok(length as u64)
            },
        )?;

        // Saturating at the epoch: this is reached across a boundary an
        // unwind would abort the process at, so it cannot be allowed to panic.
        linker.func_wrap(abi::HOST_NOW, |_caller: Caller<'_, Invocation>| {
            Ok(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_millis() as u64))
        })?;

        // Asked for on the guest's first allocation, from inside the entry it
        // is already in. Pushing these in would mean entering the guest a
        // second time, which costs ~13µs against ~30ns for a host call.
        linker.func_wrap(abi::HOST_HEAP_START, |caller: Caller<'_, Invocation>| {
            Ok(caller.heap().start)
        })?;

        linker.func_wrap(abi::HOST_HEAP_SIZE, |caller: Caller<'_, Invocation>| {
            let heap = caller.heap();
            Ok(heap.end - heap.start)
        })?;

        linker.func_wrap(
            abi::HOST_FAIL,
            |mut caller: Caller<'_, Invocation>, ptr, len| {
                let message = String::from_utf8_lossy(caller.read(ptr, len)?).into_owned();
                caller.data_mut().failure = Some(message);
                Ok(0u64)
            },
        )?;

        // The other half of every syscall call. A program given no
        // syscalls never stages anything, so this is registered
        // unconditionally and has nothing to hand over.
        linker.func_wrap(
            abi::HOST_RESULT_READ,
            |mut caller: Caller<'_, Invocation>, ptr, capacity| {
                let length = caller.data().result.len();
                let result = caller.data().result[..length.min(capacity as usize)].to_vec();
                caller.write(ptr, &result)?;
                Ok(length as u64)
            },
        )?;

        for syscall in syscalls {
            let call = syscall.call.clone();
            linker.func_wrap(
                abi::hash(&syscall.name),
                move |caller: Caller<'_, Invocation>, ptr, len| {
                    let call = call.clone();
                    let (name, depth) = (caller.data().name.clone(), caller.data().depth);
                    Invocation::stage(caller, ptr, len, move |request| {
                        call(
                            &Callsite {
                                program: &name,
                                depth,
                            },
                            request,
                        )
                    })
                },
            )?;
        }

        let mut store = Store::new(engine, Invocation::empty());
        let instance = linker.instantiate(&mut store, &module)?;

        let names: Vec<String> = instance
            .exports()
            .filter_map(|export| export.strip_prefix(abi::TOOL_PREFIX))
            .map(str::to_owned)
            .collect();
        if names.is_empty() {
            bail!("program exports no tools");
        }

        let mut tools = BTreeMap::new();
        for name in names {
            let symbol = format!("{}{name}", abi::TOOL_PREFIX);
            tools.insert(name, instance.get_typed_func(&symbol)?);
        }

        // A program that advertises a tool it does not export would fail at
        // dispatch, on a model's turn, as a missing symbol. The symbol table
        // and the manifest are both in hand here, so disagreement is caught
        // before the program is ever offered.
        let manifest = Manifest::from_elf(elf)?;
        for tool in &manifest.tools {
            if !tools.contains_key(&tool.name) {
                bail!(
                    "program manifest declares tool {:?}, which it does not export",
                    tool.name
                );
            }
        }

        Ok(Self {
            name: name.into(),
            digest: hex::encode(Sha256::digest(elf)),
            engine: engine.clone(),
            module,
            linker,
            manifest,
            tools,
        })
    }

    /// The tools this program exports, as the symbol table reports them.
    pub fn tools(&self) -> impl Iterator<Item = &str> {
        self.tools.keys().map(String::as_str)
    }

    /// What the program says it is: ABI version, tools, and usage.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Run one tool by name.
    ///
    /// The outer `Result` is the host's — a missing tool, a trap, a broken
    /// image. The inner one is the guest's: `Err` means the program reported
    /// failure, which is what a tool result carries back to the model.
    pub fn call(&self, tool: &str, args: impl Into<Vec<u8>>) -> Result<Result<String, String>> {
        let Some(func) = self.tools.get(tool) else {
            bail!("program exports no tool named {tool:?}");
        };

        // Counted before the store is built, so this guest's own depth is what
        // its syscalls are handed.
        let _level = depth::Level::enter();
        let mut store = self.instantiate(args.into())?;

        // Entering the guest blocks this thread until the guest chooses to
        // return, so the bound on that has to be held by someone else. Dropped
        // on the way out of this function, before the store is.
        let _deadline = watchdog::Deadline::set(store.interrupt_handle()?);

        let (ptr, len) = func
            .call(&mut store, ())
            .with_context(|| format!("program trapped in {tool}"))?;

        if let Some(failure) = store.data_mut().failure.take() {
            return Ok(Err(failure));
        }

        let result = store.read(ptr, len)?;
        Ok(Ok(
            String::from_utf8(result.to_vec()).context("program returned invalid UTF-8")?
        ))
    }

    /// A store with the guest mapped into it and its heap handed over.
    fn instantiate(&self, args: Vec<u8>) -> Result<Store<Invocation>> {
        let mut store = Store::new(
            &self.engine,
            Invocation {
                args,
                result: Vec::new(),
                failure: None,
                name: self.name.clone(),
                depth: depth::current(),
            },
        );
        self.linker.instantiate(&mut store, &self.module)?;
        Ok(store)
    }
}

/// Guest state for one invocation. Memory is per-invocation; anything a
/// program needs to survive belongs in a storage program, not here.
pub struct Invocation {
    /// The program this invocation is of, and how deep it sits. Read once when
    /// the store is built, so a syscall call costs no lookup.
    name: Arc<str>,
    depth: u32,
    args: Vec<u8>,
    /// The last syscall call's result, waiting for the guest to pull it.
    /// Staged rather than pushed because its size is not known until the work
    /// is done, and doing the work twice to measure it is not an option.
    result: Vec<u8>,
    /// Set when the guest reports failure, which is how a tool that failed is
    /// told apart from one that returned the word "error".
    failure: Option<String>,
}

impl Invocation {
    fn empty() -> Self {
        Self {
            name: Arc::from(""),
            depth: 0,
            args: Vec::new(),
            result: Vec::new(),
            failure: None,
        }
    }

    /// Run one syscall and leave its bytes for the guest to pull.
    ///
    /// Failure rides on the same return value: the [`abi::ERROR`] bit says the
    /// staged bytes are a message. One that fails therefore costs the
    /// guest nothing extra to find out about, and an empty result cannot be
    /// mistaken for one.
    ///
    /// A syscall that answers with [`Refused`] additionally sets
    /// [`abi::REFUSED`], which is how a guest tells "it ran and said no" from
    /// "it never ran".
    pub fn stage(
        mut caller: Caller<'_, Self>,
        ptr: u64,
        len: u64,
        program: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
    ) -> Result<u64> {
        let request = caller.read(ptr, len)?.to_vec();
        let (staged, outcome) = match program(&request) {
            Ok(result) => (result, 0),
            Err(error) => {
                let refused = error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Refused>().is_some());
                let outcome = if refused {
                    abi::ERROR | abi::REFUSED
                } else {
                    abi::ERROR
                };
                (error.to_string().into_bytes(), outcome)
            }
        };
        let length = staged.len() as u64;
        caller.data_mut().result = staged;
        Ok(length | outcome)
    }
}

/// A syscall's answer when it refused the call and nothing ran.
///
/// Returned in an `Err` — on its own or as the source of a richer one — it
/// reaches the guest with [`abi::REFUSED`] set. Anything else is the other
/// kind of failure: whatever the syscall reached did run, and said no.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}
