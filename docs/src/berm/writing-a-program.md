# Writing a Program

`berm-lang` is the guest side. It owns the ABI, so an author never sees a call
number or a pointer pair.

```rust,ignore
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

#[berm_lang::program]
mod tools {
    use berm_lang::{Failed, Out};

    /// Echo the argument blob back inside a JSON envelope.
    #[args(Echo)]
    pub fn echo(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        out.write(br#"{"echo":"#);
        out.write(args);
        out.write(b"}");
        Ok(())
    }

    /// Arguments for `echo`.
    pub struct Echo {
        /// The text to echo back.
        pub query: &'static str,
        /// Page number, zero-indexed.
        pub page: Option<u32>,
    }
}
```

Every `pub fn` in the module is a tool. Its doc comment is the description the
model reads when deciding whether to call it, so write it for that reader.

## Arguments

The handler always receives the raw blob. Declaring a shape is about the
*schema* the model is given, not about how the bytes arrive:

- `#[args(Echo)]` names a struct declared beside the tool. Its fields and their
  doc comments become the JSON Schema, and `Option<T>` is what makes a field
  optional rather than required. A field may name another struct declared in
  the module; its schema is inlined.
- `#[params("…")]` carries a JSON Schema literal, for a shape no struct
  describes.
- Neither: the schema is an open object.

Parsing is the author's choice because not every program wants a JSON parser
linked into it — a tool taking no arguments should not pay for one.

## Results and failure

`#[returns(Peek)]` names a struct declared beside the tool, read the same way as
`#[args]`, and its schema goes into the manifest as `returns`. The struct gets a
`Serialize` derive; `berm_lang::tool::reply(&value, out)` writes it as the
result. An absent `Option` field is left out rather than written as `null`.

`Out` is a bounded sink over a caller-owned buffer, not an allocation, so a
program that never needs a heap never pays for one. Writes past the end are
dropped *and remembered*, which is what keeps a truncated payload from reaching
the model looking complete.

Returning `Err(Failed)` reports failure through the ABI. It is not a trap: the
host sees a tool that ran and failed, and hands the message back as a result.

## Building

`berm new` writes the crate, so none of the ceremony below has to be typed:

```sh
rustup target add wasm32-unknown-unknown
berm new my-program
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

`berm-fixture` in this repository is the worked example — the smallest real
program, and what berm's own tests run against.

## Queries

`#[query]` marks a tool read-only. While it runs, and in every call it makes,
`berm.set`, `berm.emit` and `berm.call.after` are refused, so it leaves nothing
for the host to commit. A host may serve it over `GET`.

## Events

An `#[event]` struct in the module is a typed log. `berm_lang::emit!(Noted {
word })` emits it under the topic `Noted`, its data the struct as JSON. The
manifest lists each event under `events` with its doc comment and the schema of
its fields.

## Calling another program

`berm idl hn` writes the IDL of the program deployed as `hn` to `idls/hn.json`.
`berm_lang::declare_program!(hn)` reads it from the crate's directory or the
nearest ancestor with an `idls/`, and generates:

- `hn::types`: every struct in its tools' arguments and results and its events.
- `hn::calls`: one function per tool, taking the argument struct (`&str` for a
  tool with none) and returning the result struct (`String` for a tool with
  none).
- `hn::events`: its events, and an `Event` enum whose `read` takes a delivered
  `Logged`.
- `hn::NAME`, which every call names, and `hn::DIGEST`, the image the IDL was
  read from.

The IDL is a file in the crate; it changes only when `berm idl` is run again.
