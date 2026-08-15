//! A RISC-V compiler with a wasmtime-like interface.
//!
//! Load a statically linked RV64IMAC ELF, compile it to native code with
//! Cranelift, call its exported functions from Rust, and let it call back.
//!
//! ```no_run
//! use rvtime::{Caller, Config, Engine, Linker, Module, Store};
//!
//! # fn main() -> anyhow::Result<()> {
//! let engine = Engine::new(&Config::default())?;
//! let module = Module::from_file(&engine, "guest.elf")?;
//!
//! let mut store = Store::new(&engine, 0u64);
//! let mut linker = Linker::new(&engine);
//!
//! // The guest reaches this with `ecall` and `a7 == 1`.
//! linker.func_wrap(1, |mut caller: Caller<'_, u64>, a: u64, b: u64| {
//!     *caller.data_mut() += 1;
//!     Ok(a + b)
//! })?;
//!
//! let instance = linker.instantiate(&mut store, &module)?;
//! let add = instance.get_typed_func::<(u64, u64), u64>("op_add")?;
//! assert_eq!(add.call(&mut store, (10, 3))?, 13);
//! # Ok(())
//! # }
//! ```
//!
//! ## How this differs from wasmtime
//!
//! - **Host functions are keyed by number, not name.** A guest calls them with
//!   `ecall`, taking the number from `a7`. An ELF has no symbolic import table
//!   to resolve names against.
//! - **A store holds one instance.** A program image is one address space and
//!   one register file; there is nothing to gain from sharing a store.
//! - **Signatures are not checked.** Arguments are plain 64-bit words in
//!   `a0`..`a7`, so the type parameters on
//!   [`get_typed_func`](Instance::get_typed_func) choose how many registers to
//!   use rather than describing something the guest declared.
//!
//! ## Guest memory
//!
//! Each store reserves a guest address space of
//! [`Config::memory_size`](Config::memory_size) bytes — 64 MiB by default. The
//! reservation is lazy, so an unused tail costs address space rather than
//! resident memory, but it is still per-store address space, which matters when
//! many guests run at once.
//!
//! The size must be a power of two. Guest addresses are confined by masking
//! with `memory_size - 1`, which is what keeps a program inside its own memory;
//! any other size would leave part of the mask's range pointing past the
//! reservation. The mask is compiled into the code, so a [`Module`] is tied to
//! the size its [`Engine`] was configured with, and a [`Store`] always maps at
//! the module's size.
//!
//! One consequence worth knowing: confinement is not a precise bounds check. An
//! address past the end of the space wraps and may land on a mapped page rather
//! than faulting. The guest still cannot reach anything outside its own memory.
//!
//! The space is laid out as:
//!
//! ```text
//! [ image ][ heap ][ guard ]        ...        [ stack ]
//! 0                                                size
//! ```
//!
//! The heap is committed read-write and zeroed, and is what a guest allocator
//! manages. rvtime does not carve it up or hand it to the guest: read the
//! bounds with [`Store::heap`] and pass them in however your host interface
//! prefers — a host function you register, or arguments to an init export.
//! Committing it costs address space rather than memory, since pages are
//! faulted in only when touched.
//!
//! ## Guest requirements
//!
//! The image must be a statically linked RV64IMAC ELF **linked with
//! `--emit-relocs`**. The relocations are what identify which functions have
//! their address taken, and therefore where an indirect call may legally land.
//! Without them a computed jump has no way to be checked.

pub use crate::{
    abi::Regs,
    config::{Config, Engine, OptLevel, Strategy},
    instance::{Instance, TypedFunc},
    linker::{HostFn, IntoHostFunc, Linker},
    module::Module,
    store::{Caller, Interrupt, Store, Trap},
};

/// Guest registers, re-exported for host functions that work on them directly.
pub use rv::Reg;

mod abi;
mod config;
mod instance;
mod linker;
mod module;
mod store;
