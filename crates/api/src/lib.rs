//! What a program says it is, and what bermd's control API speaks.
//!
//! Here rather than in the service so a client can read an image and talk to
//! the API without linking a compiler.

use anyhow::{Context, Result, bail};
use object::{Object, ObjectSection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use compat::{Break, Surface};

mod compat;
pub mod host;

/// The ABI this host speaks. A program built against a different one is
/// refused rather than dispatched into a syscall its author did not
/// mean.
pub const ABI_VERSION: u32 = 4;

/// The section carrying the manifest. A section rather than an export, so
/// reading what a program claims to be never means running it.
///
/// A wasm custom section, which is what `#[link_section]` emits on
/// `wasm32`.
pub const ABI_SECTION: &str = ".berm.abi";

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub abi_version: u32,
    /// The crate's version when it was built.
    #[serde(default)]
    pub version: String,
    pub tools: Vec<ToolSpec>,
    /// When to reach for these tools, and how they go together — the
    /// question no single tool's `description` answers, because it is about
    /// choosing between them. An embedder puts this in front of a model
    /// before it decides, so it is paid on every turn: a few lines, not a
    /// manual.
    #[serde(default)]
    pub usage: String,
    /// What this program reaches for once it runs.
    ///
    /// Runtime, and resolved wherever the image lands — nothing here is
    /// fetched or installed, and a name nothing answers to is reported rather
    /// than refused. Declared by the author because a target computed at the
    /// call cannot be read off the image at all.
    #[serde(default)]
    pub deps: Deps,
    /// The typed logs it emits.
    #[serde(default)]
    pub events: Vec<EventSpec>,
    /// The SQLite database it keeps, when it keeps one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<Database>,
}

/// A program's own SQLite database: the migrations that build it, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Database {
    pub migrations: Vec<Migration>,
}

/// One migration. A host applies each once, in order, and refuses a program
/// whose migrations no longer begin with the ones it applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Migration {
    pub name: String,
    pub sql: String,
}

/// One typed log a program emits, from an `#[event]` struct. Emitted under the
/// topic `name`, its data the JSON `fields` describes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the log's data.
    pub fields: serde_json::Value,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

/// What a program reaches for once it runs, by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deps {
    /// Programs it calls by name.
    #[serde(default)]
    pub programs: Vec<String>,
    /// Syscalls it makes beyond the ABI's own, each beside what it reaches
    /// through that call — the hosts a fetch goes to, the names a secret is
    /// read by. An empty list is the call alone.
    #[serde(default)]
    pub syscalls: BTreeMap<String, Vec<String>>,
    /// Secrets it names in `{secret:NAME}`, which the host substitutes and
    /// the guest never holds.
    #[serde(default)]
    pub secrets: Vec<String>,
    /// Programs whose logs it reads, and the tool each is delivered to.
    #[serde(default)]
    pub logs: Vec<Subscription>,
}

impl Deps {
    pub fn is_empty(&self) -> bool {
        self.programs.is_empty()
            && self.syscalls.is_empty()
            && self.secrets.is_empty()
            && self.logs.is_empty()
    }
}

/// One program's logs, delivered to one tool as a [`Delivery`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub program: String,
    /// A log is delivered when it carries any of these. Empty takes every log.
    #[serde(default)]
    pub topics: Vec<String>,
    pub tool: String,
}

impl Subscription {
    /// Whether a log carrying `topics` is delivered.
    pub fn matches(&self, topics: &[String]) -> bool {
        self.topics.is_empty() || topics.iter().any(|topic| self.topics.contains(topic))
    }
}

/// What a subscribed tool is called with: logs of one program, oldest first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub program: String,
    pub logs: Vec<Logged>,
}

/// One log as delivered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logged {
    /// Its position in the program's logs, counting from 1. The host assigns
    /// it at commit.
    pub seq: u64,
    pub topics: Vec<String>,
    pub data: String,
}

/// `inner, berm.http.fetch (example.com), secret openai, logs hn (item) to
/// ingest` — every program, every syscall with what it reaches, every secret,
/// then every subscription.
impl std::fmt::Display for Deps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut items = self.programs.clone();
        items.extend(
            self.syscalls
                .iter()
                .map(|(name, reached)| match reached.is_empty() {
                    true => name.clone(),
                    false => format!("{name} ({})", reached.join(", ")),
                }),
        );
        items.extend(self.secrets.iter().map(|name| format!("secret {name}")));
        items.extend(self.logs.iter().map(|logs| match logs.topics.is_empty() {
            true => format!("logs {} to {}", logs.program, logs.tool),
            false => format!(
                "logs {} ({}) to {}",
                logs.program,
                logs.topics.join(", "),
                logs.tool
            ),
        }));
        f.write_str(&items.join(", "))
    }
}

impl Manifest {
    /// Read what an image claims to be, without compiling or running it.
    ///
    /// This is what the section is *for* (RFC 0205): learning a program's tools,
    /// deps, and usage must not mean instantiating it. An embedder assembling a
    /// prompt or listing a registry needs exactly this and nothing else.
    pub fn from_image(image: &[u8]) -> Result<Self> {
        let json =
            str::from_utf8(Self::section(image)?).context("program manifest is not UTF-8")?;
        Self::parse(json)
    }

    /// The section's bytes as they sit in the image. A publisher carries these
    /// verbatim, so what a registry serves cannot disagree with what the image
    /// holds.
    pub fn section(image: &[u8]) -> Result<&[u8]> {
        let file = object::File::parse(image).context("program is not a readable image")?;
        file.section_by_name(ABI_SECTION)
            .with_context(|| format!("program has no {ABI_SECTION} section"))?
            .data()
            .context("program manifest is unreadable")
    }

    pub fn parse(json: &str) -> Result<Self> {
        let manifest: Manifest =
            serde_json::from_str(json).context("program manifest is not valid JSON")?;
        if manifest.abi_version != ABI_VERSION {
            bail!(
                "program was built against ABI version {}, this host speaks {ABI_VERSION}",
                manifest.abi_version
            );
        }
        Ok(manifest)
    }
}

/// A deployed program, as the control API reports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub name: String,
    /// The crate's version when it was built.
    #[serde(default)]
    pub version: String,
    /// sha256 of the image.
    pub digest: String,
    /// When to reach for this program's tools, and how they go together.
    pub usage: String,
    pub tools: Vec<ToolSpec>,
    #[serde(default)]
    pub deps: Deps,
    #[serde(default)]
    pub events: Vec<EventSpec>,
    /// The part of `deps` this service answers to nothing for.
    #[serde(default, skip_serializing_if = "Deps::is_empty")]
    pub unresolved: Deps,
    /// How this deploy broke callers of the program it replaced, as
    /// [`Manifest::breaks`] words it. Set only in a deploy's answer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<String>,
}

/// What a service's owner set for calling it, as the host answering holds it.
/// Changes live, from the next call.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Terms {
    /// Each public tool's, by name.
    pub tools: BTreeMap<String, ToolTerms>,
}

/// What one call of a tool is held to, and what it costs.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolTerms {
    pub budget: Budget,
    /// USD micros a call that ends `done` costs its caller; 0 is free.
    #[serde(default)]
    pub price_micros: u64,
}

/// A tool's price, as its owner sets it.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    /// USD micros a call that ends `done` costs its caller; 0 is free.
    pub price_micros: u64,
}

/// The most one invocation may use: wasm instructions, and rows read and
/// written together.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub instructions: u64,
    pub rows: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's arguments, as the model receives it.
    pub parameters: serde_json::Value,
    /// JSON Schema for the tool's result. `None` declares no shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<serde_json::Value>,
    /// Read-only: a call to it, and every call it makes, is refused
    /// `berm.set`, `berm.emit` and `berm.call.after`, so it commits nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub query: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
    /// Callable by accounts and other programs. A tool that is not is only
    /// called by its host, for the program's own subscriptions and wakes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub public: bool,
    /// Which callers a public tool takes. `None` takes both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callers: Option<Callers>,
}

/// One kind of caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Callers {
    /// Programs reaching it through `berm.call`.
    Services,
    /// Callers from outside, as the host authenticates them.
    Accounts,
}

/// What running a tool produced.
///
/// [`Self::Failed`] is the program's own report, and it arrives with the same
/// `200` a result does: the call was fine and the tool said no. A refusal — no
/// such program, no such tool, a trap — is a [`Failed`] with a status to match.
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

impl From<Output> for Result<String, String> {
    fn from(output: Output) -> Self {
        match output {
            Output::Done(result) => Ok(result),
            Output::Failed(failure) => Err(failure),
        }
    }
}

/// What a call used of one program: its invocations, the wasm instructions
/// they executed, the largest linear memory one reached, in bytes, the rows
/// they read and wrote, and the bytes sent and received through the proxy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meter {
    pub calls: u64,
    pub instructions: u64,
    pub memory: u64,
    pub read: u64,
    pub written: u64,
    pub proxied: u64,
}

impl std::ops::AddAssign for Meter {
    /// Sums every count but `memory`, which keeps the larger.
    fn add_assign(&mut self, other: Meter) {
        self.calls += other.calls;
        self.instructions += other.instructions;
        self.memory = self.memory.max(other.memory);
        self.read += other.read;
        self.written += other.written;
        self.proxied += other.proxied;
    }
}

/// What the control API returns instead of a resource when it refuses one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failed {
    pub error: String,
}
