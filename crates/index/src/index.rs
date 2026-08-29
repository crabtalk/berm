//! The list, held and searched.

use crate::Entry;
use anyhow::{Context, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Every published version, by the program it belongs to.
///
/// Held whole, because a clone of the index is a directory of small files and
/// searching one means reading all of it either way. That is fine into the low
/// thousands of programs and wrong past them, which is the ceiling of a list
/// you can `git clone`.
#[derive(Debug, Default)]
pub struct Index {
    entries: BTreeMap<String, Vec<Entry>>,
}

impl Index {
    /// Read a clone of an index: every `.json` under `root`, each a line per
    /// version.
    pub fn load(root: &Path) -> Result<Self> {
        let mut index = Self::default();
        for path in files(root)? {
            let body = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            for line in body.lines().filter(|line| !line.trim().is_empty()) {
                // One unreadable line does not make the rest untrue, and a
                // reader that refuses the whole index over it is worse than one
                // that skips it.
                let Ok(entry) = serde_json::from_str::<Entry>(line) else {
                    continue;
                };
                index
                    .entries
                    .entry(entry.key().to_owned())
                    .or_default()
                    .push(entry);
            }
        }
        Ok(index)
    }

    /// Everything matching `term`, or everything when it is empty.
    pub fn search(&self, term: &str) -> Vec<&Entry> {
        self.entries
            .values()
            .flatten()
            .filter(|entry| term.is_empty() || entry.matches(term))
            .collect()
    }

    /// One program's versions, in the order they were published.
    pub fn versions(&self, key: &str) -> Option<&[Entry]> {
        self.entries.get(key).map(Vec::as_slice)
    }

    /// How many programs are listed, which is not how many versions there are.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Every `.json` under `root`, at any depth — the tree is
/// `{registry}/{repository}.json`, so it is as deep as a repository path.
fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(at) = pending.pop() {
        let listing =
            std::fs::read_dir(&at).with_context(|| format!("cannot read {}", at.display()))?;
        for entry in listing {
            let path = entry?.path();
            match path.is_dir() {
                true => pending.push(path),
                false if path.extension().is_some_and(|kind| kind == "json") => found.push(path),
                false => {}
            }
        }
    }
    Ok(found)
}
