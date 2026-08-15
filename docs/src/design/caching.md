# Caching

## What is worth caching

Measured before deciding: for a 99 KiB guest, loading and decoding the ELF is
**78 µs of 10.6 ms** — under 1%. Code generation is essentially all of it.

So caching anything short of generated code would have been pointless, and that
ruled out the obvious cheap options.

## What was not chosen

Serialising the finished module — emitting an object file and relocating it back
in on load — would skip everything, not just codegen. It also means writing a
relocating loader, and relocations are architecture-specific: absolute and
relative 64-bit entries cover x86_64, while arm64 needs `ADRP`/`ADD` pairs and
`CALL26`. That is a platform-specific component to write and maintain.

Cranelift's incremental cache captures most of the win without any of it.

## How it works

`Context::compile_with_cache` hashes the CLIF function together with the ISA
settings and looks the result up before generating anything. On a hit it
deserialises the compiled code; on a miss it compiles and stores.

The key is what makes it safe to share. It covers the function's *contents*, not
its name or address, so two guests containing the same function reuse one entry.
It covers the target settings, so changing the optimisation level produces
misses rather than code built for different flags.

Entries are written to a temporary file and renamed, because a daemon may
compile the same guest from several processes at once and a partially written
entry would be indistinguishable from a complete one.

## What it costs

| | time |
|---|---|
| no cache | 13.7 ms |
| cold, writing entries | 27.4 ms |
| warm | 5.3 ms |

**A warm cache is 2.6× faster; a cold one is 2× slower**, because it serialises
and writes every function. This pays off when a guest is compiled once and
loaded many times, and loses for a compile-and-discard workload.

The residual 5.3 ms is CLIF construction, key hashing and deserialisation —
which also means actual code generation was about 8.4 ms of the original 13.7.

## Damage

A cache is disk state that other things can corrupt or truncate. A damaged entry
must cause a recompile, never a miscompile, so that is tested directly: every
entry is overwritten with garbage and the module must still compile *and compute
the right answer*.
