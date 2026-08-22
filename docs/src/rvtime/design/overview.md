# Overview

## The pipeline

```text
ELF ──▶ decode ──▶ analyse ──▶ CLIF ──▶ Cranelift ──▶ native code
```

Four crates, split so that decoding is settled before any code generation
decision is made:

| crate | responsibility |
|---|---|
| `rvtime-core` | registers, instructions, ELF loading. No codegen. |
| `rvtime-cranelift` | control-flow analysis and CLIF emission. |
| `rvtime-compiler` | codegen backends, guest memory, trap handling. |
| `rvtime` | the public API. |

Backends depend on `core`; `core` depends on no backend. The seam is the `Inst`
enum, so a second backend could consume the same vocabulary without the first
knowing.

## One CLIF function per RISC-V function

The central decision, and the one everything else follows from.

An ELF already carries function structure in its symbol table, so there is no
whole-program control-flow graph to rebuild. Each RISC-V function becomes one
Cranelift function: `jal` becomes a real `call`, `ret` becomes a native
`return`, and the host's own stack carries return addresses.

The alternative — compiling the whole program into a single function with every
basic block in one map, and a jump table for every transfer — is what you are
forced into when the input has no function boundaries. It works, but it makes
every function's registers live in one enormous SSA graph, rules out compiling
anything independently, and scales badly with program size.

Because functions are separate here, they can be compiled independently, cached
independently, and eventually compiled lazily.

## What the ELF has to provide

Three things are recovered at load time and cannot be recovered later:

- **Function boundaries**, from `STT_FUNC` symbols. Interior `.L*` labels share
  the address space of real functions and are filtered out; treating one as a
  function splits a body in half.
- **Indirect jump targets**, from `R_RISCV_64` relocations landing in `.text`.
  This is why `--emit-relocs` is required — see [Calls](./calls.md).
- **Segment layout and permissions**, from the program headers.

## Guest code is never executed

The guest's instructions are compiled, not interpreted and not run in place.
Guest memory is mapped read-only where the image is executable, and never
executable at all. The only reason to map `.text` is that programs read
constants out of it.

That also means a guest which corrupts its own code image achieves nothing: the
compiled code was generated once and lives in the JIT's pages.
