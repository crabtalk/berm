//! `berm publish` — list an already-pushed image in an index.

use crate::index::Index;
use anyhow::Result;

/// A credential for the index, when it wants one. An open index does not, so
/// this is not required — and it is deliberately not a GitHub token: an index
/// has no business holding one of those.
const TOKEN: &str = "BERM_TOKEN";

pub fn run(index: Option<&String>, reference: &str) -> Result<()> {
    let token = std::env::var(TOKEN).ok();

    // The index pulls the artifact itself to fill this in, so what it lists is
    // what a registry will actually serve — not what the publisher claimed.
    let entry = Index::new(index)?.publish(reference, token.as_deref())?;
    println!("{}", entry.reference);
    println!("  digest     {}", entry.digest);
    if let Some(publisher) = &entry.publisher {
        println!("  publisher  {publisher}");
    }
    for tool in &entry.tools {
        println!("  tool       {}", tool.name);
    }
    Ok(())
}
