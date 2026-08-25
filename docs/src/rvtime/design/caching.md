# Caching

A guest arrives as one ELF, and there are two ways to avoid compiling the same
one twice. Which is in play is a build-time choice: the `jit` feature caches
generated code per function, the `aot` feature keeps the whole compiled object.

## What is worth caching

Measured before deciding: for a 99 KiB guest, loading and decoding the ELF is
**78 µs of 10.6 ms** — under 1%. Code generation is essentially all of it.

So caching anything short of generated code would have been pointless, and that
ruled out the obvious cheap options.

## Per function: the incremental cache

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

What remains on a hit is CLIF construction and key hashing, which is why a warm
cache still costs milliseconds rather than microseconds: every function is
translated again just to discover it need not be compiled.

## Whole program: the object artifact

The `aot` feature skips that residue by keeping the finished module. Compiling
writes an object file, and loading maps it — no CLIF, no hashing, no codegen.

This was once rejected here on the grounds that a relocating loader would be a
platform-specific component to write and maintain, needing `ADRP`/`ADD` pairs on
arm64 and absolute and relative 64-bit entries on x86_64. Measuring the emitted
code showed otherwise. Across all three fixtures, both optimisation levels and
three target triples, every relocation is a call from one guest function to
another:

| triple | relocations | external | kinds |
|---|---|---|---|
| aarch64-apple-darwin | 65 | 0 | `Arm64Call` |
| aarch64-unknown-linux-gnu | 65 | 0 | `Arm64Call` |
| x86_64-unknown-linux-gnu | 65 | 0 | `X86CallPCRel4` |

One kind per architecture and nothing external, because the design already
routes everything address-shaped through `VmCtx` at run time: `host_call` and
the dispatch table are indirect calls on loaded pointers, and the memory base is
a register rather than a baked constant. The generated code is self-contained
and position-independent as a unit, so the loader is one `match` with two arms.

## What is in an artifact

| section | contents |
|---|---|
| `.text` | every guest function and the trampoline, one contiguous section |
| `.rvtime.elf` | the guest ELF, verbatim |
| `.rvtime.meta` | digest, fingerprint, and the offset of each function in `.text` |

The guest ELF travels *inside* the artifact. Re-decoding it on load costs that
same 78 µs, and in exchange a `Program` is still built the one way it has always
been built — so there is no second description of what a guest is, and nothing
that can disagree.

It is an ordinary ELF or Mach-O object, so `objdump -d` disassembles the code
Cranelift generated.

## Why an artifact is refused

The address mask and the interrupt checks are compiled *into* the code. An
artifact loaded against a different address space would confine guest addresses
to a range other than the one actually reserved — which is a hole in the
sandbox, arrived at silently. So `.rvtime.meta` records the target triple, the
full ISA settings, the address space size and whether interrupt checks were
emitted, and a mismatch in any of them is a refusal rather than an adaptation.
They are stored verbatim rather than hashed: a mismatch is then a mismatch, with
no collision to reason about.

## Damage

An artifact is a file, so it can be truncated or overwritten, and unlike a
damaged cache entry it is *executable*. Code whose calls were never patched
still runs — into whatever those calls happen to reach. Nothing else in the file
would notice.

So the header carries a digest over the code, the guest image and the rest of
the header, and the number of call sites the code was emitted with. Both are
checked before anything is mapped. The guarantee tested is not that every
damaged file is rejected — lopping the last byte off a string table changes
nothing the loader reads — but that a damaged file never becomes code that runs
and answers wrongly.

Artifacts are written to a temporary file and renamed, for the same reason cache
entries are, and a stored artifact that fails to load is treated as a miss and
recompiled rather than as an error.

## What it costs

For the 359 KiB fixture, 183 functions:

| | cold | warm |
|---|---|---|
| incremental cache | 247 ms | 43 ms |
| object artifact | 159 ms | **1.5 ms** |

**A warm artifact is ~26× faster than a warm cache**, and cold is no worse: the
incremental cache serialises and writes 183 separate entries where the artifact
writes one file.

Loading breaks down as ELF decode, the digest, and the map-and-relocate. The
digest is over half of it, which is why `sha2` is built with its assembly
backend — the portable implementation costs 5 ms where the hardware instruction
costs 0.8 ms, on a load that is otherwise 0.7 ms.
