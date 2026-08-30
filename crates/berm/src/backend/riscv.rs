//! rvtime — a program as a statically linked RV64 ELF.
//!
//! Experimental. Any language with a RISC-V target is a program here, without
//! a wasm story of its own.
//!
//! A syscall arrives as `ecall`, taking the number from `a7` and the request
//! pointer and length from `a0` and `a1`. Each is registered under its own
//! number, which is the only key an ELF's guests can name.

use crate::{
    Invocation,
    backend::{Config, Guest},
    bound::watchdog,
    syscall::Table,
};
use anyhow::{Result, bail};
use rvtime::{Caller, Engine, Interrupt, Linker, Module, Store, TypedFunc};
use std::{
    collections::BTreeMap,
    ops::Range,
    sync::{
        Arc, Condvar, LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::Instant,
};

/// A guest entry point: takes nothing, returns nothing. What it answered with
/// travelled through [`crate::abi::HOST_DONE`] before it returned.
type Export = TypedFunc<(), ()>;

pub(crate) fn engine(config: &Config) -> Result<Engine> {
    let mut rvtime = rvtime::Config::new();
    // Under its own name: a directory holding both backends' entries is one
    // whose entries have to be told apart.
    if let Some(dir) = &config.cache_dir {
        rvtime.cache_dir(dir.join("riscv"));
    }
    Engine::new(&rvtime)
}

pub(crate) struct Image {
    engine: Engine,
    module: Module,
    linker: Linker<Invocation>,
    /// Resolved once at load. A [`TypedFunc`] belongs to the module rather than
    /// to a store, so these stay valid for every invocation.
    exports: BTreeMap<String, Export>,
}

impl Image {
    pub(crate) fn compile(engine: &Engine, image: &[u8], table: Table) -> Result<Self> {
        let engine = engine.clone();
        let module = Module::new(&engine, image)?;
        let mut linker = Linker::new(&engine);

        for (number, call) in table {
            linker.func_wrap(
                number,
                move |mut caller: Caller<'_, Invocation>, ptr: u64, len: u64| {
                    call(&mut caller, ptr, len)
                },
            )?;
        }

        // rvtime resolves an export against an instance, so naming them costs
        // one that runs nothing.
        let mut store = Store::new(&engine, probe());
        let instance = linker.instantiate(&mut store, &module)?;
        let names: Vec<String> = instance.exports().map(str::to_owned).collect();

        // rvtime names every compiled function, not just the ones that can be
        // entered — an entry is a function whose address the image takes, which
        // every export the ABI cares about is. The rest are internal, and
        // asking for one is how you find that out.
        let mut exports = BTreeMap::new();
        for name in names {
            if let Ok(func) = instance.get_typed_func(&name) {
                exports.insert(name, func);
            }
        }

        Ok(Self {
            engine,
            module,
            linker,
            exports,
        })
    }

    pub(crate) fn exports(&self) -> Vec<&str> {
        self.exports.keys().map(String::as_str).collect()
    }

    pub(crate) fn call(&self, symbol: &str, invocation: Invocation) -> Result<Invocation> {
        let Some(func) = self.exports.get(symbol) else {
            bail!("program exports no symbol named {symbol:?}");
        };

        let deadline = watchdog::Deadline::enter();
        let mut store = Store::new(&self.engine, invocation);
        self.linker.instantiate(&mut store, &self.module)?;

        // Entering the guest blocks this thread until the guest chooses to
        // return, so the bound on that has to be held by someone else. Declared
        // after the store, so it is withdrawn before the store is dropped.
        let _stop = Stop::at(deadline.at(), store.interrupt_handle()?);

        func.call(&mut store, ())?;
        Ok(store.into_data())
    }
}

/// The state the export-naming instance carries. It runs nothing, so none of
/// it is ever read.
fn probe() -> Invocation {
    Invocation {
        name: Arc::from(""),
        depth: 0,
        args: Vec::new(),
        staged: Vec::new(),
        outcome: None,
    }
}

impl Guest for Caller<'_, Invocation> {
    fn read(&mut self, addr: u64, len: u64) -> Result<Vec<u8>> {
        Ok(Caller::read(self, addr, len)?.to_vec())
    }

    fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<()> {
        Caller::write(self, addr, bytes)
    }

    fn heap(&mut self) -> Result<Range<u64>> {
        Ok(Caller::heap(self))
    }

    fn invocation(&mut self) -> &mut Invocation {
        self.data_mut()
    }
}

/// One guest's stop request: the ticket that withdraws it, when it is due, and
/// the handle that trips it.
struct Entry {
    ticket: u64,
    at: Instant,
    interrupt: Interrupt,
}

/// What the watchdog is waiting on, keyed by a monotonic ticket so a finished
/// invocation removes its own rather than someone else's.
///
/// rvtime stops a guest by tripping a flag it polls, which needs someone to
/// trip it — one thread serves every invocation, because a thread per call
/// would cost more to spawn than a whole invocation costs to run.
static PENDING: LazyLock<(Mutex<Vec<Entry>>, Condvar)> =
    LazyLock::new(|| (Mutex::new(Vec::new()), Condvar::new()));

/// Hands out tickets. Wrapping after 2^64 invocations is not a scenario.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Trips `interrupt` if this guard is still alive at `at`.
///
/// Withdrawal is the point: the overwhelming majority of invocations finish
/// long before their deadline, and a guard that forgot to deregister would
/// leave the watchdog interrupting a store that had already been dropped.
struct Stop(u64);

impl Stop {
    fn at(at: Instant, interrupt: Interrupt) -> Self {
        let ticket = NEXT.fetch_add(1, Ordering::Relaxed);
        let (pending, wake) = &*PENDING;
        pending.lock().expect("watchdog deadlines").push(Entry {
            ticket,
            at,
            interrupt,
        });
        // The watchdog may be parked with nothing to wait on, or waiting on a
        // deadline later than this.
        wake.notify_one();
        start();
        Self(ticket)
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        let (pending, _) = &*PENDING;
        pending
            .lock()
            .expect("watchdog deadlines")
            .retain(|entry| entry.ticket != self.0);
    }
}

/// Start the watchdog once, on the first invocation that needs it. An embedder
/// that never runs a guest never gets the thread.
fn start() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        thread::Builder::new()
            .name("berm-watchdog".to_owned())
            .spawn(watch)
            .expect("spawn the berm watchdog");
    });
}

fn watch() {
    let (pending, wake) = &*PENDING;
    let mut deadlines = pending.lock().expect("watchdog deadlines");
    loop {
        let now = Instant::now();
        // Tripping an interrupt does not remove the entry: the guest stops at
        // its next backward edge rather than immediately, and its own guard is
        // what withdraws it.
        let mut earliest: Option<Instant> = None;
        for entry in deadlines.iter() {
            if entry.at <= now {
                entry.interrupt.interrupt();
            } else {
                earliest = Some(earliest.map_or(entry.at, |e: Instant| e.min(entry.at)));
            }
        }

        deadlines = match earliest {
            Some(at) => {
                wake.wait_timeout(deadlines, at.saturating_duration_since(now))
                    .expect("watchdog deadlines")
                    .0
            }
            None => wake.wait(deadlines).expect("watchdog deadlines"),
        };
    }
}
