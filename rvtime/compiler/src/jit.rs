//! Compiling straight into runnable pages
//!
//! The code never leaves this process, so there is nothing to relocate and
//! nothing to fingerprint: Cranelift hands back pages that are already linked.

use crate::{
    Engine, Module,
    module::{self, Code},
};
use anyhow::Result;
use cranelift::{
    jit::{JITBuilder, JITModule},
    module::default_libcall_names,
};
use rv::Program;

impl Module {
    /// Compile every function in `program` for a `memory_size`-byte guest
    /// address space.
    pub fn new(
        engine: &Engine,
        program: Program,
        memory_size: u64,
        interruptible: bool,
    ) -> Result<Self> {
        let builder = JITBuilder::with_isa(engine.isa().clone(), default_libcall_names());
        let mut jit = JITModule::new(builder);

        let ids = module::compile(
            &mut jit,
            engine,
            &program,
            memory_size,
            interruptible,
            engine.cache(),
        )?;
        jit.finalize_definitions()?;

        let entries = ids
            .functions
            .iter()
            .map(|(addr, id)| (*addr, jit.get_finalized_function(*id)))
            .collect();
        let trampoline = jit.get_finalized_function(ids.trampoline);

        Ok(Module::assemble(
            Code::Jit(Box::new(jit)),
            program,
            entries,
            trampoline,
            memory_size,
            interruptible,
        ))
    }
}
