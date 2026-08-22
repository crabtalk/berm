# The Guest Heap

Giving a guest a heap, so it can allocate.

```sh
cargo run --example guest_heap
```

```rust,ignore
{{#include ../../../rvtime/rvtime/examples/guest_heap.rs:example}}
```

## Why the host has to hand it over

rvtime commits the heap but does not tell the guest where it is. That looks like
an omission and is deliberate.

Every way of conveying the bounds automatically means rvtime inventing an ABI:
a header at a fixed guest address, a reserved call number, a value stashed in
`tp`. Each of those claims part of a space the embedder owns, and each becomes a
compatibility surface the moment anyone depends on it. So `Store::heap()` reports
the bounds and you pass them in however your interface already works — an init
export, as here, or a host function you registered.

## What you get

The region sits between the guest's image and its stack, committed read-write
and zeroed. Committing it up front costs address space rather than memory:
`mprotect` only changes protection, and pages fault in when first touched.

A guard page separates it from the stack, so running off the top of the heap
faults instead of quietly landing in stack frames.

Size follows `Config::memory_size` — the heap is whatever is left after the
image and the stack. For many small guests, size them down; the default 64 MiB
is address space per store.

## Before the handover

An allocation before `heap::init` returns null, and the guest writes through it.
Guest address 0 is never committed, so that faults rather than corrupting the
bottom of the address space. A null dereference in a guest is a trap you can
catch, not silent damage.
