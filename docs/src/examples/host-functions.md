# Host Functions

Letting a guest call back into the host.

```sh
cargo run --example host_functions
```

```rust,ignore
{{#include ../../../crates/rvtime/examples/host_functions.rs:example}}
```

## Numbers, not names

A guest reaches a host function with `ecall`, taking the call number from `a7`
and arguments from `a0` onwards — the standard RISC-V syscall convention. So
`Linker` is keyed by number.

That is not a simplification of wasmtime's named imports; it is what the input
format allows. A WebAssembly module carries an import table naming what it needs,
which the host resolves. An ELF carries nothing of the sort. There is no name to
match, so the guest and the host agree on a numbering, and nothing checks that
they agree — a mismatch surfaces as `Trap::UnknownHostCall` at the moment the
guest calls it.

## Reading and writing guest memory

`Caller` gives host functions access to the guest's address space. Buffers are
passed the usual way, as a pointer and a length in two registers, and the host
validates the range before touching it.

There is no marshalling layer above that. A host function receives `u64`s and
decides what they mean, because rvtime has no way to know.

## Failing

Two different failures, and the difference matters:

- **`Ok(code)`** returns a value the guest handles. Use this for anything the
  guest should be able to recover from — a missing key, a closed connection.
- **`Err(..)`** stops the guest. The pending call fails with the error attached.
  Use this when continuing makes no sense.

This is the same split as a return value versus a trap in wasmtime. Reaching for
`Err` where the guest could have coped turns a recoverable condition into a dead
plugin.

## Arity

`func_wrap` covers zero to six arguments. Beyond that, `Linker::func` hands you
the raw `Caller` and you read registers yourself — useful when the number of
arguments is not fixed.
