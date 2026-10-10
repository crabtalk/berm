//! Where a program's emitted logs land, and how far each subscriber has read.
//!
//! One JSON line per log, appended to `{root}/logs/{program}.jsonl`; a log's
//! `seq` is its line, counting from 1. A subscriber's position is the last
//! `seq` delivered, under `{root}/positions/{subscriber}/{program}`.

use crate::utils;
use anyhow::{Context, Result};
use berm::{Log, Logged};
use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// One line of a program's logs.
#[derive(Deserialize)]
struct Line {
    topics: Vec<String>,
    data: String,
}

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

/// `program`'s logs after `seq`, oldest first.
pub(crate) fn after(root: &Path, program: &str, seq: u64) -> Result<Vec<Logged>> {
    let text = match fs::read_to_string(root.join("logs").join(format!("{program}.jsonl"))) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("failed to read logs"),
    };
    text.lines()
        .zip(1..)
        .skip(seq as usize)
        .map(|(line, seq)| {
            let line: Line = serde_json::from_str(line).context("a log line is not JSON")?;
            Ok(Logged {
                seq,
                topics: line.topics,
                data: line.data,
            })
        })
        .collect()
}

/// The last `seq` of `program` delivered to `subscriber`, or 0.
pub(crate) fn position(root: &Path, subscriber: &str, program: &str) -> Result<u64> {
    match fs::read_to_string(position_path(root, subscriber, program)) {
        Ok(text) => text.trim().parse().context("a position is not a number"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error).context("failed to read a position"),
    }
}

pub(crate) fn advance(root: &Path, subscriber: &str, program: &str, seq: u64) -> Result<()> {
    utils::write(
        &position_path(root, subscriber, program),
        seq.to_string().as_bytes(),
    )
}

fn position_path(root: &Path, subscriber: &str, program: &str) -> PathBuf {
    root.join("positions").join(subscriber).join(program)
}
