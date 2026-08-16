# rvtime

[![CI](https://github.com/crabtalk/rvtime/actions/workflows/ci.yml/badge.svg)](https://github.com/crabtalk/rvtime/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-crabtalk.github.io-blue)](https://crabtalk.github.io/rvtime/)

A RISC-V compiler with a wasmtime-like interface.

Load a statically linked RV64IMAC ELF, compile it to native code with Cranelift,
call its exported functions from Rust, and let it call back into the host.

```rust
let engine = Engine::new(&Config::default())?;
let module = Module::from_file(&engine, "guest.elf")?;

let mut store = Store::new(&engine, 0u64);
let mut linker = Linker::new(&engine);

// The guest reaches this with `ecall` and `a7 == 1`.
linker.func_wrap(1, |mut caller: Caller<'_, u64>, a: u64, b: u64| {
    *caller.data_mut() += 1;
    Ok(a + b)
})?;

let instance = linker.instantiate(&mut store, &module)?;
let add = instance.get_typed_func::<(u64, u64), u64>("op_add")?;
assert_eq!(add.call(&mut store, (10, 3))?, 13);
```

## Documentation

- **[Guide and design notes](https://crabtalk.github.io/rvtime/)** — how it works
  and why, with worked examples.
- **[API reference](https://crabtalk.github.io/rvtime/api/)** — generated from the
  source.

The design chapters cover the parts most likely to surprise: why a *direct* call
on RISC-V is encoded as an indirect jump, why registers cross calls by ABI, and
why guest memory is confined rather than bounds-checked.

## Status

Working end to end: ELF loading, RV64IMAC decoding, control-flow analysis,
Cranelift codegen, guest memory with a committed heap and guard pages, host
calls, traps, interruption, a compiled-code cache, and a guest-side SDK.

CI runs the suite on Linux/x86_64 and macOS/arm64, in debug and release. Windows
is not supported — memory and traps are POSIX.

Guests must be linked with **`--emit-relocs`**; that is the first thing to check
when one fails to load. See
[Getting Started](https://crabtalk.github.io/rvtime/getting-started.html), and
[Limitations](https://crabtalk.github.io/rvtime/limitations.html) for what is not
done yet.

## Building

```sh
cargo test --workspace
```

Rust 2024 edition. No RISC-V toolchain needed — the fixtures are committed as
prebuilt ELFs alongside their disassembly, which is also why CI can run the whole
suite without one. To regenerate them:

```sh
rustup target add riscv64imac-unknown-none-elf
rustup component add llvm-tools-preview
./fixtures/build.sh
```

## Contributing

Every test lives in `tests/`; there are no in-file test modules. The decoder is
checked differentially against `llvm-objdump` over whole `.text` sections, and
the fixtures together cover 122 distinct mnemonics.

Examples under `crates/rvtime/examples/` are documentation: the book includes
them by reference and CI runs them, so they cannot drift from the API.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
