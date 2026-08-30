//! One compiled program, and the state of one invocation of it.

use crate::{
    Syscall, abi,
    backend::{Engine, Image},
    bound::depth,
    syscall,
};
use anyhow::{Context, Result, bail};
use berm_api::Manifest;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

/// One compiled program: an image, its tools, and the syscalls it was linked
/// against. Compilation is paid once; every invocation gets a fresh instance so
/// no guest state crosses between calls.
pub struct Program {
    /// What this program was deployed as, as every syscall reports it. An
    /// `Arc` because it is cloned into each invocation.
    pub name: Arc<str>,
    /// sha256 of the image. Redeploying different bytes under the same name is
    /// a different program, and this is what says so.
    pub digest: String,
    image: Image,
    /// Read from the image at load, without running anything.
    manifest: Manifest,
    /// Each tool beside the symbol it is exported as, resolved once so a call
    /// spends no allocation rebuilding the name.
    tools: BTreeMap<String, String>,
}

impl Program {
    /// Compile `image` and resolve its exports, giving it `syscalls`. The
    /// engine's code cache makes a second load of the same bytes cheap across
    /// processes as well as within one.
    pub(crate) fn load(
        engine: &Engine,
        image: &[u8],
        name: impl Into<Arc<str>>,
        syscalls: &[Syscall],
    ) -> Result<Self> {
        let compiled = Image::compile(engine, image, syscall::table(syscalls)?)
            .context("failed to compile program")?;

        let tools: BTreeMap<String, String> = compiled
            .exports()
            .into_iter()
            .filter_map(|export| {
                let tool = export.strip_prefix(abi::TOOL_PREFIX)?;
                Some((tool.to_owned(), export.to_owned()))
            })
            .collect();
        if tools.is_empty() {
            bail!("program exports no tools");
        }

        // A program that advertises a tool it does not export would fail at
        // dispatch, on a model's turn, as a missing symbol. The export table
        // and the manifest are both in hand here, so disagreement is caught
        // before the program is ever offered.
        let manifest = Manifest::from_image(image)?;
        for tool in &manifest.tools {
            if !tools.contains_key(&tool.name) {
                bail!(
                    "program manifest declares tool {:?}, which it does not export",
                    tool.name
                );
            }
        }

        Ok(Self {
            name: name.into(),
            digest: hex::encode(Sha256::digest(image)),
            image: compiled,
            manifest,
            tools,
        })
    }

    /// The tools this program exports, as its export table reports them.
    pub fn tools(&self) -> impl Iterator<Item = &str> {
        self.tools.keys().map(String::as_str)
    }

    /// What the program says it is: ABI version, tools, and usage.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Run one tool by name.
    ///
    /// The outer `Result` is the host's — a missing tool, a trap, a broken
    /// image. The inner one is the guest's: `Err` means the program reported
    /// failure, which is what a tool result carries back to the model.
    pub fn call(&self, tool: &str, args: impl Into<Vec<u8>>) -> Result<Result<String, String>> {
        let Some(symbol) = self.tools.get(tool) else {
            bail!("program exports no tool named {tool:?}");
        };

        // Counted before the invocation is built, so this guest's own depth is
        // what its syscalls are handed.
        let _level = depth::Level::enter();
        let invocation = Invocation {
            name: self.name.clone(),
            depth: depth::current(),
            args: args.into(),
            staged: Vec::new(),
            outcome: None,
        };

        let finished = self
            .image
            .call(symbol, invocation)
            .with_context(|| format!("program trapped in {tool}"))?;

        // A tool that returned without answering wrote nothing, which is an
        // empty result rather than a missing one.
        match finished.outcome {
            Some(Err(failure)) => Ok(Err(failure)),
            Some(Ok(result)) => Ok(Ok(
                String::from_utf8(result).context("program returned invalid UTF-8")?
            )),
            None => Ok(Ok(String::new())),
        }
    }
}

/// Guest state for one invocation. Memory is per-invocation; anything a
/// program needs to survive belongs in a storage program, not here.
pub struct Invocation {
    /// The program this invocation is of, and how deep it sits. Read once when
    /// the invocation is built, so a syscall costs no lookup.
    pub(crate) name: Arc<str>,
    pub(crate) depth: u32,
    pub(crate) args: Vec<u8>,
    /// The last syscall's reply, waiting for the guest to pull it. Staged
    /// rather than pushed because its size is not known until the work is
    /// done, and doing the work twice to measure it is not an option.
    pub(crate) staged: Vec<u8>,
    /// What the tool answered with, once it has. `Err` is the program's own
    /// reported failure, which is how a tool that failed is told apart from one
    /// that returned the word "error".
    pub(crate) outcome: Option<Result<Vec<u8>, String>>,
}

/// A syscall's answer when it refused the call and nothing ran.
///
/// Returned in an `Err` — on its own or as the source of a richer one — it
/// reaches the guest with [`abi::REFUSED`] set. Anything else is the other
/// kind of failure: whatever the syscall reached did run, and said no.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}
