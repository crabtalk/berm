# berm

The runtime of harnesses. A harness is one hash-pinned RV64 ELF; berm compiles
it once, holds it by the name it answers to, and instantiates it per invocation
under [rvtime](https://crates.io/crates/rvtime) — nothing survives the call.

```rust
let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
berm.deploy("example", &elf)?;
let result = berm.call("example", "echo", br#"{"query":"hi"}"#.to_vec())?;
```

## What a guest can reach

berm serves the system harnesses whose whole behaviour is the harness model:
`berm.call`, which resolves a name against the set berm already holds, and
`berm.get` / `berm.set`, a harness's own bytes surviving its invocations.
Neither storage door takes a harness — the keyspace is whoever is asking, read
off the `Callsite` — so a guest has no way to name another's keys. Where those
bytes land is still a host's, and arrives as two closures.

Everything else is a `System` the embedder registers, and that list is the
linker a harness is instantiated with: a call to anything else traps because
nothing is registered for it, not because a check said no. berm ships no
filesystem, no command runner and no network — each needs a policy invented to
compile, and those are decisions about a host.

## What an invocation runs under

`bound/` holds what a guest cannot decline: how far a chain of harnesses may
nest, and how long one may run. Unlike a system harness, these are conditions
rather than something a guest reaches for.

`Manifest::from_elf(elf)` reads what an image claims to be without compiling or
running it.

```sh
cargo run --example measure    # prices a host call, an invocation, an allocation
```

Nothing here depends on a crabtalk crate, which is what lets the sandbox leave
that repository whenever it needs to.

## Design

[RFC 0205 — Berm](https://crabtalk.github.io/crabtalk/rfcs/0205-berm.html).

## License

Apache-2.0
