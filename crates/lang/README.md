# berm-lang

Build a program for [Crabtalk](https://github.com/crabtalk/crabtalk).

A program is code the daemon schedules: one WebAssembly module, confined to its own memory, reaching the world only through the host calls it was given. This crate owns the ABI, so you never see a call number or a pointer pair.

```rust
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

#[berm_lang::program]
mod tools {
    use berm_lang::{Failed, Out};

    /// Echo the query back.
    #[args(Echo)]
    pub fn echo(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        out.write(args);
        Ok(())
    }

    /// Arguments for `echo`.
    pub struct Echo {
        /// The text to echo back.
        pub query: String,
        /// Page number, zero-indexed.
        pub page: Option<u32>,
    }
}
```

Every `pub fn` is a tool and its doc comment is what the model reads when
deciding to call it. `#[args(Echo)]` derives the JSON Schema from that struct's
fields — their types, their doc comments, and `Option` for the ones the model
may omit. Nothing is deserialized for you, so a program that wants no JSON
parser links none.

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
