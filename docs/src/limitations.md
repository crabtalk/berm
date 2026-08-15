# Limitations

Known gaps, and what each would take.

## Platform

**Windows is not supported**, and the build says so rather than failing inside
the C compiler.

Two thirds of the port are mechanical. Guest memory maps almost directly:
`mmap`/`mprotect`/`munmap` become `VirtualAlloc`/`VirtualProtect`/`VirtualFree`.
Catching the fault is a swap of `sigaction` for `AddVectoredExceptionHandler`.
Recovery is the part that does not translate.

rvtime recovers with `_longjmp`. On Windows that unwinds, which needs unwind
information for every frame it walks, and guests are compiled with `unwind_info`
off. Turning it on does not help: **cranelift-jit never registers unwind tables
with the OS.** It contains no call to `RtlAddFunctionTable` and does not even
depend on the Windows API that provides it, so the data would exist and nothing
would know about it.

### What it would take

wasmtime solves this by not unwinding at all. Its vectored handler rewrites the
thread context and resumes execution somewhere else:

```rust,ignore
context.Rip = handler.pc as _;
context.Rbp = handler.fp as _;
context.Rsp = handler.sp as _;
EXCEPTION_CONTINUE_EXECUTION
```

Those values are captured at the guest entry point, so the effect is a
`setjmp`/`longjmp` pair built out of the operating system's context mechanism
instead of libc's.

The instructive part is that wasmtime uses the same design on Unix. Windows is
not the odd platform here — **rvtime's C shim is.** Adopting the saved-context
approach would delete the shim and the `cc` build dependency, give both
platforms one mechanism, and make the unwind-info problem disappear entirely,
because nothing would ever unwind.

What it costs is exactly what `setjmp` hides today. Restoring a context by hand
means touching architecture-specific fields — `uc_mcontext.gregs[REG_RIP]` on
Linux x86_64, `__ss.__pc` on macOS arm64, `Rip`/`Rsp` on Windows x64 — so four
variants replace one portable call.

That leaves a small assembly trampoline to capture the entry frame, a vectored
handler, and the memory port. wasmtime is Apache-2.0 with LLVM exception, so
adapting the approach is compatible with this project's licence.

### Why it waits

Not because the design is unknown; it is written down above. Because none of it
can be executed on a POSIX machine.

This is the one component where being subtly wrong converts a catchable trap
into a crash, and its characteristic failure is passing the test that was
written while breaking on a different stack shape. It wants a Windows machine in
the development loop, not a CI job at the end.

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
