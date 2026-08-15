//! Guest state and the boundary between host and guest

use crate::{Config, Engine, abi::Regs, linker::HostMap};
use anyhow::{Context, Result, anyhow};
use compiler::{Memory, trap};
use rv::Reg;
use std::{fmt, sync::Arc};
use translator::VmCtx;

/// Why a guest stopped short.
#[derive(Debug)]
pub enum Trap {
    /// A load or store landed outside the committed guest memory.
    MemoryFault {
        /// The guest address, when the fault was inside the address space.
        address: Option<u64>,
    },

    /// An indirect jump targeted something that is not a function entry.
    BadIndirectTarget,

    /// The guest executed `ebreak`.
    Breakpoint,

    /// The guest called a number no host function is registered for.
    UnknownHostCall(u64),

    /// A host function returned an error.
    HostCall(anyhow::Error),
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trap::MemoryFault {
                address: Some(addr),
            } => write!(f, "guest memory fault at {addr:#x}"),
            Trap::MemoryFault { address: None } => {
                write!(f, "memory fault outside the guest address space")
            }
            Trap::BadIndirectTarget => write!(f, "indirect jump to an unknown target"),
            Trap::Breakpoint => write!(f, "guest executed ebreak"),
            Trap::UnknownHostCall(number) => write!(f, "no host function for call {number}"),
            Trap::HostCall(error) => write!(f, "host call failed: {error}"),
        }
    }
}

impl std::error::Error for Trap {}

/// Everything a running guest owns.
pub(crate) struct State<T> {
    pub module: Arc<compiler::Module>,
    pub memory: Memory,
    pub ctx: VmCtx,
    pub hosts: Arc<HostMap<T>>,

    /// Set by a failing host call and taken by the entry point, so the error
    /// survives the return through compiled code.
    pub failure: Option<Trap>,
}

/// Owns the guest's memory, registers, and the embedder's own data.
///
/// A store holds a single instance. That is narrower than wasmtime, where one
/// store can back many instances, and it is what a program image actually
/// needs: one address space, one register file.
pub struct Store<T> {
    data: T,
    engine: Engine,
    config: Config,
    pub(crate) state: Option<State<T>>,
}

impl<T> Store<T> {
    /// Create a store holding `data`.
    pub fn new(engine: &Engine, data: T) -> Self {
        Store {
            data,
            config: *engine.config(),
            engine: engine.clone(),
            state: None,
        }
    }

    /// The embedder's data.
    pub fn data(&self) -> &T {
        &self.data
    }

    /// The embedder's data, mutably.
    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// Consume the store, returning the embedder's data.
    pub fn into_data(self) -> T {
        self.data
    }

    /// The engine this store was created with.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Read `len` bytes of guest memory at `addr`.
    pub fn read(&self, addr: u64, len: u64) -> Result<&[u8]> {
        self.instantiated()?.memory.read(addr, len)
    }

    /// Write `data` into guest memory at `addr`.
    pub fn write(&mut self, addr: u64, data: &[u8]) -> Result<()> {
        let state = self
            .state
            .as_mut()
            .ok_or_else(|| anyhow!("store has no instance"))?;
        state.memory.write(addr, data)
    }

    fn instantiated(&self) -> Result<&State<T>> {
        self.state
            .as_ref()
            .ok_or_else(|| anyhow!("store has no instance; call Linker::instantiate first"))
    }

    /// Wire up an instance. Called by [`crate::Linker::instantiate`].
    pub(crate) fn instantiate(
        &mut self,
        module: Arc<compiler::Module>,
        hosts: Arc<HostMap<T>>,
    ) -> Result<()> {
        // The size comes from the module, not this store's config: the address
        // mask is baked into the compiled code, so mapping at any other size
        // would let a guest address escape the reservation.
        let memory = Memory::new(module.program(), module.memory_size(), self.stack_size())
            .context("failed to map guest memory")?;

        let ctx = VmCtx {
            regs: [0; 32],
            memory: memory.base(),
            dispatch: module.dispatch().as_ptr(),
            dispatch_len: module.dispatch().len() as u64,
            text_base: module.program().text.start,
            host_call: dispatch::<T> as *const u8,
            // Refreshed on every entry, because the store may have moved.
            host_data: std::ptr::null_mut(),
            trap: 0,
        };

        self.state = Some(State {
            module,
            memory,
            ctx,
            hosts,
            failure: None,
        });
        Ok(())
    }

    /// The configured stack size, rounded up to a whole number of host pages.
    fn stack_size(&self) -> u64 {
        let page = compiler::memory::host_page();
        self.config.stack_size.div_ceil(page).max(1) * page
    }
}

/// Call a compiled guest function.
///
/// `params` land in the argument registers and the results are read back out
/// of them once the guest returns.
pub(crate) fn enter<T, P: Regs, R: Regs>(
    store: &mut Store<T>,
    entry: *const u8,
    params: P,
) -> Result<R> {
    // Taken before the state borrow: compiled code reaches the store through
    // this, and the store may have moved since it was instantiated.
    let host_data: *mut Store<T> = store;

    let state = store
        .state
        .as_mut()
        .ok_or_else(|| anyhow!("store has no instance; call Linker::instantiate first"))?;

    state.failure = None;
    state.ctx.trap = 0;
    state.ctx.host_data = host_data.cast();
    state.ctx.regs = [0; 32];
    state.ctx.regs[Reg::SP.index()] = state.memory.stack_pointer();
    params.write(&mut state.ctx.regs);

    trap::set_guest_region(state.memory.base() as usize, state.memory.size());

    let trampoline: extern "C" fn(*mut VmCtx, *const u8) =
        unsafe { std::mem::transmute(state.module.trampoline()) };
    let ctx: *mut VmCtx = &raw mut state.ctx;

    let outcome = trap::protect(|| trampoline(ctx, entry));

    let state = store.state.as_mut().expect("instance is still present");
    if let Err(fault) = outcome {
        return Err(Trap::MemoryFault {
            address: fault.guest,
        }
        .into());
    }

    // A host call records the reason it failed; the compiled code only knows
    // that it should stop.
    if let Some(failure) = state.failure.take() {
        return Err(failure.into());
    }

    match translator::Trap::from_code(state.ctx.trap) {
        translator::Trap::None => Ok(R::read(&state.ctx.regs)),
        translator::Trap::BadIndirectTarget => Err(Trap::BadIndirectTarget.into()),
        translator::Trap::Breakpoint => Err(Trap::Breakpoint.into()),
        translator::Trap::HostCall => Err(Trap::HostCall(anyhow!("host call failed")).into()),
    }
}

/// Handle an `ecall`.
///
/// Compiled code reaches this through [`VmCtx::host_call`]. Returning non-zero
/// tells the guest to stop; the reason is left in the store.
extern "C" fn dispatch<T>(ctx: *mut VmCtx) -> u64 {
    // Read through the raw pointer rather than taking `&mut VmCtx`: the
    // context lives inside the store, and holding both references at once
    // would alias.
    let (host_data, number) = unsafe {
        (
            (*ctx).host_data,
            (*ctx).regs[Reg::A7.index()],
        )
    };

    let store = unsafe { &mut *(host_data as *mut Store<T>) };
    let Some(state) = store.state.as_ref() else {
        return 1;
    };

    let hosts = state.hosts.clone();
    let Some(func) = hosts.get(&number) else {
        fail(store, Trap::UnknownHostCall(number));
        return 1;
    };

    match func(Caller { store }) {
        Ok(()) => 0,
        Err(error) => {
            fail(store, Trap::HostCall(error));
            1
        }
    }
}

fn fail<T>(store: &mut Store<T>, trap: Trap) {
    if let Some(state) = store.state.as_mut() {
        state.failure = Some(trap);
    }
}

/// A host function's view of the guest that called it.
pub struct Caller<'a, T> {
    pub(crate) store: &'a mut Store<T>,
}

impl<T> Caller<'_, T> {
    /// The embedder's data.
    pub fn data(&self) -> &T {
        self.store.data()
    }

    /// The embedder's data, mutably.
    pub fn data_mut(&mut self) -> &mut T {
        self.store.data_mut()
    }

    /// Read a guest register.
    pub fn reg(&self, reg: Reg) -> u64 {
        self.state().ctx.regs[reg.index()]
    }

    /// Write a guest register.
    pub fn set_reg(&mut self, reg: Reg, value: u64) {
        self.state_mut().ctx.regs[reg.index()] = value;
    }

    /// Read `len` bytes of guest memory at `addr`.
    pub fn read(&self, addr: u64, len: u64) -> Result<&[u8]> {
        self.state().memory.read(addr, len)
    }

    /// Write `data` into guest memory at `addr`.
    pub fn write(&mut self, addr: u64, data: &[u8]) -> Result<()> {
        self.state_mut().memory.write(addr, data)
    }

    /// Borrow the caller again, for passing on to a wrapped closure.
    pub(crate) fn reborrow(&mut self) -> Caller<'_, T> {
        Caller { store: self.store }
    }

    /// The `n`th argument register.
    pub(crate) fn arg(&self, index: usize) -> u64 {
        self.reg(Reg::new((Reg::A0.index() + index) as u8))
    }

    /// Place a host function's results in the argument registers.
    pub(crate) fn set_results<R: Regs>(&mut self, results: R) {
        results.write(&mut self.state_mut().ctx.regs);
    }

    fn state(&self) -> &State<T> {
        self.store.state.as_ref().expect("caller implies an instance")
    }

    fn state_mut(&mut self) -> &mut State<T> {
        self.store.state.as_mut().expect("caller implies an instance")
    }
}
