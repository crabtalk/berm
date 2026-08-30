//! `berm search` — what has been published.

use anyhow::Result;
use berm_index::Source;

/// Enough of a digest to tell two images apart at a glance, as `ls` uses.
const SHORT: usize = 12;

pub fn run(index: Option<&str>, term: &str) -> Result<()> {
    let entries = Source::new(index)?.search(term)?;
    if entries.is_empty() {
        match term.is_empty() {
            true => println!("nothing published"),
            false => println!("nothing published matches {term:?}"),
        }
        return Ok(());
    }

    let width = entries
        .iter()
        .map(|entry| entry.reference.len())
        .max()
        .unwrap_or(0)
        .max("PROGRAM".len());

    println!("{:<width$}  {:<SHORT$}  TOOLS", "PROGRAM", "DIGEST");
    for entry in &entries {
        println!(
            "{:<width$}  {:<SHORT$}  {}",
            entry.reference,
            digest(entry),
            tools(entry),
        );
    }
    Ok(())
}

fn digest(entry: &berm_index::Entry) -> &str {
    let hex = entry
        .digest
        .strip_prefix("sha256:")
        .unwrap_or(&entry.digest);
    hex.get(..SHORT).unwrap_or(hex)
}

fn tools(entry: &berm_index::Entry) -> String {
    entry
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
