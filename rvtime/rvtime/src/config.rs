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
#[derive(Clone, Debug)]
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

    /// Emit interrupt checks so a running guest can be stopped.
    ///
    /// On by default. The cost is a load, a test and a branch per loop
    /// iteration — measured at 0.2% on a tight 50-million-iteration loop, since
    /// the flag stays in L1 and the branch predicts perfectly. Without it a
    /// guest that loops forever holds the calling thread forever with no way to
    /// reclaim it, which is a far worse outcome than that.
    ///
    /// Turn it off only for a guest you trust and have profiled.
    pub interruptible: bool,

    /// Where to keep compiled artifacts, if anywhere.
    ///
    /// Without one, a guest is still compiled ahead of time but the object is
    /// discarded with it. With one, compiling the same ELF again is a read and
    /// a map. Artifacts are named for everything their code depends on, so a
    /// directory shared between differently configured engines yields separate
    /// files rather than stale ones.
    #[cfg(feature = "aot")]
    pub aot_dir: Option<std::path::PathBuf>,

    /// Where to cache generated code, if anywhere.
    ///
    /// Code generation is almost all of compile time, so a warm cache turns
    /// loading a previously seen guest into deserialisation. Entries are keyed
    /// on function contents and target settings, so a stale directory yields
    /// misses rather than wrong code, and it is safe to share between
    /// processes.
    pub cache_dir: Option<std::path::PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            strategy: Strategy::default(),
            opt_level: OptLevel::default(),
            memory_size: rv::DEFAULT_MEMORY_SIZE,
            stack_size: 1 << 20,
            interruptible: true,
            #[cfg(feature = "aot")]
            aot_dir: None,
            cache_dir: None,
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

    /// Emit interrupt checks, so [`Store::interrupt_handle`](crate::Store::interrupt_handle)
    /// can stop a running guest.
    pub fn interruptible(&mut self, yes: bool) -> &mut Self {
        self.interruptible = yes;
        self
    }

    /// Keep compiled artifacts under `dir`, reusing them across runs.
    #[cfg(feature = "aot")]
    pub fn aot_dir(&mut self, dir: impl Into<std::path::PathBuf>) -> &mut Self {
        self.aot_dir = Some(dir.into());
        self
    }

    /// Cache generated code under `dir`, reusing it across runs.
    pub fn cache_dir(&mut self, dir: impl Into<std::path::PathBuf>) -> &mut Self {
        self.cache_dir = Some(dir.into());
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
        let mut inner = compiler::Engine::new(config.opt_level)?;
        if let Some(dir) = &config.cache_dir {
            inner = inner.with_cache(dir)?;
        }
        Ok(Engine {
            inner,
            config: config.clone(),
        })
    }

    /// The configuration this engine was built with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Functions served from the code cache, and functions generated, since
    /// this engine was created. Both zero when no cache is configured.
    pub fn cache_stats(&self) -> (usize, usize) {
        self.inner
            .cache()
            .map(|c| (c.hits(), c.misses()))
            .unwrap_or((0, 0))
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
