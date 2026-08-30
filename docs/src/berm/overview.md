# Overview

A program is one WebAssembly module. berm pins it by hash, compiles it once, and
instantiates it per invocation: arguments go in through host calls, the result
comes back out through one, and nothing survives the call.

```rust,ignore
let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
berm.deploy("example", &wasm)?;

match berm.call("example", "echo", br#"{"query":"hello"}"#.to_vec())? {
    Ok(result) => println!("{result}"),
    Err(failure) => eprintln!("{failure}"),
}
```

Two levels of result, because two things can go wrong and they are not the same
thing. The outer one is the host's: no such tool, a trap, a broken image. The
inner one is the program reporting failure through the ABI — which is a result
the model should see and react to, not an error the host should swallow.

## Nothing survives the call

Every invocation gets a fresh instance, so guest memory is not carried between
calls. A program that wants something to outlive an invocation has to put it
somewhere a host is holding.

This is the opposite of a container, and it is why there is no lifecycle to
manage — nothing to start, stop, or restart. What is expensive is compilation,
not instantiation, and compilation is paid once per image.

## The linker is the boundary

A program reaches the world only through the *syscalls* it was given,
and that list is the linker it is instantiated with. A call to anything else
traps because nothing is registered for the number naming it — not because a
check ran and said no. There is no check to write, and none to forget.

berm ships none. What a filesystem is bounded by, what shape a command's result
takes, where bytes persist — each is a decision about a host, and berm has no
host. An embedder passes `System` values to `Berm::new`, and that list is the
whole of what a guest can reach.

The same argument appears one layer down in
[Host Calls](../rvtime/design/host-calls.md): rvtime ships no standard function
set for the same reason.

## Two backends, one ABI

berm compiles a WebAssembly module under wasmtime, and a statically linked RV64
ELF under [rvtime](../rvtime/introduction.md). Which one a deploy reaches is
read off the image's first four bytes — the image already says what it is, and a
second answer beside it is one that can disagree.

RISC-V is experimental. What it buys is that any language with a RISC-V target
is a program without needing a wasm story of its own.

The two answer the same ABI: the same syscall names hashed to the same numbers,
the same length-prefixed framing, the same `berm_tool_*` exports, the same
`.berm.abi` section. Every syscall is written once, against berm's own `Guest`
trait, and a backend carries it across its own boundary and supplies nothing
else. Only the transport differs — an `ecall` with the number in `a7` on one,
one imported function taking it as an argument on the other.

## Where a backend stops and berm starts

rvtime and wasmtime load an image, generate native code for it, and call it.
Neither has any idea what a program is — no tools, no manifest, no arguments, no
JSON.

berm is the layer that decides those things: that a tool is an export named
`berm_tool_*`, that an image describes itself in a `.berm.abi` section, that
arguments arrive as a blob the guest pulls in rather than as registers. Those
are conventions, and keeping them out of the backends is what lets rvtime be
used for something that is not a program at all.
