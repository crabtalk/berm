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

        let (repository, reference) = match rest.split_once('@') {
            Some(split) => split,
            None => rest.rsplit_once(':').unwrap_or((rest, "latest")),
        };

        // A host is case-insensitive and a repository has to be lowercase, so
        // both are folded rather than refused: a GitHub org spelled with
        // capitals names the same package either way, and asking someone to
        // retype it would buy nothing. The tag is left alone — its grammar
        // admits uppercase, and `:V1` is a different tag from `:v1`.
        let registry = registry.to_ascii_lowercase();
        let repository = repository.to_ascii_lowercase();

        if !registry.contains('.') && !registry.contains(':') && registry != "localhost" {
            bail!("{text:?} names no registry: {registry:?} is not a host");
        }
        if !host(&registry) {
            bail!("{text:?} names no registry: {registry:?} is not a hostname");
        }
        if repository.is_empty() {
            bail!("{text:?} names no repository");
        }
        if !repository.split('/').all(component) {
            bail!(
                "{text:?} names no repository: {repository:?} is not alphanumeric separated by \
                 `.`, `_`, `__`, `-` or `/`"
            );
        }

        Ok(Self {
            registry,
            repository,
            reference: reference.to_owned(),
        })
    }
}

/// One `/`-separated piece of a repository, against the OCI name grammar:
/// alphanumeric runs joined by `.`, `_`, `__`, or a run of `-`. Case is already
/// folded by the time this sees it.
///
/// Checked rather than assumed because a repository becomes a path. An index
/// holds one file per program at `{registry}/{repository}.json`, and `..` in a
/// name a registry served would write outside it — `Path::join` concatenates,
/// and the `..` is resolved by the kernel at open time. The grammar has no way
/// to spell that, so enforcing the grammar is the whole fix.
fn component(text: &str) -> bool {
    let bytes = text.as_bytes();
    let alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if bytes.first().is_none_or(|byte| !alphanumeric(*byte))
        || bytes.last().is_none_or(|byte| !alphanumeric(*byte))
    {
        return false;
    }

    let mut at = 0;
    while at < bytes.len() {
        if alphanumeric(bytes[at]) {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && !alphanumeric(bytes[at]) {
            at += 1;
        }
        let separator = &bytes[start..at];
        if !matches!(separator, b"." | b"_" | b"__") && !separator.iter().all(|byte| *byte == b'-')
        {
            return false;
        }
    }
    true
}

/// Whether `text` is a hostname, with an optional port.
///
/// The registry is the first path segment of an index's tree, so it is subject
/// to the same rule as a repository: `..` contains a dot and would otherwise
/// pass for a host.
fn host(text: &str) -> bool {
    let (name, port) = match text.rsplit_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (text, None),
    };
    if port.is_some_and(|port| port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    !name.is_empty()
        && name.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
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
