# Running the Service

`bermd` is a long-running service that deploys harnesses and serves their tools
over MCP.

An invocation is ephemeral, but three things around it are not: the deployed
set, the modules compiled from it, and the engine's code cache. That is what the
service exists to hold. A harness is compiled once, at deploy, so an invocation
pays only instantiation.

```sh
bermd &
berm deploy example ./harness.elf
berm ls
```

## Deploying

The control API is resource-shaped — `GET`, `PUT` and `DELETE` on
`/harnesses/{name}` — for the reason dockerd's is: the clients are a UI, a CLI,
and `curl`, and a harness is a resource.

An image is compiled *before* it is stored. A rejected one therefore leaves
nothing behind, rather than a file that fails again on every restart. What is
stored is restored on the next start, and one unloadable image is reported and
skipped rather than taking the service down with it.

There is no container here, so there is nothing to start or stop: a harness is
deployed or it is not, and dockerd's lifecycle verbs have nothing to attach to.

## One endpoint

Every deployed harness appears on a single MCP endpoint at `/mcp`, with tools
named `{harness}.{tool}` — the same dotted rule the guest ABI already uses for
host call names, so `fs.read` reads the way it does inside a harness.

One endpoint rather than one per harness, for the reason dockerd has one socket
rather than one per container. It costs two things, both cheap: names have to be
namespaced, and harness names may not contain a dot, because dispatch splits on
the first one.

Deploying and undeploying emit `notifications/tools/list_changed`. The tool set
moves under clients that are already holding a list, which per-harness endpoints
would have made someone else's problem.

## Usage, and why it is a resource

MCP gives a server one `instructions` string. A manifest carries one `usage` per
harness. Folding every harness's usage into `instructions` would put a manual in
front of the model on every turn — which is the one thing
[usage](./manifest.md#usage) is defined not to be.

So `instructions` stays a sentence, and each harness publishes its usage as a
`berm://{harness}/usage` resource. A model reads it when choosing among one
harness's tools, and pays nothing for the harnesses it is not considering.

## What a deployed harness can reach

Its arguments, the log, and any other harness on the same daemon.

There is no wiring step and no per-pair permission. What is deployed is
reachable by name, which is the reach containers on one network have of each
other, bounded the same way: by what the operator chose to run.

```rust
let result = berm_lang::call("inner", "echo", r#"{"query":"hi"}"#)?;
```

`bermd` serves this as one system harness, `berm.call`, and the target is a
field in the request rather than part of the call number. So an image is never
built against a particular deploy — the same bytes work wherever they land, and
deploying them twice under two names makes them reachable as both.

A name nothing answers to is `CallError::Refused`: nothing ran. The target
running and reporting failure is `CallError::Failed`. That is the same split the
control API draws between a status and an `Output::Failed`, and a harness can
act on it — falling back when something is not deployed, say.

`--max-call-depth` bounds the chain, 4 by default, `0` refusing the first nested
call. The bound is on runaway composition rather than on the stack, which a
level costs around 720 bytes of; what a level does cost is 64 MiB of reserved
guest address space.

That reach stops at the daemon. A harness still touches the world outside only
through system harnesses its host registered, and `bermd` registers none — so
what matters before deploying an image is what the image *is*. `Manifest::from_elf`
reads a harness's tools and usage without compiling or running it, which is what
`berm inspect` shows you.

The endpoint carries no authorization and binds loopback by default.
