//! One published version.

use berm_api::{Manifest, ToolSpec};
use serde::{Deserialize, Serialize};

/// What an index records about a harness at one digest.
///
/// Keyed by a digest, which is why the tools and usage beside it are safe to
/// keep: the bytes at a digest can never change, so a copy of what they said
/// cannot go stale. Re-pushing a tag does not rewrite an entry — it adds one,
/// and the old line still truthfully describes the old bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// As published, tag and all.
    pub reference: String,
    /// sha256 of the ELF, `sha256:`-prefixed, as the registry addresses it.
    pub digest: String,
    /// Whoever the publishing service vouched for, if it vouched for anyone.
    /// An open index records none, and the reference already names an owner
    /// that anyone can check against the registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    pub usage: String,
    pub tools: Vec<ToolSpec>,
}

impl Entry {
    pub fn new(
        reference: String,
        digest: String,
        publisher: Option<String>,
        manifest: Manifest,
    ) -> Self {
        Self {
            reference,
            digest,
            publisher,
            usage: manifest.usage,
            tools: manifest.tools,
        }
    }

    /// The harness this is a version of — the reference without its tag, which
    /// is what one file in the index is named for.
    pub fn key(&self) -> &str {
        match self.reference.split_once('@') {
            Some((repository, _)) => repository,
            None => self
                .reference
                .rsplit_once(':')
                .map_or(self.reference.as_str(), |(repository, _)| repository),
        }
    }

    /// Whether `term` appears in anything a person would search by. The tool
    /// descriptions are here because "which harness reads files" is the
    /// question being asked, and no tool is named after it.
    pub fn matches(&self, term: &str) -> bool {
        let term = term.to_lowercase();
        self.reference.to_lowercase().contains(&term)
            || self.usage.to_lowercase().contains(&term)
            || self.tools.iter().any(|tool| {
                tool.name.to_lowercase().contains(&term)
                    || tool.description.to_lowercase().contains(&term)
            })
    }
}
