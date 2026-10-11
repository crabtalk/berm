//! `berm usage` — what a program used, a day each.

use crate::Client;
use anyhow::Result;

pub fn run(client: &Client, name: &str) -> Result<()> {
    let meters = client.meters(name)?;
    if meters.is_empty() {
        println!("nothing used in the last 30 days");
        return Ok(());
    }
    println!(
        "{:<10}  {:>8}  {:>14}  {:>10}  {:>10}  {:>10}  {:>12}",
        "DAY", "CALLS", "INSTRUCTIONS", "READ", "WRITTEN", "MEMORY", "STORED BYTES"
    );
    for day in &meters {
        let date = chrono::DateTime::from_timestamp(i64::from(day.day) * 86_400, 0).map_or_else(
            || day.day.to_string(),
            |at| at.format("%Y-%m-%d").to_string(),
        );
        println!(
            "{date:<10}  {:>8}  {:>14}  {:>10}  {:>10}  {:>8}Ki  {:>12}",
            day.calls,
            day.instructions,
            day.read,
            day.written,
            day.memory / 1024,
            day.stored,
        );
    }
    Ok(())
}
