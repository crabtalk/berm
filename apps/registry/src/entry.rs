//! One published version.

use berm_api::{Manifest, ToolSpec};
use serde::{Deserialize, Serialize};

/// What the index records about a harness at one digest.
///
/// Keyed by a digest, which is why the tools and usage beside it are safe to
/// keep: the bytes at a digest can never change, so a copy of what they said
/// cannot go stale. Re-pushing a tag does not rewrite an entry — it adds one,
/// and the old line still truthfully describes the old bytes.
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    /// As published, tag and all.
    pub reference: String,
    /// sha256 of the ELF, `sha256:`-prefixed, as the registry addresses it.
    pub digest: String,
    /// The GitHub login that published it, verified at the time.
    pub publisher: String,
    pub usage: String,
    pub tools: Vec<ToolSpec>,
}

impl Entry {
    pub fn new(reference: String, digest: String, publisher: String, manifest: Manifest) -> Self {
        Self {
            reference,
            digest,
            publisher,
            usage: manifest.usage,
            tools: manifest.tools,
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
