//! One compiled program, and the state of one invocation of it.

use crate::{
    Caller, Callsite, Deps, Syscall, User, abi,
    bound::watchdog,
    engine::{Engine, Image},
    journal::{Changes, Journal},
    syscall::{self, call::Enter},
};
use anyhow::{Context, Result, bail};
use berm_api::{Callers, Manifest};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

/// The last [`Callsite::chain`] handed out.
static CHAINS: AtomicU64 = AtomicU64::new(0);

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
    /// The manifest's deps, as every invocation's [`Callsite`] carries them.
    deps: Arc<Deps>,
    /// Each tool beside the symbol it is exported as, resolved once so a call
    /// spends no allocation rebuilding the name, and whether it is a query.
    tools: BTreeMap<String, Tool>,
    /// What each invocation runs inside.
    enter: Enter,
}

struct Tool {
    symbol: String,
    query: bool,
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
        enter: Enter,
    ) -> Result<Self> {
        let compiled = Image::compile(engine, image, syscall::table(syscalls)?)
            .context("failed to compile program")?;

        let manifest = Manifest::from_image(image)?;
        let tools: BTreeMap<String, Tool> = compiled
            .exports()
            .into_iter()
            .filter_map(|export| {
                let name = export.strip_prefix(abi::TOOL_PREFIX)?;
                let query = manifest
                    .tools
                    .iter()
                    .any(|spec| spec.name == name && spec.query);
                Some((
                    name.to_owned(),
                    Tool {
                        symbol: export.to_owned(),
                        query,
                    },
                ))
            })
            .collect();
        if tools.is_empty() {
            bail!("program exports no tools");
        }

        // A program that advertises a tool it does not export would fail at
        // dispatch, on a model's turn, as a missing symbol. The export table
        // and the manifest are both in hand here, so disagreement is caught
        // before the program is ever offered.
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
            deps: Arc::new(manifest.deps.clone()),
            manifest,
            tools,
            enter,
        })
    }

    /// The tools this program exports, as its export table reports them.
    pub fn tools(&self) -> impl Iterator<Item = &str> {
        self.tools.keys().map(String::as_str)
    }

    /// Whether `tool` is one any caller may reach.
    pub fn public(&self, tool: &str) -> bool {
        self.manifest
            .tools
            .iter()
            .any(|spec| spec.name == tool && spec.public)
    }

    /// Whether `tool` takes calls from other programs, as its manifest's
    /// `callers` says.
    pub fn reached_by_programs(&self, tool: &str) -> bool {
        self.manifest
            .tools
            .iter()
            .any(|spec| spec.name == tool && spec.callers != Some(Callers::Accounts))
    }

    /// Whether `tool` takes calls from outside, as its manifest's `callers`
    /// says.
    pub fn reached_by_accounts(&self, tool: &str) -> bool {
        self.manifest
            .tools
            .iter()
            .any(|spec| spec.name == tool && spec.callers != Some(Callers::Services))
    }

    /// What the program says it is: ABI version, tools, and usage.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Run one tool by name, as the first link of a chain.
    ///
    /// `Err` is the host's — a missing tool, a trap, a broken image. The
    /// program's own reported failure is [`Outcome::result`].
    ///
    /// `caller` is who is calling, as the embedder names them; a syscall reads
    /// it off its [`Callsite`] as [`Caller::Outside`]. Any tool may be called
    /// this way: which tools an outside caller reaches is the embedder's to
    /// decide.
    pub async fn call(
        &self,
        caller: Option<Arc<str>>,
        tool: &str,
        args: impl Into<Vec<u8>>,
    ) -> Result<Outcome> {
        self.call_as(caller, None, tool, args).await
    }

    /// [`Program::call`] on behalf of `user`, which every call in the chain
    /// reads off its [`Callsite`].
    pub async fn call_as(
        &self,
        caller: Option<Arc<str>>,
        user: Option<User>,
        tool: &str,
        args: impl Into<Vec<u8>>,
    ) -> Result<Outcome> {
        let journal = Arc::new(Journal::default());
        let Ended { result, denied } = self
            .invoke(
                tool,
                args.into(),
                caller.map(Caller::Outside),
                user,
                1,
                false,
                watchdog::fresh(),
                CHAINS.fetch_add(1, Ordering::Relaxed) + 1,
                journal.clone(),
                Arc::from([self.name.clone()]),
            )
            .await?;
        let changes = match result {
            Ok(_) => journal.take(),
            Err(_) => Changes::default(),
        };
        Ok(Outcome {
            result,
            denied,
            changes,
        })
    }

    /// Run one tool as link `depth` of chain `chain`, called by `caller`, that
    /// has to be done by `deadline`, under the programs in `stack`. `query` is
    /// whether a link above it is a query. What it keeps lands in `journal`,
    /// and a failure takes it back out.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn invoke(
        &self,
        tool: &str,
        args: Vec<u8>,
        caller: Option<Caller>,
        user: Option<User>,
        depth: u32,
        query: bool,
        deadline: Instant,
        chain: u64,
        journal: Arc<Journal>,
        stack: Arc<[Arc<str>]>,
    ) -> Result<Ended> {
        let Some(Tool {
            symbol,
            query: declared,
        }) = self.tools.get(tool)
        else {
            bail!("program exports no tool named {tool:?}");
        };

        let mut at = Callsite {
            program: self.name.clone(),
            deps: self.deps.clone(),
            caller,
            user,
            depth,
            stack,
            query: query || *declared,
            deadline,
            chain,
            journal: journal.clone(),
        };
        let scope = (self.enter)(self, &at).await?;
        let own = scope.own();
        if own {
            at.journal = Arc::new(Journal::default());
        }
        let held = at.journal.clone();
        let mark = journal.mark();
        let invocation = Invocation {
            at,
            args,
            staged: Vec::new(),
            outcome: None,
            denied: false,
        };

        let ended = async {
            let finished = self
                .image
                .call(symbol, invocation)
                .await
                .with_context(|| format!("program trapped in {tool}"))?;

            // A tool that returned without answering wrote nothing, which is
            // an empty result rather than a missing one.
            let result = match finished.outcome {
                Some(Err(failure)) => Err(failure),
                Some(Ok(result)) => {
                    Ok(String::from_utf8(result).context("program returned invalid UTF-8")?)
                }
                None => Ok(String::new()),
            };
            Ok(Ended {
                denied: finished.denied && result.is_err(),
                result,
            })
        }
        .await;
        match &ended {
            Ok(Ended { result: Ok(_), .. }) => {
                let changes = match own {
                    true => held.take(),
                    false => Changes::default(),
                };
                let kept = scope
                    .close(changes)
                    .await
                    .and_then(|left| journal.extend(left));
                if let Err(error) = kept {
                    journal.revert(mark);
                    return Err(error);
                }
            }
            _ => journal.revert(mark),
        }
        ended
    }
}

/// How one invocation ended.
pub(crate) struct Ended {
    pub result: Result<String, String>,
    /// The program failed the call through `berm.deny`.
    pub denied: bool,
}

/// What one chain answered with, and what it left for the host to commit.
#[derive(Debug)]
pub struct Outcome {
    /// `Err` is the program's own reported failure, which is a tool result
    /// rather than an error.
    pub result: Result<String, String>,
    /// `result` is `Err` because the program refused its caller, through
    /// `berm.deny`.
    pub denied: bool,
    /// Empty when `result` is `Err`.
    pub changes: Changes,
}

/// Guest state for one invocation. Memory is per-invocation; anything a
/// program needs to survive belongs in a storage program, not here.
pub struct Invocation {
    /// The program this invocation is of, and the chain it sits in. Built
    /// once with the invocation, so a syscall costs no lookup.
    pub(crate) at: Callsite,
    pub(crate) args: Vec<u8>,
    /// The last syscall's reply, waiting for the guest to pull it. Staged
    /// rather than pushed because its size is not known until the work is
    /// done, and doing the work twice to measure it is not an option.
    pub(crate) staged: Vec<u8>,
    /// What the tool answered with, once it has. `Err` is the program's own
    /// reported failure, which is how a tool that failed is told apart from one
    /// that returned the word "error".
    pub(crate) outcome: Option<Result<Vec<u8>, String>>,
    /// Set by `berm.deny`.
    pub(crate) denied: bool,
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
