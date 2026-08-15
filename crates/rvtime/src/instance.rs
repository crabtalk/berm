//! Calling into an instantiated guest

use crate::{Store, abi::Regs, store};
use anyhow::{Result, anyhow, bail};
use std::{marker::PhantomData, sync::Arc};

/// A module instantiated into a store.
#[derive(Clone)]
pub struct Instance {
    module: Arc<compiler::Module>,
}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("functions", &self.module.program().functions.len())
            .finish()
    }
}

impl Instance {
    pub(crate) fn new(module: Arc<compiler::Module>) -> Self {
        Instance { module }
    }

    /// Look up an exported function and give it a Rust signature.
    ///
    /// Arguments and results are carried in `a0` onwards, so the type
    /// parameters pick how many registers are used rather than describing a
    /// signature the guest declared. Nothing verifies that the guest agrees;
    /// an ELF has no type information to check against.
    pub fn get_typed_func<P: Regs, R: Regs>(&self, name: &str) -> Result<TypedFunc<P, R>> {
        let entry = self
            .module
            .entry(name)
            .ok_or_else(|| anyhow!("no exported function named {name:?}"))?;

        if P::COUNT > 8 {
            bail!("a guest call takes at most 8 arguments, {} given", P::COUNT);
        }

        Ok(TypedFunc {
            module: self.module.clone(),
            entry,
            name: name.to_string(),
            marker: PhantomData,
        })
    }

    /// Run the program from its ELF entry point.
    ///
    /// This returns when the entry function returns, so a `_start` that never
    /// returns -- the usual shape for a freestanding image -- will not come
    /// back.
    pub fn run<T>(&self, store: &mut Store<T>) -> Result<()> {
        self.check(store)?;
        let entry = self
            .module
            .entry_at(self.module.program().entry)
            .ok_or_else(|| anyhow!("the entry point is not a compiled function"))?;
        store::enter::<T, (), ()>(store, entry, ())
    }

    /// Names of the functions this instance exports.
    pub fn exports(&self) -> impl Iterator<Item = &str> {
        self.module
            .program()
            .functions
            .values()
            .map(|f| f.name.as_str())
    }

    fn check<T>(&self, store: &Store<T>) -> Result<()> {
        let state = store
            .state
            .as_ref()
            .ok_or_else(|| anyhow!("store has no instance; call Linker::instantiate first"))?;
        if !Arc::ptr_eq(&state.module, &self.module) {
            bail!("this instance belongs to a different store");
        }
        Ok(())
    }
}

/// An exported guest function with a Rust signature.
pub struct TypedFunc<P, R> {
    module: Arc<compiler::Module>,
    entry: *const u8,
    name: String,
    marker: PhantomData<fn(P) -> R>,
}

impl<P, R> std::fmt::Debug for TypedFunc<P, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedFunc")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl<P: Regs, R: Regs> TypedFunc<P, R> {
    /// Call the function.
    pub fn call<T>(&self, store: &mut Store<T>, params: P) -> Result<R> {
        let state = store
            .state
            .as_ref()
            .ok_or_else(|| anyhow!("store has no instance; call Linker::instantiate first"))?;

        // The entry pointer is only meaningful for the module it came from.
        // Calling it against a store holding different code would jump into
        // the wrong compiled image.
        if !Arc::ptr_eq(&state.module, &self.module) {
            bail!(
                "{} belongs to a different instance than this store",
                self.name
            );
        }

        store::enter::<T, P, R>(store, self.entry, params)
    }

    /// The exported name this was resolved from.
    pub fn name(&self) -> &str {
        &self.name
    }
}
