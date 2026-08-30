//! Where a program's bytes become code that runs.
//!
//! What a backend supplies is transport and nothing else. Every syscall is
//! written once against [`Guest`], in `crate::syscall`, so a second backend
//! carries the same table across its own boundary rather than restating it.
//!
//! Which one a deploy reaches is read off the image's first four bytes.

use crate::{Invocation, syscall::Table};
use anyhow::{Result, bail};
use std::{ops::Range, path::PathBuf};

#[cfg(feature = "riscv")]
mod riscv;

/// The guest a syscall was called from.
///
/// Reading copies rather than borrowing: every syscall already owns its
/// request by the time it runs, and a borrow would have to outlive the
/// mutable one [`Self::invocation`] takes.
pub(crate) trait Guest {
    /// Copy `len` bytes of guest memory at `addr`.
    fn read(&mut self, addr: u64, len: u64) -> Result<Vec<u8>>;

    /// Copy `bytes` into guest memory at `addr`.
    fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<()>;

    /// Where this guest's heap is. An error on a backend whose guests manage
    /// their own memory, which is not a failure — nothing there asks.
    fn heap(&mut self) -> Result<Range<u64>>;

    /// The invocation this call belongs to.
    fn invocation(&mut self) -> &mut Invocation;
}

/// How a runtime compiles and holds code.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Where compiled code is cached, if anywhere.
    ///
    /// Entries are keyed on contents, so a stale directory yields misses
    /// rather than wrong code, and it is safe to share between processes.
    pub cache_dir: Option<PathBuf>,
}

/// A compilation target, shared by every program a runtime holds.
///
/// Cheap to clone; each compiled image keeps its own handle.
#[derive(Clone)]
pub struct Engine {
    #[cfg(feature = "riscv")]
    riscv: rvtime::Engine,
}

impl Engine {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "riscv")]
            riscv: riscv::engine(config)?,
        })
    }
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(&Config::default()).expect("the host target is supported")
    }
}

/// One compiled program, ready to instantiate.
pub(crate) enum Image {
    #[cfg(feature = "riscv")]
    Riscv(riscv::Image),
}

impl Image {
    /// Compile `image` against `table`, picking the backend its first bytes
    /// name. The image already says what it is, and a second answer beside it
    /// is one that can disagree.
    pub(crate) fn compile(engine: &Engine, image: &[u8], table: Table) -> Result<Self> {
        match image {
            [0x7f, b'E', b'L', b'F', ..] => {
                #[cfg(feature = "riscv")]
                return Ok(Self::Riscv(riscv::Image::compile(
                    &engine.riscv,
                    image,
                    table,
                )?));
                #[cfg(not(feature = "riscv"))]
                bail!("program is an ELF, which this build of berm was not compiled with");
            }
            _ => bail!("program is neither WebAssembly nor an ELF"),
        }
    }

    /// Every name this image exports. Which of them are tools is the ABI's
    /// question, not a backend's.
    pub(crate) fn exports(&self) -> Vec<&str> {
        match self {
            #[cfg(feature = "riscv")]
            Self::Riscv(image) => image.exports(),
        }
    }

    /// Instantiate and run one tool.
    ///
    /// The invocation goes in carrying the arguments and comes back carrying
    /// whatever the tool answered with. Nothing else survives: the instance is
    /// built for this call and dropped with it.
    pub(crate) fn call(&self, symbol: &str, invocation: Invocation) -> Result<Invocation> {
        match self {
            #[cfg(feature = "riscv")]
            Self::Riscv(image) => image.call(symbol, invocation),
        }
    }
}
