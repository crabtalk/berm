# Host Calls

## Mechanism, not policy

rvtime ships no standard host functions, and this is the decision most likely to
look like something missing.

wasmtime does the same: WASI is a separate crate, not part of the core runtime.
The reason is not fastidiousness. **The host-function set is policy** — it
encodes what a guest is permitted to do. Bake one into the runtime and every
embedder either inherits that policy or fights it.

For a plugin host it is worse than that: capability control *is* the reason to
use an in-process sandbox rather than a subprocess. A plugin can do only what
you registered. Move that surface into the runtime and you have given away the
thing that made it worth building.

So the guest crate knows how to make a call and never which calls exist, and the
embedder defines the interface.

## The mechanism

A guest executes `ecall` with the number in `a7` and arguments in `a0` onwards.
Compiled code:

1. flushes `a0`..`a7` to the VM context, since the handler reads them there;
2. calls a trampoline whose address the context carries;
3. checks the returned status;
4. reloads the argument registers.

The trampoline is monomorphised over the embedder's data type, casts the opaque
pointer back to the store, and dispatches on the number.

## Failure

The trampoline returns a status. Zero means the call succeeded and the guest
continues; anything else means it must stop, and compiled code returns
immediately with the reason recorded in the store.

That gives host functions two distinct failures:

- `Ok(code)` — a value the guest handles.
- `Err(..)` — the guest stops.

Without the status the guest would run on after a failed call with whatever
happened to be in `a0`.

## Cost

Flushing and reloading eight registers around each call. That is the price of
letting a handler read and write guest state through a stable structure rather
than threading registers through a signature the host would have to know.

Only the argument registers move — callee-saved and temporary registers stay in
Cranelift variables across the call, because the ABI says nothing may rely on
temporaries surviving one.
