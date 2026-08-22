# Calls

## A direct call looks indirect

The single most surprising thing about compiling RISC-V. LLVM materialises a
call target as a pair:

```asm
auipc ra, 0x0          # ra = pc + 0
jalr  ra, 0x4a(ra)     # jump to ra + 0x4a
```

That is a *direct* call to a known address, encoded as a jump through a
register. Almost every call in a compiled binary looks like this; plain `jal` is
comparatively rare.

Compiling every `jalr` as an indirect dispatch would be correct and
catastrophically slow. So a per-block constant-propagation pass tracks known
register values and folds the pair back into the address it was always going to
compute.

Matching on adjacency would be simpler and wrong. LLVM may schedule other
instructions between the two, and linker relaxation can collapse the pair into a
bare `jal`. The pass tracks values, not patterns.

## Classifying a transfer

Once a target is known, what it *is* depends on the program, not the encoding:

| shape | meaning |
|---|---|
| `jalr zero, 0(ra)` | return |
| target is a known function entry | call (tail call if `rd` is `zero`) |
| target is inside this function | local jump |
| target unknown | indirect call |

The order matters. Asking "is `rd` equal to `ra`?" is not sufficient: a
`jal ra, <local label>` targets an address inside the current function and is
not a call, while recursion targets an address that is both a function entry and
inside the current function and *is* one. Checking the entry set first gets both
right.

This was a real bug, found by a fixture that spelled out encodings LLVM never
emits.

## Indirect calls

A computed target is checked against a dispatch table: one slot per two bytes of
`.text`, since RISC-V instructions are two-byte aligned, filled from the
relocation-derived entry set. A target with no slot traps.

That null check is what stops a corrupted function pointer from becoming an
arbitrary jump. It is also why `--emit-relocs` is mandatory: the relocations are
what say which addresses a function pointer may legitimately hold. The
alternative — scanning `.rodata` for values that look like code addresses — is a
heuristic, and a missed switch table would turn into a trap on correct code.

## Tail calls

`jr` compiles as call-then-return. Semantically identical, but it consumes a
native frame, so unbounded tail recursion grows the host stack rather than
running in constant space. Switching the guest convention to `CallConv::Tail`
would fix it.
