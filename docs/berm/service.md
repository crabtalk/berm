---
title: Running the Service
---

`bermd` is a long-running service that deploys programs and serves their tools
over MCP.

An invocation is ephemeral, but three things around it are not: the deployed
set, the modules compiled from it, and the engine's code cache. That is what the
service exists to hold. A program is compiled once, at deploy, so an invocation
pays only instantiation.

```sh
bermd &
crab deploy
crab ls
```

## Deploying

The control API is resource-shaped — `GET`, `PUT` and `DELETE` on
`/programs/{name}` — for the reason dockerd's is: the clients are a UI, a CLI,
and `curl`, and a program is a resource.

`GET /programs/{name}` answers the program as anyone it is visible to sees it:
its IDL under `program`, and under `served` the calls of its tools from outside
that ended done, per day over the last 30 days and in total.
`GET /programs/{name}/idl` answers the IDL alone: name, version, digest, package
fields and public tools, what a caller needs. `GET /programs/{name}/runtime` answers
how it runs: its other tools, `deps`, `events`, and what of `deps` nothing here
answers to. A deploy answers the IDL beside how it broke callers of the program
it replaced.

A deploy is recorded only once its image has compiled, so a rejected one is
never restored. What is recorded is restored on the next start, and one
unloadable image is reported and skipped rather than taking the service down
with it.

A tool is called with `POST /programs/{name}/tools/{tool}`, the body its
argument object. A tool its manifest marks `query` also answers
`GET /programs/{name}/tools/{tool}?args=<json>`, the argument object URL-encoded;
any other tool answers that with `405`.

There is no container here, so there is nothing to start or stop: a program is
deployed or it is not, and dockerd's lifecycle verbs have nothing to attach to.

## One endpoint

Every deployed program appears on a single MCP endpoint at `/mcp`, with tools
named `{program}.{tool}` — the same dotted rule the guest ABI already uses for
host call names, so `fs.read` reads the way it does inside a program.

One endpoint rather than one per program, for the reason dockerd has one socket
rather than one per container. It costs two things, both cheap: names have to be
namespaced, and program names may not contain a dot, because dispatch splits on
the first one.

Deploying and undeploying emit `notifications/tools/list_changed`. The tool set
moves under clients that are already holding a list, which per-program endpoints
would have made someone else's problem.

## READMEs as resources

MCP gives a server one `instructions` string, sent on every turn. Each program
with a [README](./manifest.md#package) publishes it as a
`berm://{program}/readme` resource, which a model reads when choosing among one
program's tools.

## What a deployed program can reach

Its arguments, the log, itself, and the programs on the same daemon its
manifest declares in `programs`. A call to any other name is
`CallError::Refused`.

```rust
let result = berm_lang::call("inner", "echo", r#"{"query":"hi"}"#)?;
```

`bermd` serves this as one syscall, `berm.call`, and the target is a
field in the request rather than part of the call number. So an image is never
built against a particular deploy — the same bytes work wherever they land, and
deploying them twice under two names makes them reachable as both.

A name nothing answers to is `CallError::Refused`: nothing ran. The target
running and reporting failure is `CallError::Failed`. That is the same split the
control API draws between a status and an `Output::Failed`, and a program can
act on it — falling back when something is not deployed, say.

`--max-call-depth` bounds the chain, 4 by default, `0` refusing the first nested
call. The bound is on runaway composition rather than on the stack, which a
level costs around 720 bytes of; what a level does cost is 64 MiB of reserved
guest address space.

That reach stops at the daemon. A program still touches the world outside only
through syscalls its host registered, and `bermd` registers none — so
what matters before deploying an image is what the image *is*. `Manifest::from_image`
reads a program's tools and README without compiling or running it, which is what
`crab inspect` shows you.

The endpoint carries no authorization and binds loopback by default.
