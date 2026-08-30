//! `berm ls` — what is deployed.

use crate::Client;
use anyhow::Result;
use berm_api::Program;

/// Enough of a digest to tell two images apart at a glance, which is all a
/// listing is for. `inspect` prints the whole one.
const SHORT: usize = 12;

pub fn run(client: &Client) -> Result<()> {
    let programs = client.list()?;
    if programs.is_empty() {
        println!("no programs deployed");
        return Ok(());
    }

    let width = programs
        .iter()
        .map(|program| program.name.len())
        .max()
        .unwrap_or(0)
        .max("NAME".len());

    println!("{:<width$}  {:<SHORT$}  TOOLS", "NAME", "DIGEST");
    for program in &programs {
        println!(
            "{:<width$}  {:<SHORT$}  {}",
            program.name,
            digest(program),
            tools(program),
        );
    }
    Ok(())
}

fn digest(program: &Program) -> &str {
    program.digest.get(..SHORT).unwrap_or(&program.digest)
}

fn tools(program: &Program) -> String {
    program
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
