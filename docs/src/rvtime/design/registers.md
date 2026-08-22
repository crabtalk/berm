# Registers

Guest registers live in Cranelift variables inside a function, so register
allocation is Cranelift's problem and SSA construction comes free from the
function builder. `x0` folds to a constant and writes to it are discarded.

The question is what happens at a call boundary.

## The signature

```text
fn(vmctx, sp, a0..a7) -> (a0, a1)
```

Only the registers the RISC-V ABI says are live get passed. Callee-saved
registers stay in the caller's variables and are never handed over.

That works because a callee which clobbers `s0` spills it to the guest stack in
its own prologue and reloads it in its epilogue, exactly as the hardware would —
those are real guest memory accesses that rvtime honours. The value it spills is
its own zero-initialised `s0` rather than the caller's, which is unobservable:
nothing reads that slot except the epilogue that restores it.

The alternative — passing all thirty-two registers through the VM context —
would be correct regardless of what the guest does, at the cost of roughly
thirty loads and thirty stores wrapped around every call, including a
two-instruction leaf function.

## Why `sp` goes in but does not come out

`sp` is callee-saved. A conforming function restores it before returning, so the
caller's own value is still correct and returning it would be redundant.

It also would not fit. Three results are fine on aarch64, which has three return
registers, but **x86_64's `Fast` convention has two** — a three-result signature
fails to compile there outright. The design was arm64-shaped without anyone
noticing until CI ran on x86_64.

Two results is therefore both the correct number and the maximum, which is why
`get_typed_func` refuses a wider result type rather than reading registers the
callee never wrote.

## `gp` and `tp`

The exception. They are set once at startup and read everywhere, so threading
them through every signature would be wasteful and dropping them would be wrong.
They live in the VM context and are loaded only by functions that reference
them, and written back only by functions that modify them.

## The assumption

All of this assumes the guest honours the LP64 ABI. Compiler output does.
Hand-written assembly that passes data in `s0` or `t0` across a call does not,
and will misbehave rather than be diagnosed — rvtime cannot detect it.
