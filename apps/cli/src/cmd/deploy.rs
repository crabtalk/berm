//! `berm deploy` — hand bermd an image.

use crate::{Client, cmd::inspect};
use anyhow::{Context, Result};
use std::path::Path;

pub fn run(client: &Client, name: &str, image: &Path) -> Result<()> {
    let elf = std::fs::read(image).with_context(|| format!("cannot read {}", image.display()))?;

    let harness = client.deploy(name, elf)?;
    // The service compiled it to answer, so what comes back is what it will
    // serve — worth showing rather than saying "ok".
    inspect::show(&harness);
    Ok(())
}
