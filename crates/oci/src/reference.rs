//! Where an image lives.

use anyhow::{Context, Result, bail};
use std::{fmt, str::FromStr};

/// `ghcr.io/org/name:tag`, or `…@sha256:…`.
pub struct Reference {
    pub registry: String,
    pub repository: String,
    /// A tag, or a `sha256:…` digest.
    pub reference: String,
}

impl FromStr for Reference {
    type Err = anyhow::Error;

    /// Docker's rule: the first segment is a registry when it looks like a
    /// host. Unlike Docker, an unqualified name is refused rather than sent to
    /// a default registry — berm has no reason to pick one.
    fn from_str(text: &str) -> Result<Self> {
        let (registry, rest) = text
            .split_once('/')
            .with_context(|| format!("{text:?} names no registry"))?;
        if !registry.contains('.') && !registry.contains(':') && registry != "localhost" {
            bail!("{text:?} names no registry: {registry:?} is not a host");
        }

        let (repository, reference) = match rest.split_once('@') {
            Some(split) => split,
            None => rest.rsplit_once(':').unwrap_or((rest, "latest")),
        };
        if repository.is_empty() {
            bail!("{text:?} names no repository");
        }

        Ok(Self {
            registry: registry.to_owned(),
            repository: repository.to_owned(),
            reference: reference.to_owned(),
        })
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let held = if self.reference.starts_with("sha256:") {
            '@'
        } else {
            ':'
        };
        write!(
            f,
            "{}/{}{held}{}",
            self.registry, self.repository, self.reference
        )
    }
}
