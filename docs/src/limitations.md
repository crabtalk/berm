# Limitations

Known gaps, and what each would take.

## Platform

**Windows is not supported.** Guest memory and trap handling are POSIX:
`mmap`/`mprotect`, `sigaction`, and `setjmp`/`longjmp`. A port means
`VirtualAlloc`/`VirtualProtect` and a vectored exception handler, and would have
to reckon with Windows' 64 KiB allocation granularity being distinct from its
page size — which affects the address space layout.

CI covers Linux/x86_64 and macOS/arm64.

## Guests

**No hardware floating point.** RV64IMAC only. Guests can use floats via
soft-float, which is bit-exact, but an image built for `riscv64gc` contains real
`F`/`D` instructions and is rejected at load with the offending instruction
named.

**No `std`.** This follows from the above: every Rust RISC-V std target is `gc`.
Guests are `no_std` plus `alloc`, which covers crates that do not need an
operating system. Anything that does — files, sockets, threads — has to come
from host functions.

**The LP64 ABI is assumed.** Hand-written assembly that passes data in
callee-saved or temporary registers across a call will misbehave, and rvtime
cannot detect it. Compiler output is fine. See [Registers](./design/registers.md).

**`--emit-relocs` is required**, so a stripped third-party binary will not load.
This is the accepted trade for knowing exactly where an indirect call may land.

## Runtime

**Tail calls grow the native stack.** `jr` compiles as call-then-return, which is
semantically identical but consumes a frame. Unbounded tail recursion will
exhaust the host stack instead of running in constant space. Switching the guest
convention to `CallConv::Tail` fixes it.

**Interruption only covers loops.** A long-running straight-line computation
cannot be stopped part-way. See [Interruption](./design/interruption.md).

**Compilation is eager.** `Config::strategy` exists with a single `Eager`
variant; nothing is compiled lazily, so a module pays for functions that are
never called.

**Memory confinement is not a precise bounds check.** An address past the end of
the address space wraps rather than faulting, so a wild pointer can corrupt the
guest's own memory. It cannot escape it. See [Memory](./design/memory.md).

**No guest execution benchmarks.** Compile-time figures are measured; claims
about how fast compiled guest code runs are not.
