//! `berm logs` — what a program emitted.

use crate::Client;
use anyhow::Result;

/// Logs one request asks for.
const PAGE: u64 = 1000;

/// Print `name`'s logs after `after`, oldest first, one per line.
pub fn run(client: &Client, name: &str, after: u64) -> Result<()> {
    let mut after = after;
    loop {
        let page = client.logs(name, after, PAGE)?;
        for log in &page {
            println!("{}  {}  {}", log.seq, log.topics.join(","), log.data);
        }
        match page.last() {
            Some(last) if page.len() as u64 == PAGE => after = last.seq,
            _ => return Ok(()),
        }
    }
}
