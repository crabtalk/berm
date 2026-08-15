# Getting Started

## Adding rvtime

```toml
[dependencies]
rvtime = "0.0.1"
```

rvtime runs on Linux and macOS. Windows is not supported — guest memory and trap
handling are POSIX, and a port would mean `VirtualAlloc`/`VirtualProtect` and a
vectored exception handler.

## Building a guest

The image must be a **statically linked RV64IMAC ELF, linked with
`--emit-relocs`**:

```sh
cargo build --release --target riscv64imac-unknown-none-elf \
    --config 'target.riscv64imac-unknown-none-elf.rustflags=["-Clink-arg=--emit-relocs"]'
```

The relocations are not optional, and leaving them out is the first thing to
check when a guest fails to load. They are what identify which functions have
their address taken, and therefore where an indirect call may legally land. See
[Calls](./design/calls.md).

A guest is `no_std`. It gets `alloc` once you hand it a heap, which unlocks
`Vec`, `String`, and any crate that does not need `std`. It cannot use `std`
itself: every Rust RISC-V std target is `riscv64gc`, whose code contains `F`/`D`
instructions rvtime does not implement.

Floating point still works — on a target without hardware float, LLVM lowers it
to soft-float calls into `compiler_builtins`, which is ordinary integer code.

## The guest SDK

`rvtime-guest` supplies the two things every guest needs: a way to reach the
host, and an allocator.

```rust,ignore
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
    unsafe { call2(1, a, b) } // whatever number the embedder registered
}
```

There are no standard host functions in the SDK. It knows *how* to make a call,
never *which* calls exist.

## Keeping exports alive

The linker garbage-collects anything unreachable from the entry point, which
removes exported functions the host means to call. Anchor them:

```rust,ignore
#[unsafe(no_mangle)]
pub static EXPORTS: [extern "C" fn(u64, u64) -> u64; 2] = [add, multiply];
```

and reference that table from `_start`, or the table itself will be collected
along with everything it names.

## Where to go next

[Calling a Guest](./examples/calling-a-guest.md) is the smallest complete
program. [Host Functions](./examples/host-functions.md) covers the other
direction.
