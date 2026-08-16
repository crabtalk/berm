# Calling a Guest

The smallest complete program: compile an ELF, instantiate it, call a function.

```sh
cargo run --example calling_a_guest
```

```rust,ignore
{{#include ../../../crates/rvtime/examples/calling_a_guest.rs:example}}
```

## What is happening

**`Engine`** holds the target configuration — optimisation level, address space
size, whether interruption checks are emitted. Compiled code is tied to it,
because some of those settings are baked into the generated instructions. A
module and the store that runs it must come from the same engine.

**`Module::new`** does all the work: it parses the ELF, recovers function
boundaries from the symbol table, decodes every instruction, analyses control
flow, and generates native code for the whole program. There is no lazy path
yet, so this is where the time goes — see [Performance](../performance.md).

**`Store`** owns one guest: its memory, its registers, and whatever data you
want host functions to see. One store holds one instance, which is narrower
than wasmtime and matches what a program image needs — one address space, one
register file.

**`get_typed_func`** resolves a symbol and gives it a Rust signature. Nothing
checks that signature against the guest, because an ELF carries no type
information to check against. The type parameters choose how many argument
registers to write and how many results to read; they are not a contract the
guest declared.

At most **eight arguments** and **two results** — `a0`..`a7` going in, `a0` and
`a1` coming back. Asking for more results is an error rather than a silent
misread, because the registers beyond those two hold whatever was there before
the call. See [Registers](../design/registers.md).
