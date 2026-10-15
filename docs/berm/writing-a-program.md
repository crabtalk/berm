---
title: Writing a Program
---

`berm-lang` is the guest side. It owns the ABI, so an author never sees a call
number or a pointer pair.

```rust,ignore
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use berm_lang::Schema;

#[berm_lang::program]
mod tools {
    use crate::Echo;
    use alloc::string::String;
    use berm_lang::Result;

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
}

/// What `echo` answers.
#[derive(Schema)]
pub struct Echo {
    pub query: String,
    pub page: Option<u32>,
}
```

Every `pub fn` in the module is a tool. Its doc comment is the description the
model reads when deciding whether to call it, so write it for that reader.

## Arguments

A tool's parameters are the properties of its arguments, a JSON object. A
parameter's doc comment is its description, and an `Option` parameter may be
omitted. A tool with no parameters takes any arguments and reads none.

Each parameter's type implements `Schema`: strings, numbers, `bool`, `Vec`,
`Option`, `BTreeMap<String, T>`, and any type with `#[derive(Schema)]`, in any
module or crate. The derive takes a struct with named fields — an object titled
by its name, its fields' doc comments their descriptions — or an enum of unit
variants, a string that is one of them in `snake_case`. A `Schema` type owns
its data, and the derive gives it serde's `Serialize` and `Deserialize`.

## Results and failure

A tool returns `berm_lang::Result<T>`. `T` is a `Schema` type, written as the
JSON result and declared in the manifest as `returns`; `()` writes nothing. An
absent `Option` field is left out rather than written as `null`.

`Err(berm_lang::Error)` reports failure through the ABI, its `message` the
caller's to read. It is not a trap: the host sees a tool that ran and failed. A
syscall's `CallError` and a `String` convert into it, so `?` reports either. An
enum marked `#[error_code]`, each variant with `#[msg("…")]`, converts into it
with that message. `berm_lang::deny(message)` fails the call as denied.

## Building

`crab new` writes the crate, so none of the ceremony below has to be typed:

```sh
rustup target add wasm32-unknown-unknown
crab new my-program
cd my-program
cargo build --release --target wasm32-unknown-unknown
```

The tools live in `src/lib.rs` under a three-line
`src/bin/main.rs` that does nothing but `extern crate` them, because cargo emits
an image for a bin target and an archive for a lib — the library alone never
gets linked, and the binary alone cannot be reached from `tests/`.

Off a guest target the crate is an ordinary library, so tools can be unit tested
natively instead of cross-compiling to run anything at all. `berm_lang::test`
stands in for the host: it holds the argument blob and collects what a program
logged. A call reaching a syscall that has no stand-in panics naming it,
rather than reading a plausible zero.

`berm_host::test::TestHost`, behind `berm-host`'s `test` feature, runs the built
image instead: the real engine and host, with fetches answered from what the
test arranged, a clock the test sets, and wakes the test fires. A call's result
and meter, the logs and state it committed, and the wakes it armed are read back
from it. The services in [crabtalk/crab](https://github.com/crabtalk/crab) run
against recorded feeds this way.

`berm-fixture` in this repository is the worked example — the smallest real
program, and what berm's own tests run against.

## Queries

`#[query]` marks a tool read-only. While it runs, and in every call it makes,
`berm.set`, `berm.emit` and `berm.call.after` are refused, so it leaves nothing
for the host to commit. A host may serve it over `GET`.

## Crons

`#[cron]` marks a tool its host calls each time its cron expression fires,
whatever the last call ended as, and not while its owner is held. A cron tool
takes no parameters and is not `#[public]`. Its expression is five fields,
minute to day of week, read in UTC: `*/5 * * * *`. The deploy sets it, from
`[crons]` in `berm.toml`, or its owner does while it runs; until one is set it
is not called.

## Events

An `#[event]` struct anywhere in the program's crate is a typed log, and a
`Schema` type. `berm_lang::emit!(Noted { word })` emits it under the topic
`Noted`, its data the struct as JSON. The manifest lists each event under
`events` with its doc comment and the schema of its fields.

Logs are what a program's developer reads to debug it: the host keeps each one beside the call it was emitted in, whatever that call ended as, for 30 days. A log holds at most 4 KiB, its data and topics together, and is refused past that. A program's data goes in its database, read by other programs through its query tools.

## Calling another program

`crab idl hn` writes the IDL of the program deployed as `hn` to `idls/hn.json`.
`berm_lang::declare_program!(hn)` reads it from the crate's directory or the
nearest ancestor with an `idls/`, and generates:

- `hn::types`: every titled object and string enum in its tools' arguments and
  results.
- `hn::calls`: one function per tool, taking its arguments in the IDL's key
  order, an optional one as an `Option`, and returning its result (`()` for a
  tool with none).
- `hn::NAME`, which every call names.

A tool the IDL marks `deprecated` is generated with `#[deprecated]`.

## Changing an interface

A tool's callers were built against its IDL. A change that breaks them — see
[the manifest](manifest.md#compatibility) — keeps the old tool beside the new
one, marked `#[deprecated]`, until no caller uses it.

The IDL is a file in the crate; it changes only when `crab idl` is run again.
