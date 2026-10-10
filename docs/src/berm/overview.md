# Overview

A program is one WebAssembly module. berm pins it by hash, compiles it once, and
instantiates it per invocation: arguments go in through host calls, the result
comes back out through one, and nothing survives the call.

```rust,ignore
let berm = Berm::new(&engine, call::DEFAULT_CALL_DEPTH, vec![]);
berm.deploy("example", &wasm)?;

match berm.call(None, "example", "echo", br#"{"query":"hello"}"#.to_vec()).await? {
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

## Where wasmtime stops and berm starts

wasmtime loads an image, generates native code for it, and calls it. It has no idea what a program is — no tools, no manifest, no arguments, no JSON.

berm is the layer that decides those things: that a tool is an export named `berm_tool_*`, that an image describes itself in a `.berm.abi` section, and that arguments arrive as a blob the guest pulls in. Every syscall is the same number, request and staged reply, carried by one import, `berm.syscall`.
