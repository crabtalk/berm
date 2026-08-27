//! Compiling a guest ahead of time, and keeping the result
//!
//! A guest still arrives as an ELF. The difference is that compiling it
//! produces an object, so the next time the same ELF arrives there is nothing
//! to compile -- the artifact is read out of the cache and mapped instead.

use crate::Engine;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Compile `elf`, reusing a stored artifact when one matches.
pub(crate) fn compile(engine: &Engine, elf: &[u8]) -> Result<compiler::Module> {
    let config = engine.config();
    let (memory_size, interruptible) = (config.memory_size, config.interruptible);
    let cache = engine.compiler().cache();

    let key = cache.map(|_| key(engine, elf));
    if let (Some(cache), Some(key)) = (cache, &key)
        && let Some(stored) = cache.load(key)
    {
        // A stored artifact is a file other things can damage, so a load that
        // fails is a miss rather than an error: it is recompiled below.
        match compiler::Module::load(engine.compiler(), &stored, memory_size, interruptible) {
            Ok(module) => return Ok(module),
            Err(error) => tracing::debug!("recompiling a damaged artifact: {error:#}"),
        }
    }

    let program = rv::elf::load(elf).context("failed to load the guest image")?;
    let artifact =
        compiler::Module::object(engine.compiler(), &program, elf, memory_size, interruptible)?;

    if let (Some(cache), Some(key)) = (cache, &key) {
        cache.store(key, &artifact);
    }

    // Run the artifact rather than anything held over from compiling it, so
    // the path that runs on a hit is the only path there is.
    compiler::Module::load(engine.compiler(), &artifact, memory_size, interruptible)
}

/// What to file this guest under.
///
/// Everything the compiled code depends on goes into the key, so an artifact
/// built for another target or address space is a different entry rather than
/// a stale one to be detected and thrown away.
fn key(engine: &Engine, elf: &[u8]) -> [u8; 32] {
    let isa = engine.compiler().isa();
    let mut hasher = Sha256::new();
    hasher.update(elf);
    hasher.update(isa.triple().to_string());
    hasher.update(isa.flags().to_string());
    hasher.update(engine.config().memory_size.to_le_bytes());
    hasher.update([engine.config().interruptible as u8]);
    hasher.finalize().into()
}
