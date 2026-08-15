# rvtime

A RISC-V compiler with a wasmtime-like interface.

Load a statically linked RV64IMAC ELF, compile it to native code with Cranelift,
call its exported functions from Rust, and let it call back into the host.

```rust,ignore
let engine = Engine::new(&Config::default())?;
let module = Module::from_file(&engine, "guest.elf")?;

let mut store = Store::new(&engine, ());
let instance = Linker::new(&engine).instantiate(&mut store, &module)?;

let add = instance.get_typed_func::<(u64, u64), u64>("op_add")?;
assert_eq!(add.call(&mut store, (10, 3))?, 13);
```

## What this is for

Running code you did not write, in the same process, without letting it reach
anything you did not hand it. A guest gets its own address space, can only call
host functions you registered, and can be stopped mid-run.

The compiler is eager and whole-program: `Module::new` decodes every function in
`.text` and generates native code for all of them. There is no interpreter and
no bytecode — a guest call is a native call.

## What it is not

**Not a WebAssembly runtime with different input.** The two problems differ in
ways that shape everything downstream. WebAssembly arrives with a type system,
an import table, and structured control flow. An ELF arrives with none of that:
no signatures to check calls against, no symbolic imports to resolve, and
control flow that has to be recovered from the instruction stream. Several
decisions in [Design](./design/overview.md) exist only because of that gap.

**Not a policy.** rvtime compiles, confines, and calls. Which host functions
exist, what they mean, and what a guest may do are the embedder's to decide. The
runtime ships no standard function set — see [Host Calls](./design/host-calls.md)
for why that separation is load-bearing rather than fastidious.

## How to read this

The [examples](./examples/calling-a-guest.md) are real programs in the
repository, compiled by CI, and included here by reference rather than pasted —
so what you read is what runs. Start there if you want to use rvtime.

The [design](./design/overview.md) chapters explain why the compiler is shaped
the way it is. Read those if you want to change it, or if you hit something
surprising and want to know whether it is deliberate.

API documentation lives in [rustdoc](https://docs.rs/rvtime) and is not
duplicated here.
