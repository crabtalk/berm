# berm

The runtime of programs. A program is one hash-pinned WebAssembly module; berm
compiles it once, holds it by the name it answers to, and instantiates it per
invocation under wasmtime — nothing survives the call.

```rust
let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
berm.deploy("example", &wasm)?;
let result = berm.call(None, "example", "echo", br#"{"query":"hi"}"#.to_vec()).await?;
```

## What a guest can reach

berm serves the syscalls whose whole behaviour is the program model:
`berm.call`, which resolves a name against the set berm already holds, and
`berm.get` / `berm.set`, a program's own bytes surviving its invocations.
Neither storage door takes a program — the keyspace is whoever is asking, read
off the `Callsite` — so a guest has no way to name another's keys. Where those
bytes land is still a host's, and arrives as two closures.

Everything else is a `Syscall` the embedder registers, and that list is the
linker a program is instantiated with: a call to anything else traps because
nothing is registered for it, not because a check said no. The standard calls —
`berm.get`/`berm.set`, `berm.call.after`, `berm.http.fetch` — come from
`berm::syscall`'s constructors, which read and check the request and hand the
work to closures the embedder supplies. `berm.http.fetch` refuses a host the
program's manifest does not declare, and substitutes `{secret:NAME}` in header
values so the guest never holds a secret.

## What an invocation runs under

`bound/` holds what a guest cannot decline: how far a chain of programs may
nest, and how long one may run. Unlike a syscall, these are conditions
rather than something a guest reaches for.

`Manifest::from_image(bytes)` reads what an image claims to be without compiling or
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
