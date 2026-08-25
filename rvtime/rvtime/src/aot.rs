//! Compiling a guest ahead of time, and keeping the result
//!
//! A guest still arrives as an ELF. The difference is that compiling it
//! produces a file, so the next time the same ELF arrives there is nothing to
//! compile -- the object is mapped and run instead.

use crate::Engine;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Compile `elf`, reusing a stored artifact when one matches.
pub(crate) fn compile(engine: &Engine, elf: &[u8]) -> Result<compiler::Module> {
    let config = engine.config();
    let (memory_size, interruptible) = (config.memory_size, config.interruptible);

    let path = config
        .aot_dir
        .as_ref()
        .map(|dir| dir.join(name(engine, elf)));
    if let Some(path) = &path
        && let Ok(stored) = std::fs::read(path)
    {
        // A stored artifact is a file other things can damage, so a load that
        // fails is a miss rather than an error: it is recompiled below.
        match compiler::Module::load(engine.compiler(), &stored, memory_size, interruptible) {
            Ok(module) => return Ok(module),
            Err(error) => tracing::debug!("recompiling {}: {error:#}", path.display()),
        }
    }

    let program = rv::elf::load(elf).context("failed to load the guest image")?;
    let artifact =
        compiler::Module::object(engine.compiler(), &program, elf, memory_size, interruptible)?;

    if let Some(path) = &path {
        store(path, &artifact);
    }

    // Run the artifact rather than anything held over from compiling it, so
    // the path that runs on a hit is the only path there is.
    compiler::Module::load(engine.compiler(), &artifact, memory_size, interruptible)
}

/// What to file this guest under.
///
/// Everything the compiled code depends on goes into the name, so an artifact
/// built for another target or address space is a different file rather than a
/// stale one to be detected and thrown away.
fn name(engine: &Engine, elf: &[u8]) -> String {
    let isa = engine.compiler().isa();
    let mut hasher = Sha256::new();
    hasher.update(elf);
    hasher.update(isa.triple().to_string());
    hasher.update(isa.flags().to_string());
    hasher.update(engine.config().memory_size.to_le_bytes());
    hasher.update([engine.config().interruptible as u8]);
    format!("{:x}.o", hasher.finalize())
}

/// Write an artifact where the next run will find it.
///
/// Written to a unique temporary and renamed over the target, because several
/// processes may compile the same guest at once and a half-written file would
/// otherwise be indistinguishable from a complete one. Failure is not an
/// error: the artifact was compiled and can still be run.
fn store(path: &Path, artifact: &[u8]) {
    if let Some(dir) = path.parent()
        && let Err(error) = std::fs::create_dir_all(dir)
    {
        tracing::debug!("cannot create {}: {error}", dir.display());
        return;
    }

    let temp: PathBuf = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&temp, artifact).is_ok() && std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
}
