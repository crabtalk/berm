//! Small things that belong to no one type.

use anyhow::{Context, Result};
use std::{fs, path::Path};

/// Write `bytes` to `path`, replacing whatever was there.
///
/// Through a temporary and a rename: a reader is given the whole of the last
/// write or the whole of this one, never half of either.
pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().context("a stored path has no directory")?;
    fs::create_dir_all(directory).context("failed to open a storage directory")?;

    let staging = path.with_extension("writing");
    fs::write(&staging, bytes).context("failed to stage a write")?;
    fs::rename(&staging, path).context("failed to complete a write")
}
