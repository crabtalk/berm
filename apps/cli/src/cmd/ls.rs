//! `berm ls` — what is deployed.

use crate::Client;
use anyhow::Result;
use berm_api::Harness;

/// Enough of a digest to tell two images apart at a glance, which is all a
/// listing is for. `inspect` prints the whole one.
const SHORT: usize = 12;

pub fn run(client: &Client) -> Result<()> {
    let harnesses = client.list()?;
    if harnesses.is_empty() {
        println!("no harnesses deployed");
        return Ok(());
    }

    let width = harnesses
        .iter()
        .map(|harness| harness.name.len())
        .max()
        .unwrap_or(0)
        .max("NAME".len());

    println!("{:<width$}  {:<SHORT$}  TOOLS", "NAME", "DIGEST");
    for harness in &harnesses {
        println!(
            "{:<width$}  {:<SHORT$}  {}",
            harness.name,
            digest(harness),
            tools(harness),
        );
    }
    Ok(())
}

fn digest(harness: &Harness) -> &str {
    harness.digest.get(..SHORT).unwrap_or(&harness.digest)
}

fn tools(harness: &Harness) -> String {
    harness
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
