---
title: berm
---

**[berm](./berm/overview.md)** is the OS for agent harnesses: it pins a WebAssembly module by hash, compiles it once, instantiates it per invocation, and hands it only the syscalls it was given. `bermd` deploys programs and serves their tools over MCP, which is how Claude Code, Codex and OpenCode reach them.

## How to read this

Its five chapters cover what the sandbox guarantees, how to write a guest against it, what an image declares about itself, how one travels between machines, and how the service serves it.

API documentation is not published; `cargo doc --open` builds it from the source.
