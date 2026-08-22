//! `berm publish` — list an already-pushed image in an index.

use crate::index::Index;
use anyhow::{Context, Result};

/// The same credential `push` uses, for the same reason: it is what CI has.
const TOKEN: &str = "GITHUB_TOKEN";

pub fn run(index: Option<&String>, reference: &str) -> Result<()> {
    let token = std::env::var(TOKEN)
        .context("publishing needs a GitHub token: set GITHUB_TOKEN, or export `gh auth token`")?;

    // The index pulls the artifact itself to fill this in, so what it lists is
    // what a registry will actually serve — not what the publisher claimed.
    let entry = Index::new(index)?.publish(reference, &token)?;
    println!("{}", entry.reference);
    println!("  digest     {}", entry.digest);
    println!("  publisher  {}", entry.publisher);
    for tool in &entry.tools {
        println!("  tool       {}", tool.name);
    }
    Ok(())
}
