# bermd

A long-running service that deploys harnesses and serves their tools over MCP.

An invocation is ephemeral — berm instantiates a harness per call and nothing
survives it. What the service holds is everything around that: the deployed set,
the modules compiled from it, and the engine's code cache. A harness is compiled
once, at deploy, so an invocation pays only instantiation.

```sh
bermd --addr 127.0.0.1:7777 --root ~/.berm
```

## Deploying

The control API is resource-shaped, for the reason dockerd's is: the clients are
a UI, a CLI, and `curl`.

```sh
curl -X PUT --data-binary @harness.elf http://127.0.0.1:7777/harnesses/example
curl http://127.0.0.1:7777/harnesses
curl -X DELETE http://127.0.0.1:7777/harnesses/example
```

An image is compiled before it is stored, so a broken one is refused by the
deploy that introduced it rather than on a model's turn. What is stored is
restored on the next start.

To check a running service rather than serve anything, `berm-fixture` is the
guest berm's own tests use — its tools price things rather than do them, so it
proves the path works and is not something to leave deployed.

```sh
cargo build --release -p berm-fixture --target riscv64imac-unknown-none-elf
curl -X PUT --data-binary @target/riscv64imac-unknown-none-elf/release/fixture \
  http://127.0.0.1:7777/harnesses/fixture
```

## MCP

Every deployed harness appears on one endpoint at `/mcp`, its tools named
`{harness}.{tool}` — the same dotted rule the guest ABI already uses for host
call names. Deploying and undeploying emit `notifications/tools/list_changed`,
because the tool set moves under clients already holding a list.

A harness's `usage` is published as a `berm://{harness}/usage` resource rather
than folded into the server's `instructions`, which a model pays for on every
turn.

## What a harness can reach

bermd passes no system harnesses to `Berm::load`, so a deployed harness can log,
read its arguments, and return — nothing else.

The endpoint carries no authorization and binds loopback by default.

## License

Apache-2.0
