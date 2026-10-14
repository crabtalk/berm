//! `crab idl` — a deployed program's IDL, for `declare_program!`.

use crate::Client;
use anyhow::{Context, Result};
use std::{fs, path::Path};

/// Write `name`'s IDL to `{dir}/{name}.json`.
pub fn run(client: &Client, name: &str, dir: &Path) -> Result<()> {
    let mut program = client.inspect(name)?;
    // What this service lacks is no part of what the program is, and its
    // internal tools are no one else's to call.
    program.unresolved = Default::default();
    program.tools.retain(|tool| tool.public);
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{name}.json"));
    let json = serde_json::to_string_pretty(&program)?;
    fs::write(&path, json + "\n").with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}
