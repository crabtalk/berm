//! wasmtime — a program as WebAssembly.
//!
//! What a program is built for. Every syscall arrives through one import,
//! `berm.syscall`, carrying the number its name hashes to alongside the
//! request — the same number rvtime takes in `a7`, so one name resolves the
//! same way on both.

use crate::{
    Invocation,
    backend::{Config, Guest},
    bound::watchdog,
    syscall::Table,
};
use anyhow::{Context, Result, anyhow, bail};
use std::{
    ops::Range,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use wasmtime::{
    Caller, Engine, Extern, InstancePre, Linker, Memory, Module, Store, ToWasmtimeResult, TypedFunc,
};

/// The module every syscall is imported from, and the one name under it.
const MODULE: &str = "berm";
const SYSCALL: &str = "syscall";

/// How often the engine's epoch advances, and so the granularity a guest's
/// deadline is rounded up to. Fine enough that a runaway program is stopped
/// promptly, coarse enough that the tick costs nothing.
const TICK: Duration = Duration::from_millis(100);

pub(crate) fn engine(config: &Config) -> Result<Engine> {
    let mut wasmtime = wasmtime::Config::new();
    // Without this the compiled code has no checks to notice a deadline, and a
    // guest that loops forever holds the calling thread forever.
    wasmtime.epoch_interruption(true);
    if let Some(dir) = &config.cache_dir {
        let mut cache = wasmtime::CacheConfig::new();
        cache.with_directory(dir.join("wasm"));
        wasmtime.cache(Some(wasmtime::Cache::new(cache)?));
    }

    let engine = Engine::new(&wasmtime)?;
    tick(&engine);
    Ok(engine)
}

/// Advance `engine`'s epoch until it is dropped.
///
/// One thread for every guest the engine runs: a deadline is a countdown each
/// store holds, so nothing here has to know which of them are running.
fn tick(engine: &Engine) {
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

pub(crate) struct Image {
    engine: Engine,
    /// Imports resolved once, at deploy. A guest reaching for something no
    /// syscall is registered for is refused by the deploy that introduced it,
    /// rather than trapping on a model's turn.
    pre: InstancePre<Invocation>,
    exports: Vec<String>,
}

impl Image {
    pub(crate) fn compile(engine: &Engine, image: &[u8], table: Table) -> Result<Self> {
        let engine = engine.clone();
        let module = Module::new(&engine, image)?;

        let mut linker: Linker<Invocation> = Linker::new(&engine);
        let table = Arc::new(table);
        linker.func_wrap(
            MODULE,
            SYSCALL,
            move |mut caller: Caller<'_, Invocation>, number: u64, ptr: u32, len: u32| {
                // A syscall the guest was not given traps: the absence is the
                // enforcement.
                let answer = match table.get(&number) {
                    Some(call) => call(&mut caller, ptr as u64, len as u64),
                    None => Err(anyhow!("no syscall for {number}")),
                };
                answer.to_wasmtime_result()
            },
        )?;

        Ok(Self {
            exports: module.exports().map(|e| e.name().to_owned()).collect(),
            pre: linker.instantiate_pre(&module)?,
            engine,
        })
    }

    pub(crate) fn exports(&self) -> Vec<&str> {
        self.exports.iter().map(String::as_str).collect()
    }

    pub(crate) fn call(&self, symbol: &str, invocation: Invocation) -> Result<Invocation> {
        let deadline = watchdog::Deadline::enter();
        let mut store = Store::new(&self.engine, invocation);

        // Rounded up, and never zero: a guest gets at least what is left of the
        // bound it inherited, and a deadline of no ticks would trap it before
        // it ran an instruction.
        let remaining = deadline.at().saturating_duration_since(Instant::now());
        let ticks = remaining.div_duration_f64(TICK).ceil() as u64;
        store.set_epoch_deadline(ticks.max(1));

        let instance = self.pre.instantiate(&mut store)?;
        let tool: TypedFunc<(), ()> = instance.get_typed_func(&mut store, symbol)?;
        tool.call(&mut store, ())?;
        Ok(store.into_data())
    }
}

impl Guest for Caller<'_, Invocation> {
    fn read(&mut self, addr: u64, len: u64) -> Result<Vec<u8>> {
        let memory = memory(self)?;
        let mut bytes = vec![0u8; len as usize];
        memory.read(&*self, addr as usize, &mut bytes)?;
        Ok(bytes)
    }

    fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<()> {
        let memory = memory(self)?;
        Ok(memory.write(&mut *self, addr as usize, bytes)?)
    }

    /// A wasm guest grows and manages its own memory, so nothing asks.
    fn heap(&mut self) -> Result<Range<u64>> {
        bail!("a WebAssembly program owns its memory; the host has no heap to hand it")
    }

    fn invocation(&mut self) -> &mut Invocation {
        self.data_mut()
    }
}

fn memory(caller: &mut Caller<'_, Invocation>) -> Result<Memory> {
    caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .context("program exports no memory")
}
