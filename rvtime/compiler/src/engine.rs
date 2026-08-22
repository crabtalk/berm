//! Target configuration

use anyhow::{Result, anyhow};
use cranelift::{
    codegen::{isa::OwnedTargetIsa, settings},
    native,
    prelude::Configurable,
};

/// How hard Cranelift should work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptLevel {
    /// Compile as quickly as possible. The right default when a program is
    /// compiled once and run once.
    #[default]
    None,

    /// Generate the fastest code, at the cost of compile time.
    Speed,
}

/// A configured target.
#[derive(Clone)]
pub struct Engine {
    isa: OwnedTargetIsa,
    cache: Option<std::sync::Arc<crate::Cache>>,
}

impl Engine {
    /// Build an engine for the host machine.
    pub fn new(opt: OptLevel) -> Result<Self> {
        let mut flags = settings::builder();
        flags.set(
            "opt_level",
            match opt {
                OptLevel::None => "none",
                OptLevel::Speed => "speed",
            },
        )?;

        // Guest code cannot unwind, and nothing walks its frames.
        flags.set("unwind_info", "false")?;

        let isa = native::builder()
            .map_err(|e| anyhow!("unsupported host: {e}"))?
            .finish(settings::Flags::new(flags))?;

        Ok(Engine { isa, cache: None })
    }

    /// Cache generated code under `dir`, reusing it across runs.
    pub fn with_cache(mut self, dir: impl Into<std::path::PathBuf>) -> Result<Self> {
        self.cache = Some(std::sync::Arc::new(crate::Cache::open(dir)?));
        Ok(self)
    }

    /// The compiled-code cache, if one was configured.
    pub fn cache(&self) -> Option<&std::sync::Arc<crate::Cache>> {
        self.cache.as_ref()
    }

    /// The target ISA.
    pub fn isa(&self) -> &OwnedTargetIsa {
        &self.isa
    }
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(OptLevel::default()).expect("the host target is supported")
    }
}
