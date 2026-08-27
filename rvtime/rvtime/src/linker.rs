//! Registering host functions

use crate::{Caller, Engine, Instance, Module, Store, abi::Regs};
use anyhow::{Context, Result, bail};
use std::{collections::HashMap, sync::Arc};

/// A host function, erased down to the register file it operates on.
pub type HostFn<T> = Box<dyn Fn(Caller<'_, T>) -> Result<()> + Send + Sync>;

pub(crate) type HostMap<T> = HashMap<u64, HostFn<T>>;

/// Defines the host functions a guest can call.
///
/// A guest reaches these with `ecall`, taking the call number from `a7` and
/// arguments from `a0` onwards -- the standard RISC-V syscall convention. The
/// key is therefore a number rather than a name: unlike WebAssembly, an ELF
/// carries no symbolic import table for the host to resolve.
pub struct Linker<T> {
    hosts: Arc<HostMap<T>>,
}

impl<T> std::fmt::Debug for Linker<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut numbers: Vec<_> = self.hosts.keys().copied().collect();
        numbers.sort_unstable();
        f.debug_struct("Linker").field("calls", &numbers).finish()
    }
}

impl<T> Linker<T> {
    /// A linker with no host functions.
    pub fn new(_engine: &Engine) -> Self {
        Linker {
            hosts: Arc::new(HashMap::new()),
        }
    }

    /// Register a host function that works on registers directly.
    ///
    /// Use this when the call does not have a fixed arity -- reading a
    /// variable number of arguments, say. [`Linker::func_wrap`] is nicer
    /// otherwise.
    pub fn func(
        &mut self,
        number: u64,
        func: impl Fn(Caller<'_, T>) -> Result<()> + Send + Sync + 'static,
    ) -> Result<&mut Self> {
        self.insert(number, Box::new(func))
    }

    /// Register a host function with a typed signature.
    ///
    /// ```ignore
    /// linker.func_wrap(1, |_: Caller<'_, ()>, a: u64, b: u64| Ok(a + b))?;
    /// ```
    pub fn func_wrap<P, R>(
        &mut self,
        number: u64,
        func: impl IntoHostFunc<T, P, R>,
    ) -> Result<&mut Self> {
        self.insert(number, func.into_host())
    }

    /// Instantiate `module` into `store`.
    ///
    /// This maps the guest's memory and wires the store to the compiled code.
    pub fn instantiate(&self, store: &mut Store<T>, module: &Module) -> Result<Instance> {
        store.instantiate(module.inner().clone(), self.hosts.clone())?;
        Ok(Instance::new(module.inner().clone()))
    }

    /// Register `func`, refusing a number that already has one.
    ///
    /// Refusing rather than replacing is what keeps a derived call number
    /// safe: an embedder that hashes names into this space cannot check for a
    /// collision itself without knowing every number already registered, and a
    /// silent replacement would send a guest's calls somewhere its author never
    /// named.
    fn insert(&mut self, number: u64, func: HostFn<T>) -> Result<&mut Self> {
        let hosts = Arc::get_mut(&mut self.hosts)
            .context("host functions cannot be registered after instantiate")?;
        if hosts.contains_key(&number) {
            bail!("host call {number} already has a function registered");
        }
        hosts.insert(number, func);
        Ok(self)
    }
}

/// Converts a closure into a [`HostFn`] by reading its arguments out of the
/// argument registers and writing its results back.
pub trait IntoHostFunc<T, P, R> {
    /// Erase the closure's signature.
    fn into_host(self) -> HostFn<T>;
}

macro_rules! host_func {
    ($($name:ident : $ty:ty = $index:expr),*) => {
        impl<T, F, R> IntoHostFunc<T, ($($ty,)*), R> for F
        where
            F: Fn(Caller<'_, T>, $($ty),*) -> Result<R> + Send + Sync + 'static,
            R: Regs,
        {
            fn into_host(self) -> HostFn<T> {
                Box::new(move |mut caller: Caller<'_, T>| {
                    $(let $name: $ty = caller.arg($index);)*
                    let results = self(caller.reborrow(), $($name),*)?;
                    caller.set_results(results);
                    Ok(())
                })
            }
        }
    };
}

host_func!();
host_func!(a: u64 = 0);
host_func!(a: u64 = 0, b: u64 = 1);
host_func!(a: u64 = 0, b: u64 = 1, c: u64 = 2);
host_func!(a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3);
host_func!(a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3, e: u64 = 4);
host_func!(a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3, e: u64 = 4, f: u64 = 5);
