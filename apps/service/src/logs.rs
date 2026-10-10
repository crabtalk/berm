//! Where a program's emitted logs land.
//!
//! One JSON line per log, appended to `{root}/logs/{program}.jsonl`.

use anyhow::{Context, Result};
use berm::Log;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

/// Append each of `logs` to its program's file.
pub(crate) fn append(root: &Path, logs: &[Log]) -> Result<()> {
    let directory = root.join("logs");
    if !logs.is_empty() {
        fs::create_dir_all(&directory).context("failed to open the logs directory")?;
    }
    for log in logs {
        let mut line = serde_json::to_vec(&serde_json::json!({
            "topics": log.topics,
            "data": log.data,
        }))?;
        line.push(b'\n');
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join(format!("{}.jsonl", log.program)))
            .and_then(|mut file| file.write_all(&line))
            .context("failed to append a log")?;
    }
    Ok(())
}
