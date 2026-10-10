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

An image is compiled before it is stored, so a broken one is refused by the
deploy that introduced it rather than on a model's turn. What is stored is
restored on the next start.

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
to the hosts its manifest declares under `berm.http.fetch`, and any other
program deployed beside it. There is no wiring step: what is deployed is
reachable by name, the way containers on one network reach each other.

```sh
berm deploy inner ./inner.wasm
berm deploy outer ./outer.wasm
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
berm inspect ghcr.io/org/example:v1
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
