# rvtime

A RISC-V compiler with a wasmtime-like interface.

Load a statically linked RV64IMAC ELF, compile it to native code with Cranelift,
call its exported functions from Rust, and let it call back into the host.

```rust
use rvtime::{Caller, Config, Engine, Linker, Module, Store};

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

## Status

Working end to end: ELF loading, RV64IMAC decoding, control-flow analysis,
Cranelift codegen, guest memory with a committed heap and guard pages, host
calls, traps, a compiled-code cache, and a guest-side SDK. 101 tests, no
clippy warnings.

**Developed and tested on macOS/arm64 only.** The code paths are shared and
nothing is macOS-specific by design, but rvtime has never been run on Linux.
Treat Linux support as unverified.

Not done yet: lazy compilation, real tail calls, and benchmarks. See
[Limitations](#limitations).

## Guest requirements

The image must be a **statically linked RV64IMAC ELF, linked with
`--emit-relocs`**:

```sh
cargo build --release --target riscv64imac-unknown-none-elf \
    --config 'target.riscv64imac-unknown-none-elf.rustflags=["-Clink-arg=--emit-relocs"]'
```

The relocations are not optional. They are what identify which functions have
their address taken, and therefore where an indirect call may legally land.
Without them a computed jump cannot be checked, and the alternative — guessing
from a heuristic scan — turns a missed switch table into a trap on correct code.

Floating point (`F`/`D`) is not supported. Decoding happens at load time, so a
`riscv64gc` image that actually uses it fails in `Module::new` with the offending
instruction, rather than misbehaving later.

## How it works

**One CLIF function per RISC-V function.** The ELF already carries function
structure, so there is no whole-program control-flow graph to rebuild: `jal`
becomes a real `call`, `ret` becomes a native `return`, and the host's own stack
carries return addresses. This is what makes per-function compilation strategies
possible at all.

**Direct calls have to be recovered.** On RISC-V a direct call is *encoded* as
an indirect jump — LLVM emits `auipc ra, hi` followed by `jalr ra, lo(ra)`.
Compiling every `jalr` as an indirect dispatch would be correct and
catastrophically slow, so a per-block constant-propagation pass folds the pair
back into the address it was always going to compute. Matching on adjacency
would be simpler and wrong: LLVM may schedule instructions between the two, and
linker relaxation can collapse the pair into a bare `jal`.

**Registers cross a call by ABI.** A compiled function is
`fn(vmctx, sp, a0..a7) -> (sp, a0, a1)`. Callee-saved registers stay in the
caller's CLIF variables and are never handed over; a callee that clobbers `s0`
spills it to the guest stack in its own prologue exactly as the hardware would,
and those are real memory accesses that rvtime honours. This assumes the guest
follows the LP64 ABI — compiler output does, hand-written assembly passing data
in `s0` across a call does not.

**Memory is confined, not bounds-checked.** Each store reserves a guest address
space (64 MiB by default, configurable) as one `PROT_NONE` mapping and commits
the image into it. A guest address becomes a host address by masking with
`memory_size - 1` and adding the base, so a guest can never reach outside its
own memory; anything not committed is a guard page, and a hit is caught as a
`Trap::MemoryFault` with the exact guest address. The size must be a power of
two, and the mask is compiled in — so a `Module` is tied to the size its
`Engine` was configured with.

The trade-off is explicit: an address past the end of the space *wraps* and may
land on a mapped page rather than faulting. The sandbox holds either way, but
this is not a precise bounds check.

**Generated code can be cached.** Set `Config::cache_dir` and Cranelift's
incremental cache persists generated functions across runs, keyed on the CLIF
contents plus the target settings — so changing the optimisation level or the
target produces misses rather than stale code, and two guests sharing a function
share one entry. Measured on the 99 KiB `hosted` fixture: 13.7 ms with no cache,
5.3 ms warm. The first compile is slower (~27 ms) because it also serialises and
writes every function, so this pays off when a guest is compiled once and loaded
many times.

**Guest memory is never executable.** Compiled code lives in the JIT's own
pages, so the execute bit is dropped when mapping. Permissions apply at *host*
page granularity, which is not the guest's 4 KiB page — on a 16 KiB-page host
(macOS/arm64) an image's read-only, executable and writable segments typically
share one page and take the union of their permissions. That lets a guest write
to its own code; it does not weaken the sandbox boundary.

## Layout

```
crates/
  core/        shared vocabulary: registers, instructions, ELF loading. No codegen.
  cranelift/   translator: control-flow analysis and CLIF emission.
  compiler/    codegen backends, guest memory, trap handling.
  rvtime/      the public API, for the host.
  guest/       the guest-side SDK, compiled for RISC-V. Not in the workspace.
fixtures/      RISC-V test guests, with prebuilt .elf and .objdump goldens.
```

Backends depend on `core`; `core` depends on no backend. The seam is the `Inst`
enum — decoding is settled before any codegen decision is made, so a second
backend could consume the same vocabulary without the first knowing.

## Writing a guest

`rvtime-guest` is the guest-side crate: it knows *how* to reach the host and
supplies an allocator, and nothing else. There are no standard host functions in
it — the call numbers, their meanings, and what a guest may do are the
embedder's to define.

```rust
#![no_std]
#![no_main]

extern crate alloc;
use rvtime_guest::{call2, heap};

/// Traps instead of looping, so a guest bug reaches the host as a catchable
/// trap rather than hanging the thread that called in.
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    rvtime_guest::abort()
}

/// The embedder calls this first, passing the bounds of `Store::heap()`.
#[unsafe(no_mangle)]
pub extern "C" fn init_heap(start: u64, size: u64) -> u64 {
    unsafe { heap::init(start as usize, size as usize) };
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn add(a: u64, b: u64) -> u64 {
    unsafe { call2(1, a, b) }   // whatever number the embedder registered
}
```

rvtime commits a heap but never tells the guest where it is: the host reads
`Store::heap()` and passes the bounds in through its own interface. That keeps
the runtime free of any opinion about the host ABI. Once the allocator has them
`alloc` works — `Vec`, `String`, and any crate that needs no `std`.

## Building

```sh
cargo test --workspace
```

Rust 2024 edition. No RISC-V toolchain needed — the fixtures are committed as
prebuilt ELFs alongside their disassembly.

To regenerate them after changing a fixture's source:

```sh
rustup target add riscv64imac-unknown-none-elf
rustup component add llvm-tools-preview
./fixtures/build.sh
```

## Testing

Every test lives in `tests/`; there are no in-file test modules.

The decoder is checked **differentially against LLVM**: sweep every `.text`
section and require agreement with `llvm-objdump -M no-aliases` on address,
length, and opcode for every instruction. Boundary agreement is the real signal
— a wrong compressed immediate desynchronises the sweep and every later address
diverges. Together the fixtures cover 122 distinct mnemonics:

| fixture | what it is for |
|---|---|
| `basic` | compiled Rust: leaf functions, loops, recursion, indirect calls, atomics |
| `wide` | a `global_asm!` block spelling out encodings LLVM never emits |
| `hosted` | `ecall` host calls and the guest SDK, with a `_start` that returns |

`wide` exists because compiled Rust only exercises what LLVM happens to choose,
which left most of RV64IMAC untested. It earned its keep immediately by catching
a real bug: `jal ra, <local label>` was being classified as a call, because the
analysis asked whether `rd == ra` rather than whether the target was a known
function entry.

## Differences from wasmtime

- **Host functions are keyed by number, not name.** The guest calls them with
  `ecall`, taking the number from `a7` — the standard RISC-V syscall convention.
  An ELF has no symbolic import table to resolve names against.
- **A store holds one instance.** A program image is one address space and one
  register file; there is nothing to gain from sharing a store.
- **Signatures are not checked.** Arguments are plain 64-bit words in `a0`..`a7`,
  so the type parameters on `get_typed_func` choose how many registers to use
  rather than describing something the guest declared.

## Limitations

- **Linux is unverified.** Everything here has only run on macOS/arm64.
- **No floating point.** RV64IMAC only; no `F`/`D`.
- **Tail calls grow the stack.** `jr` compiles as call-then-return, which is
  semantically identical but means unbounded tail recursion grows the *native*
  stack. Fixed by switching to `CallConv::Tail`.
- **Eager compilation only.** `Config::strategy` exists with a single
  `Strategy::Eager` variant; lazy compilation is not implemented.
- **Non-ABI assembly breaks.** A guest that passes data in callee-saved or
  temporary registers across a call will misbehave. Compiler output is fine.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
