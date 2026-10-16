//! wasmtime — where a program's bytes become code that runs.
//!
//! Every syscall arrives through one import, `berm.syscall`, carrying the
//! number its name hashes to alongside the request.
//!
//! A guest runs on a fiber: a hosted syscall suspends it, and the thread is
//! free until the reply resolves.

use crate::{
    Budget, Invocation, Meter,
    syscall::{self, Door, Table},
};
use anyhow::{Context, Result, anyhow, bail};
use std::{
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use wasmtime::{
    Caller, Extern, InstanceAllocationStrategy, InstancePre, Linker, Memory, Module,
    PoolingAllocationConfig, Store, ToWasmtimeResult,
};

/// The module every syscall is imported from, and the one name under it.
const MODULE: &str = "berm";
const SYSCALL: &str = "syscall";

/// How often the engine's epoch advances, and so the granularity a guest's
/// deadline is rounded up to. Fine enough that a runaway program is stopped
/// promptly, coarse enough that the tick costs nothing.
const TICK: Duration = Duration::from_millis(100);

/// How a runtime compiles and holds code.
#[derive(Clone, Debug)]
pub struct Config {
    /// Where compiled code is cached, if anywhere.
    ///
    /// Entries are keyed on contents, so a stale directory yields misses
    /// rather than wrong code, and it is safe to share between processes.
    pub cache_dir: Option<PathBuf>,
    /// How many guests may be live at once, across every program and nested
    /// calls included. Slots are reserved when the engine is built; a call
    /// past the last one fails.
    pub instances: u32,
    /// The largest linear memory one guest may grow to, in bytes.
    pub memory: usize,
    /// The most wasm instructions one invocation may execute.
    pub fuel: u64,
    /// The most rows one invocation may read and write together. A row
    /// syscall made once they are used is refused; the statement that crosses
    /// the budget runs to its end.
    pub rows: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_dir: None,
            instances: 1_000,
            memory: 64 << 20,
            fuel: 1_000_000_000,
            rows: 1_000_000,
        }
    }
}

/// A compilation target, shared by every program a runtime holds.
///
/// Cheap to clone; each compiled image keeps its own handle.
#[derive(Clone)]
pub struct Engine {
    wasmtime: wasmtime::Engine,
    /// [`Config::fuel`].
    fuel: u64,
    /// [`Config::rows`].
    rows: u64,
}

impl Engine {
    pub fn new(config: &Config) -> Result<Self> {
        let mut wasmtime = wasmtime::Config::new();
        // Without this the compiled code has no checks to notice a deadline,
        // and a guest that loops forever holds the calling thread forever.
        wasmtime.epoch_interruption(true);
        // Instructions are counted in fuel, one per wasm operator. A guest
        // bound by computation runs measurably slower for it.
        wasmtime.consume_fuel(true);
        if let Some(dir) = &config.cache_dir {
            let mut cache = wasmtime::CacheConfig::new();
            cache.with_directory(dir.join("wasm"));
            wasmtime.cache(Some(wasmtime::Cache::new(cache)?));
        }

        // An instance, its memory, its table and the fiber it runs on are
        // taken from slots reserved here rather than mapped per call. One of
        // each per guest: a guest asking for a second memory or table fails to
        // instantiate.
        let mut pool = PoolingAllocationConfig::new();
        pool.total_core_instances(config.instances)
            .total_memories(config.instances)
            .total_tables(config.instances)
            .total_stacks(config.instances)
            .max_memory_size(config.memory);
        wasmtime.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
        let engine = wasmtime::Engine::new(&wasmtime)?;
        tick(&engine);
        Ok(Self {
            wasmtime: engine,
            fuel: config.fuel,
            rows: config.rows,
        })
    }
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(&Config::default()).expect("the host target is supported")
    }
}

/// Advance `engine`'s epoch until it is dropped.
///
/// One thread for every guest the engine runs: a deadline is a countdown each
/// store holds, so nothing here has to know which of them are running.
fn tick(engine: &wasmtime::Engine) {
    let engine = engine.weak();
    thread::Builder::new()
        .name("berm-epoch".to_owned())
        .spawn(move || {
            loop {
                thread::sleep(TICK);
                let Some(engine) = engine.upgrade() else {
                    return;
                };
                engine.increment_epoch();
            }
        })
        .expect("spawn the berm epoch ticker");
}

/// One compiled program, ready to instantiate.
pub(crate) struct Image {
    engine: wasmtime::Engine,
    /// [`Config::fuel`] and [`Config::rows`].
    pub(crate) budget: Budget,
    /// Imports resolved once, at deploy. A guest reaching for something no
    /// syscall is registered for is refused by the deploy that introduced it,
    /// rather than trapping on a model's turn.
    pre: InstancePre<Invocation>,
    exports: Vec<String>,
}

impl Image {
    /// Compile `image` against `table`.
    pub(crate) fn compile(engine: &Engine, image: &[u8], table: Table) -> Result<Self> {
        let budget = Budget {
            instructions: engine.fuel,
            rows: engine.rows,
        };
        let engine = engine.wasmtime.clone();
        let module = Module::new(&engine, image)?;

        let mut linker: Linker<Invocation> = Linker::new(&engine);
        let table = Arc::new(table);
        linker.func_wrap_async(
            MODULE,
            SYSCALL,
            move |mut caller: Caller<'_, Invocation>, (number, ptr, len): (u64, u32, u32)| {
                // A syscall the guest was not given traps: the absence is the
                // enforcement.
                let door = table.get(&number).cloned();
                Box::new(async move {
                    let (ptr, len) = (ptr as u64, len as u64);
                    let answer = match door {
                        Some(Door::Builtin(call)) => call(&mut caller, ptr, len),
                        Some(Door::Hosted(call)) => match syscall::request(&mut caller, ptr, len) {
                            Ok((at, request)) => {
                                let reply = call(at, request).await;
                                Ok(syscall::stage(&mut caller, reply))
                            }
                            Err(error) => Err(error),
                        },
                        None => Err(anyhow!("no syscall for {number}")),
                    };
                    answer.to_wasmtime_result()
                })
            },
        )?;

        Ok(Self {
            exports: module.exports().map(|e| e.name().to_owned()).collect(),
            pre: linker.instantiate_pre(&module)?,
            engine,
            budget,
        })
    }

    /// Every name this image exports. Which of them are tools is the ABI's
    /// question.
    pub(crate) fn exports(&self) -> Vec<&str> {
        self.exports.iter().map(String::as_str).collect()
    }

    /// Instantiate and run one tool, executing at most `fuel` instructions, and
    /// meter the instructions it executed and the linear memory it reached
    /// against its program, trapped or not.
    ///
    /// The invocation goes in carrying the arguments and comes back carrying
    /// whatever the tool answered with. Nothing else survives: the instance is
    /// built for this call and dropped with it.
    pub(crate) async fn call(
        &self,
        symbol: &str,
        invocation: Invocation,
        fuel: u64,
    ) -> Result<Invocation> {
        let deadline = invocation.at.deadline;
        let mut store = Store::new(&self.engine, invocation);
        store.limiter(|invocation| &mut invocation.held);

        // Rounded up, and never zero: a guest gets at least what is left of the
        // bound it inherited, and a deadline of no ticks would trap it before
        // it ran an instruction.
        let remaining = deadline.saturating_duration_since(Instant::now());
        let ticks = remaining.div_duration_f64(TICK).ceil() as u64;
        store.set_epoch_deadline(ticks.max(1));
        store.set_fuel(fuel)?;

        // Instantiating runs the guest's own start-up, which is counted with
        // its call.
        let instance = self.pre.instantiate_async(&mut store).await;
        let called = match &instance {
            Ok(instance) => match instance.get_typed_func::<(), ()>(&mut store, symbol) {
                Ok(tool) => tool.call_async(&mut store, ()).await,
                Err(error) => Err(error),
            },
            Err(_) => Ok(()),
        };
        let left = store.get_fuel()?;
        let instructions = fuel - left;
        let memory = instance.as_ref().ok().map_or(0, |instance| {
            instance
                .get_memory(&mut store, "memory")
                .map_or(0, |memory| memory.data_size(&store) as u64)
        });
        store.data().at.meter(Meter {
            instructions,
            memory,
            ..Meter::default()
        });
        if (instance.is_err() || called.is_err()) && left == 0 {
            bail!("ran past its budget of {} instructions", fuel);
        }
        instance?;
        called?;
        Ok(store.into_data())
    }
}

/// Copy `len` bytes of guest memory at `addr`.
pub(crate) fn read(caller: &mut Caller<'_, Invocation>, addr: u64, len: u64) -> Result<Vec<u8>> {
    let memory = memory(caller)?;
    let mut bytes = vec![0u8; len as usize];
    memory.read(&*caller, addr as usize, &mut bytes)?;
    Ok(bytes)
}

/// Copy `bytes` into guest memory at `addr`.
pub(crate) fn write(caller: &mut Caller<'_, Invocation>, addr: u64, bytes: &[u8]) -> Result<()> {
    let memory = memory(caller)?;
    Ok(memory.write(&mut *caller, addr as usize, bytes)?)
}

fn memory(caller: &mut Caller<'_, Invocation>) -> Result<Memory> {
    caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .context("program exports no memory")
}
