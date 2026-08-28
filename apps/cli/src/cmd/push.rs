//! `berm push` — publish an image to a registry.

use crate::cmd::inspect;
use anyhow::{Context, Result};
use berm_api::{Harness, Manifest};
use berm_oci::{Access, Reference, Registry};
use std::{fs, path::Path, str::FromStr};

pub fn run(reference: &str, image: &Path) -> Result<()> {
    let reference = Reference::from_str(reference)?;
    let elf = fs::read(image).with_context(|| format!("cannot read {}", image.display()))?;

    // Refused here, by whoever built it, rather than at deploy on someone
    // else's machine. Reading the manifest never runs the guest.
    let manifest = Manifest::from_elf(&elf)?;
    let section = Manifest::section(&elf)?;

    let registry = Registry::open(&reference, Access::Write)?;
    let digest = registry.push(&reference.reference, &elf, section)?;

    // What the registry will now serve, rendered the way `inspect` renders
    // what the service serves.
    inspect::show(&Harness {
        name: reference.to_string(),
        digest,
        usage: manifest.usage,
        tools: manifest.tools,
        deps: manifest.deps,
        // Nothing to resolve against: a registry runs no harnesses and dials
        // nowhere, so what this image needs is a question for whoever deploys it.
        unresolved: Vec::new(),
    });
    Ok(())
}
