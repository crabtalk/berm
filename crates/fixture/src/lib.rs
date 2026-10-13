//! The reference guest: the smallest real program, and what berm is measured
//! and tested against.
//!
//! Everything below the `#[program]` line is what an author actually writes.
//! The exports, the manifest section, the dispatch, and the panic handler come
//! from the SDK.
//!
//! Each tool prices or proves one thing, which is why they are not useful on
//! their own: `echo` reads its arguments and answers them back, `carry` takes
//! the same payload without reading it to price a parse, `chatty` makes a
//! hundred host calls to price one, `probe` allocates to show the heap arrives without
//! a second entry into the guest, `spin` loops past the instructions a call
//! includes, and `boom` fails on purpose.
//! `tick` arms itself, to show a wake outliving the invocation that asked for
//! it, `wait` makes one host call a host can make slow, and `fetch` makes one
//! HTTP request to a host its manifest declares. `note` writes and emits, and
//! `relay` calls `note` on `inner`, to show what a failed call takes with it.
//! `seen` takes the `Noted` events of whatever is deployed as `fixture`, read
//! through `declare_program!(fixture)`, the fixture's own IDL in `idls/`, and
//! `ask` calls `peek` and `echo` on `fixture` through it. `peek`
//! is a query: it reads `note`, tries to write, emit, arm a wake and call
//! `note` on `inner`, and answers what each was told.
//! `crates/berm/examples/measure.rs` and `load.rs` read the numbers off them,
//! and `tests/tools.rs` is the only exercise the SDK's host-side `test::call`
//! gets.

// `no_std` and `no_main` are the guest's shape. Off a guest target this is an
// ordinary library, so `cargo test` runs the tools below natively.
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use berm_lang::{Schema, event};

berm_lang::declare_program!(fixture);

// `inner` is what `nest`, `relay` and `recurse` reach for. It is not resolved
// here, only declared.
#[berm_lang::program(
    programs = ["inner"],
    syscalls = {
        "berm.get",
        "berm.set",
        "berm.call.after",
        "berm.http.fetch" = ["127.0.0.1"],
    },
    secrets = ["fixture"],
)]
mod tools {
    use crate::{Attempts, Echo, Fetched, Noted, Peek, Round, Wait};
    use alloc::{format, string::String, vec::Vec};
    use berm_lang::{Caller, Logged, Result};

    /// Answers its arguments back.
    #[public]
    pub fn echo(
        /// The text to echo back.
        query: String,
        /// Page number, zero-indexed.
        page: Option<u32>,
    ) -> Result<Echo> {
        Ok(Echo { query, page })
    }

    /// Takes its arguments without reading them, to price the parse `echo`
    /// makes of the same payload.
    #[public]
    pub fn carry() -> Result<()> {
        Ok(())
    }

    /// Loops `rounds` times, to run a call past the instructions it includes.
    #[public]
    pub fn spin(rounds: u64) -> Result<u64> {
        let mut acc = 0u64;
        for round in 0..rounds {
            acc = core::hint::black_box(acc.wrapping_mul(31).wrapping_add(round));
        }
        Ok(acc)
    }

    /// Makes 100 host calls, to price one.
    #[public]
    pub fn chatty() -> Result<()> {
        let mut total = 0;
        for _ in 0..100 {
            total += berm_lang::args_len();
        }
        if total == usize::MAX {
            return Err("impossible".into());
        }
        Ok(())
    }

    /// Allocates, to prove the heap arrives without a second entry.
    #[public]
    pub fn probe() -> Result<Vec<u8>> {
        let v = alloc::vec![7u8; 4096];
        Ok(v[..2].to_vec())
    }

    /// Reads one stored key and answers whether it was set. Under
    /// `examples/load.rs` the read stands in for a host call that waits on
    /// I/O.
    #[public]
    pub fn wait() -> Result<Wait> {
        Ok(match berm_lang::get("wait")? {
            Some(_) => Wait::Set,
            None => Wait::Unset,
        })
    }

    /// Fetches `url` and answers the status and the body's length.
    #[public]
    pub fn fetch(
        url: String,
        /// One `Name: Value` header, which may name `{secret:fixture}`.
        header: Option<String>,
    ) -> Result<Fetched> {
        let header = header.as_deref().and_then(|header| {
            header
                .split_once(':')
                .map(|(name, value)| (name.trim(), value.trim()))
        });
        let response = berm_lang::http::fetch("GET", &url, header.as_slice(), b"")?;
        Ok(Fetched {
            status: response.status,
            bytes: response.body.len() as u64,
        })
    }

    /// Says who called: `outside NAME`, `program NAME`, or `none`, then
    /// ` user ISSUER SUBJECT` when the chain runs for a user.
    #[public]
    pub fn whoami() -> Result<String> {
        let mut said = match berm_lang::caller()? {
            Some(Caller::Outside(name)) => format!("outside {name}"),
            Some(Caller::Program(name)) => format!("program {name}"),
            None => String::from("none"),
        };
        if let Some(user) = berm_lang::user()? {
            said.push_str(&format!(" user {} {}", user.issuer, user.subject));
        }
        Ok(said)
    }

    /// Answers `ok` to a program, and denies any other caller.
    #[public]
    pub fn guarded() -> Result<String> {
        match berm_lang::caller()? {
            Some(Caller::Program(_)) => Ok("ok".into()),
            _ => Err(berm_lang::deny("programs only")),
        }
    }

    /// Answers `ok`; only programs may call it.
    #[public]
    #[callers(services)]
    pub fn fromservices() -> Result<String> {
        Ok("ok".into())
    }

    /// Calls `guarded` and `fromservices` on `fixture`, and answers what they
    /// said.
    #[public]
    pub fn askguarded() -> Result<String> {
        use crate::fixture::calls;

        Ok(format!("{} {}", calls::guarded()?, calls::fromservices()?))
    }

    /// Calls `whoami` on `fixture`, and answers what it said.
    #[public]
    pub fn askwho() -> Result<String> {
        Ok(crate::fixture::calls::whoami()?)
    }

    /// Always fails, to exercise the error path.
    #[public]
    pub fn boom() -> Result<()> {
        Err("boom, as requested".into())
    }

    /// Calls `echo` on whatever is deployed as `inner`, to prove a program can
    /// reach a program. Says which kind of failure came back, because telling
    /// "it ran and said no" from "it was never there" is the point of the
    /// second bit on the wire.
    #[public]
    pub fn nest(query: String, page: Option<u32>) -> Result<Echo> {
        let args = berm_lang::json::to(&Echo { query, page })?;
        match berm_lang::call("inner", "echo", &args) {
            Ok(result) => Ok(berm_lang::json::from(&result)?),
            Err(error) if error.refused() => Err(format!("refused: {}", error.message()).into()),
            Err(error) => Err(error.into()),
        }
    }

    /// Counts how many times it has run, arming itself to run again in `delay`
    /// milliseconds until it has run three times. Proves an invocation can
    /// outlive the one that asked for it, and that what crosses the gap is the
    /// program's own keys.
    pub fn tick(delay: u64) -> Result<Round> {
        let round = berm_lang::get("ticks")
            .ok()
            .flatten()
            .and_then(|seen| core::str::from_utf8(&seen).ok()?.parse::<u64>().ok())
            .unwrap_or(0)
            + 1;
        berm_lang::set("ticks", format!("{round}").as_bytes())?;

        // The delay rides along, so each round arms the next on the same one.
        if round < 3 {
            berm_lang::after(delay, "fixture", "tick", &format!(r#"{{"delay":{delay}}}"#))?;
        }
        Ok(Round {
            round,
            at: berm_lang::now(),
        })
    }

    /// Writes `word` under `note`, emits it as a `Noted` event, answers what it
    /// reads back, and fails when `word` is `fail`.
    #[public]
    pub fn note(word: String) -> Result<String> {
        berm_lang::set("note", word.as_bytes())?;
        berm_lang::emit!(Noted {
            bytes: word.len() as u64,
            word: word.clone(),
        })?;
        if word == "fail" {
            return Err("failed, as requested".into());
        }
        let seen = berm_lang::get("note")?.ok_or("note was not kept")?;
        Ok(String::from_utf8_lossy(&seen).into_owned())
    }

    /// Calls `note` on `inner` with `word`, carries on whether or not that
    /// failed, emits `word` under the topic `relay`, and answers what this
    /// program reads back under `note`.
    #[public]
    pub fn relay(word: String) -> Result<String> {
        let args = format!(r#"{{"word":{}}}"#, berm_lang::json::to(&word)?);
        let _ = berm_lang::call("inner", "note", &args);
        berm_lang::emit(&["relay"], &word)?;
        Ok(match berm_lang::get("note")? {
            Some(seen) => String::from_utf8_lossy(&seen).into_owned(),
            None => String::from("unset"),
        })
    }

    /// Takes the `Noted` events of `fixture` and keeps the last one's `seq`
    /// and word under `seen`, answering them.
    #[logs("fixture" = ["Noted"])]
    pub fn seen(program: String, logs: Vec<Logged>) -> Result<String> {
        use crate::fixture::events::Event;

        if program != "fixture" {
            return Err(format!("logs of {program}, not fixture").into());
        }
        let Some(last) = logs.last() else {
            return Ok(String::new());
        };
        let Some(Ok(Event::Noted(noted))) = Event::read(last) else {
            return Err("the last log is not a Noted event".into());
        };
        let said = format!("{} {}", last.seq, noted.word);
        berm_lang::set("seen", said.as_bytes())?;
        Ok(said)
    }

    /// Calls `peek` on `fixture`, then `echo` on `fixture` with the note it
    /// read, and answers the query `echo` answered.
    #[public]
    pub fn ask() -> Result<String> {
        use crate::fixture::calls;

        let peek = calls::peek()?;
        let echoed = calls::echo(None, peek.note.unwrap_or_default())?;
        Ok(echoed.query)
    }

    /// Reads `note`, then tries to write it, emit under `note`, arm `tick` and
    /// call `note` on `inner`, and answers what it read and what each attempt
    /// was told.
    #[query]
    #[public]
    pub fn peek() -> Result<Peek> {
        let told = |attempt: core::result::Result<(), berm_lang::CallError>| match attempt {
            Ok(()) => String::from("allowed"),
            Err(error) => error.message().into(),
        };
        let note = berm_lang::get("note")
            .ok()
            .flatten()
            .map(|value| String::from_utf8_lossy(&value).into_owned());
        Ok(Peek {
            note,
            attempts: Attempts {
                set: told(berm_lang::set("note", b"peeked")),
                emit: told(berm_lang::emit(&["note"], "peeked")),
                after: told(berm_lang::after(0, "fixture", "tick", r#"{"delay":0}"#)),
                call: told(berm_lang::call("inner", "note", r#"{"word":"peeked"}"#).map(|_| ())),
            },
        })
    }

    /// Calls itself on `inner` one level deeper, which is this same program
    /// when deployed under that name — the runaway a depth limit exists to
    /// stop. Answers how many levels got through before the host refused.
    #[public]
    pub fn recurse(depth: u32) -> Result<u32> {
        let deeper = depth + 1;
        match berm_lang::call("inner", "recurse", &format!(r#"{{"depth":{deeper}}}"#)) {
            Ok(reached) => Ok(berm_lang::json::from(&reached)?),
            // The bottom of the chain: the host refused to go further, and the
            // depth reached rides back up as the result.
            Err(error) if error.refused() => Ok(deeper),
            Err(error) => Err(error.into()),
        }
    }
}

/// What `echo` answers.
#[derive(Schema)]
pub struct Echo {
    pub query: String,
    pub page: Option<u32>,
}

/// What `wait` answers.
#[derive(Schema)]
pub enum Wait {
    Set,
    Unset,
}

/// What `fetch` answers.
#[derive(Schema)]
pub struct Fetched {
    pub status: u16,
    /// The body's length.
    pub bytes: u64,
}

/// What `tick` answers.
#[derive(Schema)]
pub struct Round {
    /// How many times it has run, this time included.
    pub round: u64,
    /// When, in Unix milliseconds.
    pub at: u64,
}

/// A word `note` wrote.
#[event]
pub struct Noted {
    pub word: String,
    /// Its length in bytes.
    pub bytes: u64,
}

/// What `peek` answers.
#[derive(Schema)]
pub struct Peek {
    /// What `note` held, if anything.
    pub note: Option<String>,
    pub attempts: Attempts,
}

/// What each of `peek`'s attempts was told.
#[derive(Schema)]
pub struct Attempts {
    pub set: String,
    pub emit: String,
    pub after: String,
    pub call: String,
}
