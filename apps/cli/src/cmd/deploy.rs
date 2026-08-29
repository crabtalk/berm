//! `berm deploy` — hand bermd an image.

use crate::{Client, cmd::inspect};
use anyhow::{Context, Result};
use berm_oci::{Access, Reference, Registry};
use std::{fs, path::Path, str::FromStr};

pub fn run(client: &Client, name: &str, image: &str) -> Result<()> {
    let program = client.deploy(name, read(image)?)?;
    // The service compiled it to answer, so what comes back is what it will
    // serve — worth showing rather than saying "ok".
    inspect::show(&program);
    Ok(())
}

/// A file if one is there, a registry reference otherwise. The file wins, so an
/// image sitting in the working directory is never mistaken for something to go
/// and fetch.
fn read(image: &str) -> Result<Vec<u8>> {
    let path = Path::new(image);
    if path.exists() {
        return fs::read(path).with_context(|| format!("cannot read {image}"));
    }

    let reference = Reference::from_str(image)
        .with_context(|| format!("{image:?} is neither a file nor a registry reference"))?;
    Registry::open(&reference, Access::Read)?.pull(&reference.reference)
}
