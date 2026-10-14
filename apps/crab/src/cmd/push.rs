//! `crab push` — publish an image to a registry.

use crate::cmd::inspect;
use anyhow::{Context, Result};
use berm_api::{Manifest, Program, Terms};
use berm_oci::{Access, Reference, Registry};
use std::{fs, path::Path, str::FromStr};

pub fn run(reference: &str, image: &Path) -> Result<()> {
    let bytes = fs::read(image).with_context(|| format!("cannot read {}", image.display()))?;
    // Refused here, by whoever built it, rather than at deploy on someone
    // else's machine. Reading the manifest never runs the guest.
    let manifest = Manifest::from_image(&bytes)?;
    let digest = push(reference, &bytes)?;

    // What the registry will now serve, rendered the way `inspect` renders
    // what the service serves.
    inspect::show(
        &Program {
            name: reference.to_string(),
            version: manifest.version,
            digest,
            usage: manifest.usage,
            tools: manifest.tools,
            deps: manifest.deps,
            events: manifest.events,
            // Nothing to resolve against: a registry runs no programs and dials
            // nowhere, so what this image needs is a question for whoever deploys it.
            unresolved: Default::default(),
            breaks: Vec::new(),
        },
        &Terms::default(),
    );
    Ok(())
}

/// Upload `image` as `reference`, answering with its digest.
pub fn push(reference: &str, image: &[u8]) -> Result<String> {
    let reference = Reference::from_str(reference)?;
    let section = Manifest::section(image)?;
    Registry::open(&reference, Access::Write)?.push(&reference.reference, image, section)
}
