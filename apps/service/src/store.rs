//! Where a program's bytes live between invocations.
//!
//! One file per key under `{root}/state/{program}`, the name hex-encoded so
//! any key a guest can build is a legal one. A directory per program is what
//! makes the isolation visible on disk as well as on the wire.

use crate::{Service, database, utils};
use anyhow::{Context, Result};
use berm::{Syscall, Write, syscall::store};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Weak,
};

/// `berm.get` and `berm.set`, reading files under `root`, or the program's
/// own database when it keeps one.
pub(crate) fn syscalls(root: &Path, service: Weak<Service>) -> Vec<Syscall> {
    let root = root.to_owned();
    store::programs(move |program, key| {
        let (root, service) = (root.clone(), service.upgrade());
        async move {
            let kept = service.as_ref().and_then(|service| {
                service
                    .get(&program)
                    .filter(|deployed| database::keeps(deployed))
                    .map(|_| service.databases.get(&program))
            });
            match kept {
                Some(database) => database?.get(key).await,
                None => read(&root, &program, &key),
            }
        }
    })
}

/// Write what a chain left under `root`.
pub(crate) fn commit(root: &Path, writes: &[Write]) -> Result<()> {
    for write in writes {
        utils::write(&key_path(root, &write.program, &write.key), &write.value)?;
    }
    Ok(())
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
