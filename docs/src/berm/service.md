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

Nothing. `bermd` passes no system harnesses to `Berm::load`, so a guest can log,
read its arguments, and return.

The endpoint carries no authorization and binds loopback by default.
