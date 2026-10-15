# berm-lang

Build a program for [Crabtalk](https://github.com/crabtalk/crabtalk).

A program is code the daemon schedules: one WebAssembly module, confined to its own memory, reaching the world only through the host calls it was given. This crate owns the ABI, so you never see a call number or a pointer pair.

```rust
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use berm_lang::Schema;

#[berm_lang::program]
mod tools {
    use crate::Echo;
    use alloc::string::String;
    use berm_lang::Result;

    /// Echo the query back.
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

Every `pub fn` is a tool and its doc comment is what the model reads when
deciding to call it. Its parameters are its arguments — their types, their doc
comments, and `Option` for the ones the model may omit — and the `Ok` of its
`Result` is its reply. `#[derive(Schema)]` gives a type its JSON Schema and its
serde impls, in any module or crate.

**A tool is internal unless marked `#[public]`.** Only the host calls an internal tool, for the program's own wakes. A `#[public]` tool can be called by any account the host authenticates and any program allowed to reach it, with arguments the caller chooses. `berm_lang::caller()` says who called, as the host set it; a tool that limits its callers checks it.

Off the guest's target this is an ordinary binary, which is why the `cfg_attr`
above is worth copying: tools then run under `cargo test` with no wasm
toolchain and no daemon. See `berm_lang::test`.

## Building

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

Leaving `[build] target` unset keeps the host build, and therefore `cargo test`, working without a flag.

The resulting `.wasm` is the whole artifact: one file, every platform.

## Reaching a runtime

This crate owns the ABI and declares no syscall of its own. What a
program can reach is whatever its host registered, so naming any of it here
would be the SDK deciding what a host must serve.

A program running under the Crabtalk daemon adds
[`berm-crabtalk`](https://crates.io/crates/berm-crabtalk), which declares the
`crabtalk` namespace — files, commands, HTTP, the runtime — and carries the
message types.

## Design

[RFC 0205 — Berm](https://crabtalk.github.io/crabtalk/rfcs/0205-berm.html).

## License

Apache-2.0
