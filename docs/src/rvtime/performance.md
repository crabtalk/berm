# Performance

Every figure here was measured on the repository's own fixtures, on an Apple M
series machine. They are recorded because each one changed a decision.

## Compilation

For `hosted.elf` — 99 KiB, 55 functions, built with `alloc`:

| phase | time |
|---|---|
| ELF load and decode | 78 µs |
| everything (`Module::new`) | 10.6 ms |

**Code generation is ~99% of compile time.** That is why caching targets codegen
and nothing else, and why the ELF front end has never needed optimising.

Compilation is eager and whole-program: every function in `.text` is compiled
when the module is created, whether or not it is ever called.

## Caching

| | time |
|---|---|
| no cache | 13.7 ms |
| cold, writing entries | 27.4 ms |
| warm | 5.3 ms |

A warm cache is **2.6× faster than none**; a cold one is **2× slower**. See
[Caching](./design/caching.md).

## Compiling ahead of time

Compiling once into an object file and mapping it back, against caching
generated code per function. Two guests, both compiled through `rvtime::Module`:

The fastest of forty compiles, since a loaded machine moves the median around
far more than it moves the floor:

| guest | | cold | warm |
|---|---|---|---|
| `hosted.elf`, 113 KiB, 55 fns | incremental cache | 120 ms | 14 ms |
| | object artifact | 59 ms | **0.5 ms** |
| fixture, 359 KiB, 183 fns | incremental cache | 425 ms | 69 ms |
| | object artifact | 355 ms | **2.2 ms** |

**A warm artifact is 26–32× faster than a warm cache**, because a cache hit
still translates every function to CLIF to compute its key, and an artifact
skips straight to mapping the code. Cold is no worse: the cache writes one entry
per function, the artifact writes one file.

Loading a 575 KiB artifact is ELF decode, digest, then map-and-relocate. The
digest is the largest single part at 0.8 ms, and costs 5 ms on a build whose
`sha2` cannot reach the CPU's SHA extensions.

## Interruption

**0.2%** on a 50-million-iteration tight loop — the worst case, since the check
is proportionally largest where the loop body is smallest. Straight-line code
pays nothing.

A first attempt measured this against a recursive `fib` and reported *−20%*,
which was noise: `fib` is call-dominated, so the loop check barely features.
Measuring the wrong workload is easier than it looks.

## Optimisation level

`OptLevel::None` compiles faster; `OptLevel::Speed` generates better code. On
these fixtures the difference in compile time is small and in run time not
reliably measurable, because the fixtures are too short to say anything useful.

There is no benchmark suite yet, so treat any claim about *guest* execution
speed as unmeasured. The figures above are all about the compiler, not the code
it produces.
