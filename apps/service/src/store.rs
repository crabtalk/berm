//! Where a harness's bytes live between invocations.
//!
//! One file per key under `{root}/state/{harness}`, the name hex-encoded so
//! any key a guest can build is a legal one. A directory per harness is what
//! makes the isolation visible on disk as well as on the wire.

use anyhow::{Context, Result};
use berm::{System, system::store};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// `berm.get` and `berm.set`, against files under `root`.
pub(crate) fn system(root: &Path) -> Vec<System> {
    let (reading, writing) = (root.to_owned(), root.to_owned());
    store::harnesses(
        move |harness, key| read(&reading, harness, key),
        move |harness, key, value| write(&writing, harness, key, value),
    )
}

fn key_path(root: &Path, harness: &str, key: &str) -> PathBuf {
    root.join("state")
        .join(harness)
        .join(hex::encode(key.as_bytes()))
}

/// What `harness` last wrote under `key`, or `None`.
pub(crate) fn read(root: &Path, harness: &str, key: &str) -> Result<Option<Vec<u8>>> {
    match fs::read(key_path(root, harness, key)) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("failed to read stored state"),
    }
}

/// Write `key`, replacing whatever was there.
///
/// Through a temporary and a rename: a reader is either given the whole of the
/// last write or the whole of this one, never half of either.
pub(crate) fn write(root: &Path, harness: &str, key: &str, value: &[u8]) -> Result<()> {
    let path = key_path(root, harness, key);
    let directory = path.parent().context("stored state has no directory")?;
    fs::create_dir_all(directory).context("failed to open the state directory")?;

    let staging = path.with_extension("writing");
    fs::write(&staging, value).context("failed to stage stored state")?;
    fs::rename(&staging, &path).context("failed to store state")
}
