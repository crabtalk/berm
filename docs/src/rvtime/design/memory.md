# Memory

## Layout

```text
[ image ][ heap ][ guard ]        ...        [ stack ]
0                                                size
```

The whole address space is reserved in one `PROT_NONE` mapping and the pieces
are committed into it, so a guest address is an offset from a single base.
Everything not committed is a guard page.

## Confinement, not bounds checking

A guest address becomes a host address by masking and adding:

```text
host = base + (guest & (size - 1))
```

That mask is the sandbox. Guest addresses are 64-bit values a program can
compute arbitrarily; masking keeps every access inside the reservation, where
anything uncommitted faults. Without it a guest could compute an address past
the end and read host memory.

It requires the size to be a power of two — any other size would leave part of
the mask's range pointing outside the reservation.

**This confines rather than bounds-checks.** An address past the end wraps and
may land on a committed page instead of faulting. A guest can therefore corrupt
itself with a wild pointer, but it cannot reach anything outside its own memory.
Precise faulting would need explicit compare-and-branch on every access, which
costs throughput.

## Where the mask lives

Compiled into the generated code as an immediate. That makes a `Module` tied to
the size its `Engine` was configured with, and it is why a `Store` maps memory
at the *module's* size rather than its own config — mapping at any other size
would let the compiled mask disagree with the reservation.

## Faults

An out-of-bounds access hits a guard page and raises a signal inside JIT
compiled code. A handler records the faulting address, translates it back into
the guest address space, and unwinds via `setjmp`/`longjmp` to the frame that
entered the guest.

Both `SIGSEGV` and `SIGBUS` are handled: Linux reports these as `SIGSEGV`, macOS
on arm64 reports `SIGBUS`. Handling only one silently never fires on the other.

## Page granularity

Permissions apply at *host* page granularity, which is not the guest's 4 KiB
page. macOS on arm64 uses 16 KiB pages, and a typical RISC-V image places its
read-only, executable and writable segments 4 KiB apart — so all three land in
one host page.

Where segments share a page it takes the **union** of their permissions. The
alternative is for whichever segment is written last to silently strip rights
from the others. On a 16 KiB-page host this lets a guest write to its own code;
on a 4 KiB-page host it does not. Either way the sandbox boundary is unaffected,
since that is the reservation, not the page bits.

## The heap

Committed read-write and zeroed at instantiation, occupying everything between
the image and the stack less one guard page. Committing it up front costs
address space rather than memory, because `mprotect` only changes protection and
pages fault in when first touched.

rvtime does not carve it up or tell the guest where it is — see
[The Guest Heap](../examples/guest-heap.md).
