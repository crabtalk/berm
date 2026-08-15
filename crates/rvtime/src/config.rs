//! Engine configuration

use anyhow::Result;

pub use compiler::OptLevel;

/// When functions get compiled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Strategy {
    /// Compile every function when the module is created.
    #[default]
    Eager,
}

/// How an [`Engine`](crate::Engine) behaves.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// When functions get compiled.
    pub strategy: Strategy,

    /// How hard Cranelift works.
    pub opt_level: OptLevel,

    /// Size of the guest address space, in bytes.
    ///
    /// Must be a power of two: guest addresses are confined by masking with
    /// `memory_size - 1`. The whole range is reserved lazily, so an unused tail
    /// costs address space rather than memory -- but a large value still costs
    /// address space per store, which matters when many run at once.
    pub memory_size: u64,

    /// Size of the guest stack, which occupies the top of the address space.
    pub stack_size: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            strategy: Strategy::default(),
            opt_level: OptLevel::default(),
            memory_size: rv::DEFAULT_MEMORY_SIZE,
            stack_size: 1 << 20,
        }
    }
}

impl Config {
    /// A fresh configuration.
    pub fn new() -> Self {
        Config::default()
    }

    /// Set when functions get compiled.
    pub fn strategy(&mut self, strategy: Strategy) -> &mut Self {
        self.strategy = strategy;
        self
    }

    /// Set how hard Cranelift works.
    pub fn opt_level(&mut self, level: OptLevel) -> &mut Self {
        self.opt_level = level;
        self
    }

    /// Set the guest address space size. Must be a power of two between
    /// [`rv::MIN_MEMORY_SIZE`] and [`rv::MAX_MEMORY_SIZE`]; validated when a
    /// module is compiled.
    pub fn memory_size(&mut self, bytes: u64) -> &mut Self {
        self.memory_size = bytes;
        self
    }

    /// Set the guest stack size. Rounded up to the host page size.
    pub fn stack_size(&mut self, bytes: u64) -> &mut Self {
        self.stack_size = bytes;
        self
    }
}

/// A compilation target and the configuration it was built with.
///
/// Cheap to clone; modules and stores hold their own handle.
#[derive(Clone)]
pub struct Engine {
    inner: compiler::Engine,
    config: Config,
}

impl Engine {
    /// Build an engine for the host machine.
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Engine {
            inner: compiler::Engine::new(config.opt_level)?,
            config: *config,
        })
    }

    /// The configuration this engine was built with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn compiler(&self) -> &compiler::Engine {
        &self.inner
    }
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(&Config::default()).expect("the host target is supported")
    }
}
