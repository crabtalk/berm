//! Where a program's bytes live between invocations.
//!
//! One file per key under `{root}/state/{program}`, the name hex-encoded so
//! any key a guest can build is a legal one. A directory per program is what
//! makes the isolation visible on disk as well as on the wire.

use crate::utils;
use anyhow::{Context, Result};
use berm::{Syscall, syscall::store};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// `berm.get` and `berm.set`, against files under `root`.
pub(crate) fn syscalls(root: &Path) -> Vec<Syscall> {
    let (reading, writing) = (root.to_owned(), root.to_owned());
    store::programs(
        move |program, key| read(&reading, program, key),
        move |program, key, value| write(&writing, program, key, value),
    )
}

fn key_path(root: &Path, program: &str, key: &str) -> PathBuf {
    root.join("state")
        .join(program)
        .join(hex::encode(key.as_bytes()))
}

/// What `program` last wrote under `key`, or `None`.
pub(crate) fn read(root: &Path, program: &str, key: &str) -> Result<Option<Vec<u8>>> {
    match fs::read(key_path(root, program, key)) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("failed to read stored state"),
    }
}

/// Write `key`, replacing whatever was there.
pub(crate) fn write(root: &Path, program: &str, key: &str, value: &[u8]) -> Result<()> {
    utils::write(&key_path(root, program, key), value)
}
