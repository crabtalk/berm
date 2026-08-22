# berm

The command-line client for [bermd](../service).

```sh
berm new example
berm push ghcr.io/org/example:v1 ./harness.elf
berm deploy example ghcr.io/org/example:v1
berm ls
berm inspect example
berm rm example
```

`deploy` takes a file or a registry reference — a path that exists is read,
anything else is pulled and checked against the digest the registry advertised.

`new` and `push` are the subcommands that do not talk to the service. `new`
writes a harness crate pinning the `berm-lang` that shipped with this binary;
`push` publishes an image, using `GITHUB_TOKEN` for the registry.

`--host` points at a service somewhere other than `http://127.0.0.1:7777`.

`inspect` renders a tool's JSON Schema as the argument list it describes, since
a schema printed raw is something to read past rather than read.

Invoking a tool is not here: agents reach tools over MCP, and this drives the
control API.

## License

Apache-2.0
