# Interrupting a Guest

Stopping a guest that will not stop itself.

```sh
cargo run --example interrupting
```

```rust,ignore
{{#include ../../../rvtime/rvtime/examples/interrupting.rs:example}}
```

## Why this exists

Without it, a guest that loops forever holds the thread that called into it
forever. There is no way to take that thread back: you cannot safely kill a
thread mid-execution, and the guest is running native code with no scheduler
above it.

For a guest you wrote, that is a bug you fix. For anything installed at runtime,
it is a denial of service against the host — which is why the checks are **on by
default**, and why `interrupt_handle()` errors rather than returning a handle
that would silently do nothing on a module compiled without them.

## What it costs

A load, a test and a branch on every backward edge. Measured at **0.2%** on a
50-million-iteration tight loop — the flag stays in L1 and the branch predicts
perfectly, so it fills slack the loop already had.

Straight-line code pays nothing: the flag pointer is loaded once per function,
and only in functions that actually loop.

## What it does not cover

Only loops. That is enough for non-termination — a guest can run forever by
looping, and unbounded recursion exhausts the stack and traps — but it means a
long-running *straight-line* computation cannot be interrupted mid-way.

In practice, optimisation decides. A counted loop that LLVM turns into a closed
form has no backward edge and therefore no check; it also always terminates, so
nothing is lost.

## Using it

`Interrupt` is `Send + Sync` and cheap to clone, so a watchdog thread can hold
one while the guest runs elsewhere. `interrupt()` returns immediately; the guest
stops at its next loop iteration and its pending call fails with
`Trap::Interrupted`. `clear()` withdraws the request so the store can be used
again.
