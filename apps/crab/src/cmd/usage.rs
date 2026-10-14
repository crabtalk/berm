//! `crab usage` — what a program used, a day each.

use crate::utils;
use anyhow::Result;
use crabtalk_sdk::client::Client;

pub fn run(client: &Client, name: &str) -> Result<()> {
    let meters = utils::wait(client.meters(name))?;
    if meters.is_empty() {
        println!("nothing used in the last 30 days");
        return Ok(());
    }
    println!(
        "{:<10}  {:>8}  {:>14}  {:>14}  {:>10}  {:>10}  {:>10}  {:>13}  {:>12}",
        "DAY",
        "CALLS",
        "INSTRUCTIONS",
        "OVERAGE",
        "READ",
        "WRITTEN",
        "MEMORY",
        "PROXIED BYTES",
        "STORED BYTES"
    );
    for day in &meters {
        let date = chrono::DateTime::from_timestamp(i64::from(day.day) * 86_400, 0).map_or_else(
            || day.day.to_string(),
            |at| at.format("%Y-%m-%d").to_string(),
        );
        println!(
            "{date:<10}  {:>8}  {:>14}  {:>14}  {:>10}  {:>10}  {:>8}Ki  {:>13}  {:>12}",
            day.calls,
            day.instructions,
            day.overage,
            day.read,
            day.written,
            day.memory / 1024,
            day.proxied,
            day.stored,
        );
    }
    Ok(())
}
