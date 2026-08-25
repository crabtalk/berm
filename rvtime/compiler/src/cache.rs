//! A disk-backed cache for compiled code
//!
//! One directory, whatever the backend has to keep in it: generated code per
//! function for the JIT, whole object artifacts for the loader. Both want the
//! same three things -- a content-addressed name, an atomic write, and a count
//! of what was reused -- so both get them from here.
//!
//! Compilation is dominated by code generation — for a 99 KiB guest, decoding
//! and analysing the ELF is under 1% of the work and Cranelift is the rest. So
//! the thing worth caching is the generated code, and Cranelift's incremental
//! cache is keyed on a hash of the CLIF function plus the ISA settings.
//!
//! That key is what makes the cache safe to share. It captures the function's
//! contents rather than its name or address, so two guests containing the same
//! function reuse one entry, and a change to the target or optimisation level
//! produces different keys rather than stale code.
//!
//! Entries are written atomically, because a daemon may compile the same plugin
//! from several processes at once.

use anyhow::{Context, Result};
use cranelift::codegen::incremental_cache::CacheKvStore;
use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Compiled functions kept under a directory.
///
/// Shared behind an [`Arc`], so every module compiled from one engine
/// accumulates into the same cache. All of its state is atomic or lives in the
/// filesystem, which arbitrates concurrent writers.
pub struct Cache {
    dir: PathBuf,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl Cache {
    /// Open (and create) a cache directory.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create cache directory {}", dir.display()))?;
        Ok(Cache {
            dir,
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        })
    }

    /// The directory entries are kept in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Entries served from the cache since it was opened.
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    /// Entries that had to be compiled.
    pub fn misses(&self) -> usize {
        self.misses.load(Ordering::Relaxed)
    }

    /// Read the entry `key` names, counting the hit or the miss.
    pub fn load(&self, key: &[u8]) -> Option<Vec<u8>> {
        match fs::read(self.path(key)) {
            Ok(blob) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(blob)
            }
            Err(_) => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Write the entry `key` names. Failure is silent: the caller has the
    /// value in hand either way, and a cache that cannot be written is slow
    /// rather than broken.
    pub fn store(&self, key: &[u8], value: &[u8]) {
        // Write to a unique temporary and rename over the target. A partially
        // written entry would otherwise be indistinguishable from a complete
        // one, and rename is atomic on every filesystem we care about.
        let target = self.path(key);
        let temp = target.with_extension(format!("tmp{}", std::process::id()));

        if fs::write(&temp, value).is_ok() && fs::rename(&temp, &target).is_err() {
            let _ = fs::remove_file(&temp);
        }
    }

    fn path(&self, key: &[u8]) -> PathBuf {
        use std::fmt::Write;
        let mut name = String::with_capacity(key.len() * 2);
        for byte in key {
            let _ = write!(name, "{byte:02x}");
        }
        self.dir.join(name)
    }
}

/// Adapts a shared [`Cache`] to Cranelift's store trait, which takes `&mut self`
/// to insert. Nothing here needs unique access; the wrapper exists only to
/// satisfy the signature.
pub(crate) struct Handle(pub Arc<Cache>);

impl CacheKvStore for Handle {
    fn get(&self, key: &[u8]) -> Option<Cow<'_, [u8]>> {
        self.0.load(key).map(Cow::Owned)
    }

    fn insert(&mut self, key: &[u8], value: Vec<u8>) {
        self.0.store(key, &value);
    }
}
