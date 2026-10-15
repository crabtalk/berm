# berm

[![CI](https://github.com/crabtalk/berm/actions/workflows/ci.yml/badge.svg)](https://github.com/crabtalk/berm/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/berm.svg)](https://crates.io/crates/berm)
[![Docs](https://img.shields.io/badge/docs-crabtalk.github.io-blue)](https://crabtalk.github.io/berm/)

The OS for agent harnesses.

Claude Code, Codex and OpenCode reach berm over MCP. What they install runs as
a process, under a syscall table that is readable before it runs and fixed once
it does.

A program is one WebAssembly module. berm pins it by hash, compiles it once, and
instantiates it per invocation — arguments go in through syscalls, the result
comes back out through one, and nothing survives the call.

```rust
let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
berm.deploy("example", &wasm)?;

// The outer result is the host's — a missing tool, a trap. The inner one is
// the program reporting failure, which is a result the model should see.
match berm.call(None, "example", "echo", br#"{"query":"hello"}"#.to_vec()).await? {
    Ok(result) => println!("{result}"),
    Err(failure) => eprintln!("{failure}"),
}
```

A program's public tools are reachable by name from anything deployed beside it that declares it. Beyond
that a program reaches the world only through the *syscalls* it was given, and
that table is the linker it is instantiated with — a call to anything else traps
because nothing is registered for it, not because a check said no. berm ships
none: what a filesystem is bounded by, and where bytes persist, are decisions
about a host, and berm has no host.

`Manifest::from_image(bytes)` reads what an image claims to be — its tools,
their schemas, when to reach for them, and what it will reach for itself —
without compiling or running it.

**A tool is internal unless marked `#[public]`.** Only the host calls an internal tool, for the program's own wakes and schedules. A `#[public]` tool can be called by any account the host authenticates and any program allowed to reach it, with arguments the caller chooses; `berm_lang::caller()` says who called. A deploy that makes a tool public is refused until the deployer confirms it.

## Running programs

`bermd` is a long-running service that deploys programs and serves every one of
them on a single MCP endpoint, with tools named `{program}.{tool}`.

```sh
bermd &
crab deploy
crab ls
```

`crab deploy` builds the program in the working directory and deploys it as its
`berm.toml` names it.

See [`crates/daemon`](crates/daemon) for the control API and what a deployed
program can reach.

## Moving programs

A program is one file, so it travels as one OCI layer with no tarball around it
— and because the layer is the image and nothing else, the digest a registry
addresses it by is the digest `crab ls` prints.

Against a host that pulls from a registry, `crab deploy` pushes the image to the
`image` its `berm.toml` names first. See
[Publishing a Program](https://crabtalk.github.io/berm/docs/berm/publishing/).

## Documentation

- **[Guide and design notes](https://crabtalk.github.io/berm/docs/)** — how it
  works and why, with worked examples.
- **API reference** — `cargo doc --open`.

## Building

```sh
cargo test --workspace
```

Rust 2024 edition, POSIX only. Building a program additionally needs the guest
target:

```sh
rustup target add wasm32-unknown-unknown
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
