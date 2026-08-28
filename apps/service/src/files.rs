//! The runtime's records, as files under the service root.
//!
//! A directory per kind, and one file per record inside it. What berm keeps
//! there — images, connections, wakes — is berm's to say; this only decides
//! that it is a filesystem and where.

use anyhow::{Context, Result};
use berm::{Records, Storage};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Written on the end of every record, so a listing can tell one from whatever
/// else shares the directory.
const SUFFIX: &str = "berm";

pub(crate) struct Files {
    root: PathBuf,
}

impl Files {
    pub(crate) fn open(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    fn path(&self, records: Records, key: &str) -> PathBuf {
        // Hex, so any key berm builds is a legal filename — an id, a harness
        // name, and whatever a later kind of record is keyed by.
        self.root
            .join(records.as_str())
            .join(format!("{}.{SUFFIX}", hex::encode(key.as_bytes())))
    }
}

impl Storage for Files {
    fn put(&self, records: Records, key: &str, value: &[u8]) -> Result<()> {
        crate::utils::write(&self.path(records, key), value)
    }

    fn remove(&self, records: Records, key: &str) -> Result<bool> {
        match fs::remove_file(self.path(records, key)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).context("failed to drop a record"),
        }
    }

    fn list(&self, records: Records) -> Result<Vec<(String, Vec<u8>)>> {
        let directory = self.root.join(records.as_str());
        if !directory.is_dir() {
            return Ok(Vec::new());
        }

        let mut held = Vec::new();
        for entry in fs::read_dir(&directory).context("failed to read a record directory")? {
            let path = entry?.path();
            let Some(key) = path
                .extension()
                .filter(|extension| *extension == SUFFIX)
                .and_then(|_| path.file_stem())
                .and_then(|stem| stem.to_str())
                .and_then(|stem| hex::decode(stem).ok())
                .and_then(|key| String::from_utf8(key).ok())
            else {
                continue;
            };
            held.push((key, fs::read(&path).context("failed to read a record")?));
        }
        Ok(held)
    }
}
