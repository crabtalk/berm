# berm

[![CI](https://github.com/crabtalk/berm/actions/workflows/ci.yml/badge.svg)](https://github.com/crabtalk/berm/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/berm.svg)](https://crates.io/crates/berm)
[![Docs](https://img.shields.io/badge/docs-crabtalk.github.io-blue)](https://crabtalk.github.io/berm/)

The OS for agent harnesses.

Claude Code, Codex and OpenCode reach berm over MCP. What they install runs as
a process, under a syscall table that is readable before it runs and fixed once
it does.

A program is one statically linked RV64 ELF. berm pins it by hash, compiles it
once, and instantiates it per invocation — arguments go in through syscalls, the
result comes back out of guest memory, and nothing survives the call.

```rust
let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
berm.deploy("example", &elf)?;

// The outer result is the host's — a missing tool, a trap. The inner one is
// the program reporting failure, which is a result the model should see.
match berm.call("example", "echo", br#"{"query":"hello"}"#.to_vec())? {
    Ok(result) => println!("{result}"),
    Err(failure) => eprintln!("{failure}"),
}
```

What is deployed is reachable by name from anything deployed beside it. Beyond
that a program reaches the world only through the *syscalls* it was given, and
that table is the linker it is instantiated with — a call to anything else traps
because nothing is registered for it, not because a check said no. berm ships
none: what a filesystem is bounded by, and where bytes persist, are decisions
about a host, and berm has no host.

`Manifest::from_elf(elf)` reads what an image claims to be — its tools, their
schemas, when to reach for them, and what it will reach for itself — without
compiling or running it.

## Running programs

`bermd` is a long-running service that deploys programs and serves every one of
them on a single MCP endpoint, with tools named `{program}.{tool}`.

```sh
bermd &
berm deploy example ./program.elf
berm ls
```

See [`apps/service`](apps/service) for the control API and what a deployed
program can reach.

## Moving programs

A program is one file, so it travels as one OCI layer with no tarball around it
— and because the layer is the ELF and nothing else, the digest a registry
addresses it by is the digest `berm ls` prints.

```sh
berm push ghcr.io/org/example:v1 ./program.elf
berm deploy example ghcr.io/org/example:v1
berm search "read a file"
```

`deploy` takes a file or a reference. Finding one is a separate question, since
no registry will tell you who published a program: the list is a git repository,
so `search` reads a clone of it with no service and no credential. See
[Publishing a Program](https://crabtalk.github.io/berm/book/berm/publishing.html).

## Documentation

- **[Guide and design notes](https://crabtalk.github.io/berm/book/)** — how it
  works and why, with worked examples.
- **API reference** — `cargo doc --open`.

## Building

```sh
cargo test --workspace
```

Rust 2024 edition, POSIX only. Building a program additionally needs the guest
target:

```sh
rustup target add riscv64imac-unknown-none-elf
```

Guests must be linked with **`--emit-relocs`**; that is the first thing to check
when one fails to load.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
