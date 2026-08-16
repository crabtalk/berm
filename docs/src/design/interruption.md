# Interruption

## Backward edges are enough

A guest can only run forever by looping. Unbounded recursion exhausts the native
stack and traps, because each guest call is a real native call. So a check on
every backward edge catches every case of non-termination, and nothing else
needs one.

The translator emits, before each backward transfer:

```text
flag = load(interrupt_pointer)
if flag != 0 -> trap(Interrupted)
```

The pointer is loaded once per function, and only in functions that contain a
backward edge — straight-line code pays nothing.

## Not hoisting the check

The subtle part. If Cranelift treated the flag load as loop-invariant it would
hoist it out, and the guest would never see a request raised after it entered
the loop. Interruption would silently never work.

The load is therefore left able to trap and not marked `can_move`, which is what
prevents code motion. Since that is an argument about optimiser behaviour rather
than something the type system enforces, it is verified directly: a test runs a
guest in a genuine infinite loop, interrupts it from another thread, and fails
loudly on a timeout rather than hanging.

## Where the flag lives

In an `Arc<AtomicU64>`, with the VM context holding a pointer to it — not inline
in the context. A `Store` is `Send`, so it can move; a pointer into it would
dangle. The `Arc` also lets a watchdog on another thread hold a handle.

## Cost

A load, a test and a branch per iteration: **0.2%** on a 50-million-iteration
tight loop. The flag stays in L1 and the branch predicts perfectly, so it fills
slack the loop already had.

At that price the checks are on by default. A thread that cannot be reclaimed is
a far worse outcome than a fifth of a percent.

## What it does not cover

Only loops. A long-running straight-line computation cannot be interrupted
part-way through.

Optimisation decides more than the source does here: a counted loop that LLVM
turns into a closed form has no backward edge and therefore no check. It also
always terminates, so nothing is lost — but it means "this function loops in the
source" does not imply "this function is interruptible".
