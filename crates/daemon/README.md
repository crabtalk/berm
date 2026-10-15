# bermd

A long-running service that deploys programs and serves their tools over MCP.

An invocation is ephemeral — berm instantiates a program per call and nothing
survives it. What the service holds is everything around that: the deployed set,
the modules compiled from it, and the engine's code cache. A program is compiled
once, at deploy, so an invocation pays only instantiation.

```sh
bermd --addr 127.0.0.1:7777 --root ~/.berm
```

## Deploying

The control API is resource-shaped, for the reason dockerd's is: the clients are
a UI, a CLI, and `curl`.

```sh
curl -X PUT --data-binary @program.wasm http://127.0.0.1:7777/programs/example
curl http://127.0.0.1:7777/programs
curl -X DELETE http://127.0.0.1:7777/programs/example
```

A tool is called with `POST`; one marked `query` also takes `GET`, its
arguments URL-encoded in `args`.

```sh
curl -X POST -d '{"query":"hi"}' http://127.0.0.1:7777/programs/fixture/tools/echo
curl -G --data-urlencode 'args={}' http://127.0.0.1:7777/programs/fixture/tools/peek
```

What a program emitted is at `GET /programs/{name}/logs`, oldest first, `?topic=&call=&after=&page=&limit=`, with the other pages in `Link`; each log names the call it was emitted in and is kept 30 days, whatever the call ended as. Each run of a program — a call from outside or a wake — is at `GET /programs/{name}/calls`, newest first, `?tool=&page=&limit=`, with its outcome, duration, meter and price. `GET /programs/{name}/schedule` answers the pending wakes that call one of its tools, and each scheduled tool with its period. `PUT /programs/{name}/tools/{tool}/schedule` sets a scheduled tool's period as `{"every": ms}`, `null` stopping it; a deploy's `?schedule=tool:ms,…` sets every period, and is refused, unless `force`, when it would undo one set since the last deploy that set them.

A tool's answer carries `Berm-Meter`, what the call used across every program it reached, as JSON: `{"calls", "instructions", "overage", "memory", "read", "written", "proxied"}` — wasm instructions executed, those each invocation executed past its first 1,000,000 summed, the largest linear memory one instance reached in bytes, rows read and written, and bytes sent and received through the proxy. The same call against the same state counts the same here and on Cloud. `GET /programs/{name}/meters` answers a program's meters over the last 30 days, a day each, with the bytes it stored. An invocation stops at its tool's instruction budget, and a row syscall it makes once it has read and written its row budget together is refused: 1,000,000,000 instructions and 1,000,000 rows, unless `PUT /programs/{name}/tools/{tool}/budget` set `{"instructions", "rows"}` lower, from the next call. `PUT /programs/{name}/tools/{tool}/price` sets `{"price_micros"}`, what a call of the tool that ends done costs its caller in USD micros, 0 by default; bermd charges nothing. `GET /programs/{name}/terms` answers each public tool's as `{"tools": {"<tool>": {"budget": {"instructions", "rows"}, "price_micros"}}}`, and MCP's tool list carries a tool's in `_meta.terms`.

`POST /programs/{name}/tools/{tool}/dry` runs a public tool dry and answers as a call does, `Berm-Meter` included. The body is `{"args", "answers", "offline"}`: `args` is the argument object, a string handed to the tool as its text. A dry chain keeps nothing berm holds, in any program it reaches: no state, logs or database writes are committed, no wake is armed, and it is not counted in `usage`; its meter is counted. Its requests to other hosts are sent as a call sends them, unless one of `answers`, `{"method", "url", "status", "headers", "body"}`, matches by method and URL, which answers it in place of sending; with `offline`, a request none matches is refused. MCP lists a `dry_run` tool taking `{"tool": "{program}.{tool}", "args", "answers", "offline"}` and answering `{"output", "meter"}`.

A tool call may carry `X-Berm-Caller`, the caller a program reads from `berm_lang::caller()` and the user it reads from `berm_lang::user()`, as JSON: `{"id": "alice", "user": {"issuer": "https://id.example", "subject": "42"}}`. bermd checks none of it. A call without one has no caller.

A deploy is recorded only once its image has compiled, so a broken one is
refused by the deploy that introduced it rather than on a model's turn. What is
recorded is restored on the next start.

To check a running service rather than serve anything, `berm-fixture` is the
guest berm's own tests use — its tools price things rather than do them, so it
proves the path works and is not something to leave deployed.

```sh
cargo build --release -p berm-fixture --target wasm32-unknown-unknown
curl -X PUT --data-binary @target/wasm32-unknown-unknown/release/fixture.wasm \
  http://127.0.0.1:7777/programs/fixture
```

## MCP

Every deployed program appears on one endpoint at `/mcp`, its tools named
`{program}.{tool}` — the same dotted rule the guest ABI already uses for host
call names. Deploying and undeploying emit `notifications/tools/list_changed`,
because the tool set moves under clients already holding a list.

A program's `usage` is published as a `berm://{program}/usage` resource rather
than folded into the server's `instructions`, which a model pays for on every
turn.

## What a program can reach

Its arguments, the log, its own stored keys, the clock, a callback timer, HTTP
to the hosts its manifest declares under `berm.http.fetch` (or `berm.http.proxy`, through `--proxy` when given and direct otherwise), itself, and the programs deployed beside it that its manifest declares in `programs`, by name.

```sh
crab deploy inner
crab deploy outer
```

```rust
let forecast = berm_lang::call("inner", "echo", r#"{"query":"hi"}"#)?;
```

The name is read at the call, so an image is not built against a particular
deploy: the same one works wherever it lands, and deploying it twice under two
names makes it reachable as both. A name nothing answers to gives
`CallError::Refused`, which a program can tell apart from `CallError::Failed` —
the target ran and said no.

`--max-call-depth` bounds how far a chain may nest, 4 by default; `0` refuses
the first nested call.

Outside the daemon, a program reaches only the hosts its manifest declares;
bermd registers no files and no commands. The bound worth minding is therefore
what an image *is*, not which of your programs it can call: read what one
claims before deploying it.

```sh
crab inspect ghcr.io/org/example:v1
```

A request header may name `{secret:NAME}` for a secret the manifest declares.
bermd substitutes what `{root}/secrets.toml` grants that program for that host,
read at every request:

```toml
[fixture.fixture]
value = "..."
hosts = ["127.0.0.1"]
```

The endpoint carries no authorization and binds loopback by default.

## License

Apache-2.0
