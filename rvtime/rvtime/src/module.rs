//! Compiled programs

use crate::Engine;
use anyhow::{Context, Result};
use std::{path::Path, sync::Arc};

/// A compiled RISC-V program.
///
/// Cloning shares the compiled code rather than recompiling.
#[derive(Clone)]
pub struct Module {
    inner: Arc<compiler::Module>,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("functions", &self.inner.program().functions.len())
            .field("entry_point", &format_args!("{:#x}", self.entry_point()))
            .field("memory_size", &format_args!("{:#x}", self.memory_size()))
            .finish()
    }
}

impl Module {
    /// Compile a statically linked RV64 ELF image.
    pub fn new(engine: &Engine, bytes: &[u8]) -> Result<Module> {
        #[cfg(feature = "aot")]
        let inner = crate::aot::compile(engine, bytes)?;

        #[cfg(not(feature = "aot"))]
        let inner = {
            let program = rv::elf::load(bytes).context("failed to load the guest image")?;
            compiler::Module::new(
                engine.compiler(),
                program,
                engine.config().memory_size,
                engine.config().interruptible,
            )?
        };

        Ok(Module {
            inner: Arc::new(inner),
        })
    }

    /// Compile the ELF image at `path`.
    pub fn from_file(engine: &Engine, path: impl AsRef<Path>) -> Result<Module> {
        let path = path.as_ref();
        let bytes =
            std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        Module::new(engine, &bytes).with_context(|| format!("failed to compile {}", path.display()))
    }

    /// Names of the functions this module exports.
    pub fn exports(&self) -> impl Iterator<Item = &str> {
        self.inner
            .program()
            .functions
            .values()
            .map(|f| f.name.as_str())
    }

    /// The ELF entry point.
    pub fn entry_point(&self) -> u64 {
        self.inner.program().entry
    }

    /// Whether this module checks for interruption while running.
    pub fn interruptible(&self) -> bool {
        self.inner.interruptible()
    }

    /// The guest address space size this module was compiled for.
    pub fn memory_size(&self) -> u64 {
        self.inner.memory_size()
    }

    pub(crate) fn inner(&self) -> &Arc<compiler::Module> {
        &self.inner
    }
}
