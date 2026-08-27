# berm-system

System harnesses an embedder hands to [`berm`](https://crates.io/crates/berm) —
the ones whose behaviour is the harness model rather than a host's world.

berm serves `berm.call` itself, because resolving a name needs only the set of
deployed harnesses it already is. Everything else a guest reaches is a `System`
the embedder passed to `Berm::new`.

## `berm.get` and `berm.set`

A harness's own bytes, surviving its invocations — guest memory does not cross
one. Neither door takes a harness: the keyspace is whichever one is asking, read
off the `Callsite`, so a guest has no way to name another's keys.

```rust
use berm_system::store;

let system = store::harnesses(
    move |harness, key| read(harness, key),
    move |harness, key, value| write(harness, key, value),
);
```

`get` answers one field for a value and none for a key never written, so an
empty value and an absent one are told apart. There is no `delete`.

## What belongs here

A harness only if it needs no policy invented to compile: persistence arrives
as an argument. A filesystem cannot be written without choosing a root, so it is
written by the host that chose one.

## License

Apache-2.0
