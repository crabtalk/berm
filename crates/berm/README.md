# berm

The runtime of harnesses. A harness is one hash-pinned RV64 ELF; berm compiles
it once, holds it by the name it answers to, and instantiates it per invocation
under [rvtime](https://crates.io/crates/rvtime) — nothing survives the call.

```rust
let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
berm.deploy("example", &elf)?;
let result = berm.call("example", "echo", br#"{"query":"hi"}"#.to_vec())?;
```

What is deployed is reachable by name from anything deployed beside it, which is
the one system harness berm serves itself. Everything else a harness reaches is
a `System` the embedder passed in, and that list is the linker it is
instantiated with — a call to anything else traps because nothing is registered
for it. berm ships none.

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
