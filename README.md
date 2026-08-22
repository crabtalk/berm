# berm

[![CI](https://github.com/crabtalk/berm/actions/workflows/ci.yml/badge.svg)](https://github.com/crabtalk/berm/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-crabtalk.github.io-blue)](https://crabtalk.github.io/berm/)

A sandbox for harnesses.

A harness is one statically linked RV64 ELF. berm pins it by hash, compiles it
once, and instantiates it per invocation — arguments go in through host calls,
the result comes back out of guest memory, and nothing survives the call.

```rust
let berm = Berm::load(&engine, &elf, &[])?;

// The outer result is the host's — a missing tool, a trap. The inner one is
// the harness reporting failure, which is a result the model should see.
match berm.call("echo", br#"{"query":"hello"}"#.to_vec())? {
    Ok(result) => println!("{result}"),
    Err(failure) => eprintln!("{failure}"),
}
```

A harness reaches the world only through *system harnesses* it was given, and
the grant is the `Linker` it is instantiated with — an ungranted call traps
because nothing is registered for it, not because a check said no. berm ships
none: what a filesystem is bounded by, and where bytes persist, are decisions
about a host, and berm has no host.

`Manifest::from_elf(elf)` reads what an image claims to be — its tools, their
schemas, when to reach for them — without compiling or running it.

## rvtime

[rvtime](rvtime/README.md) is what compiles and confines the guest, and it ships
from this repository too. It has no idea what a harness is: it loads an ELF,
generates native code for it, and calls it. Every convention that makes a guest
a *harness* — tools, a manifest, an argument blob — lives on the berm side,
which is what leaves rvtime usable for a guest that is not one.

## Running harnesses

`bermd` is a long-running service that deploys harnesses and serves every one of
them on a single MCP endpoint, with tools named `{harness}.{tool}`.

```sh
bermd &
berm deploy example ./harness.elf
berm ls
```

See [`apps/service`](apps/service) for the control API and what a deployed
harness can reach.

## Documentation

- **[Guide and design notes](https://crabtalk.github.io/berm/)** — berm first,
  then rvtime.
- **[API reference](https://crabtalk.github.io/berm/api/)** — generated from the
  source.

## Building

```sh
cargo test --workspace
```

Rust 2024 edition, POSIX only. Building a harness additionally needs the guest
target:

```sh
rustup target add riscv64imac-unknown-none-elf
```

Guests must be linked with **`--emit-relocs`**; that is the first thing to check
when one fails to load.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
