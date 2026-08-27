# berm-system

The host half of the system harnesses a host running more than one harness
serves. [`berm`](https://crates.io/crates/berm) ships none of its own: every
`Harness` a guest can reach is one the embedder passed to `Berm::load`.

## `berm.call`

One harness reaching a tool on another. berm names it and serves it for nobody
— a `Berm` is one harness with nothing to dispatch to — so a host running more
than one is what registers it. How a name resolves is the argument.

```rust
use berm_system::call;

let system = vec![call::harness(
    call::DEFAULT_CALL_DEPTH,
    move |harness, tool, args| resolve(harness)?.call(tool, args),
)];
```

The depth bound is on runaway composition, not on the native stack: `0` refuses
the first nested call.

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

A harness only if it needs no policy invented to compile: resolution and
persistence arrive as arguments. A filesystem cannot be written without choosing
a root, so it is written by the host that chose one.

## License

Apache-2.0
