//! What a harness says it is, and what bermd's control API speaks.
//!
//! Here rather than in the service so a client can read an image and talk to
//! the API without linking a compiler.

use anyhow::{Context, Result, bail};
use object::{Object, ObjectSection};
use serde::{Deserialize, Serialize};

/// The ABI this host speaks. A harness built against a different one is
/// refused rather than dispatched into a system harness its author did not
/// mean.
pub const ABI_VERSION: u32 = 0;

/// ELF section carrying the manifest. A section rather than an export, so
/// reading what a harness claims to be never means running it.
pub const ABI_SECTION: &str = ".berm.abi";

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub abi_version: u32,
    pub tools: Vec<ToolSpec>,
    /// When to reach for these tools, and how they go together — the
    /// question no single tool's `description` answers, because it is about
    /// choosing between them. An embedder puts this in front of a model
    /// before it decides, so it is paid on every turn: a few lines, not a
    /// manual.
    #[serde(default)]
    pub usage: String,
    /// What this harness reaches for once it runs: harnesses it calls by name,
    /// and hosts it dials, told apart by whether one carries a scheme.
    ///
    /// Runtime, and resolved wherever the image lands — nothing here is
    /// fetched or installed, and a name nothing answers to is reported rather
    /// than refused. Declared by the author because a target computed at the
    /// call cannot be read off the image at all.
    #[serde(default)]
    pub deps: Vec<String>,
}

impl Manifest {
    /// Read what an ELF claims to be, without compiling or running it.
    ///
    /// This is what the section is *for* (RFC 0205): learning a harness's tools,
    /// deps, and usage must not mean instantiating it. An embedder assembling a
    /// prompt or listing a registry needs exactly this and nothing else.
    pub fn from_elf(elf: &[u8]) -> Result<Self> {
        let json = str::from_utf8(Self::section(elf)?).context("harness manifest is not UTF-8")?;
        Self::parse(json)
    }

    /// The section's bytes as they sit in the ELF. A publisher carries these
    /// verbatim, so what a registry serves cannot disagree with what the image
    /// holds.
    pub fn section(elf: &[u8]) -> Result<&[u8]> {
        let file = object::File::parse(elf).context("harness is not a readable ELF")?;
        file.section_by_name(ABI_SECTION)
            .with_context(|| format!("harness has no {ABI_SECTION} section"))?
            .data()
            .context("harness manifest is unreadable")
    }

    pub fn parse(json: &str) -> Result<Self> {
        let manifest: Manifest =
            serde_json::from_str(json).context("harness manifest is not valid JSON")?;
        if manifest.abi_version != ABI_VERSION {
            bail!(
                "harness was built against ABI version {}, this host speaks {ABI_VERSION}",
                manifest.abi_version
            );
        }
        Ok(manifest)
    }
}

/// A deployed harness, as the control API reports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Harness {
    pub name: String,
    /// sha256 of the ELF.
    pub digest: String,
    /// When to reach for this harness's tools, and how they go together.
    pub usage: String,
    pub tools: Vec<ToolSpec>,
    #[serde(default)]
    pub deps: Vec<String>,
    /// The subset of `deps` this service answers to nothing for.
    #[serde(default)]
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's arguments, as the model receives it.
    pub parameters: serde_json::Value,
}

/// What running a tool produced.
///
/// [`Self::Failed`] is the harness's own report, and it arrives with the same
/// `200` a result does: the call was fine and the tool said no. A refusal — no
/// such harness, no such tool, a trap — is a [`Failed`] with a status to match.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    Done(String),
    Failed(String),
}

impl From<Result<String, String>> for Output {
    fn from(outcome: Result<String, String>) -> Self {
        match outcome {
            Ok(result) => Self::Done(result),
            Err(failure) => Self::Failed(failure),
        }
    }
}

/// What the control API returns instead of a resource when it refuses one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failed {
    pub error: String,
}
