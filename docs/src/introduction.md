# berm

Two things ship from this repository, and this book covers both.

**[berm](./berm/overview.md)** is the OS for agent harnesses: it pins a guest by
hash, compiles it once, instantiates it per invocation, and hands it only the
syscalls it was given. `bermd` deploys programs and serves their tools over MCP,
which is how Claude Code, Codex and OpenCode reach them.

**[rvtime](./rvtime/introduction.md)** is the RISC-V compiler underneath — load
an ELF, generate native code for it with Cranelift, call its exported functions,
and let it call back.

The split is load-bearing rather than tidy. rvtime knows about ELFs, registers,
and traps; berm knows about tools, manifests, and arguments. Every convention
that makes a guest a berm *program* lives on the berm side, which is what leaves
rvtime usable for a guest that is not one.

## How to read this

Start with berm to write or run a program. Its five chapters cover what the
sandbox guarantees, how to write a guest against it, what an image declares
about itself, how one travels between machines, and how the service serves it.

Read rvtime to change the compiler, or when something surprises you and the
question is whether it was deliberate. Its
[examples](./rvtime/examples/calling-a-guest.md) are real programs in the
repository, compiled by CI and included here by reference rather than pasted, so
what you read is what runs.

API documentation is not published; `cargo doc --open` builds it from the
source.
