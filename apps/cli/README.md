# berm

The command-line client for [bermd](../service).

```sh
berm deploy example ./harness.elf
berm ls
berm inspect example
berm rm example
```

`--host` points at a service somewhere other than `http://127.0.0.1:7777`.

`inspect` renders a tool's JSON Schema as the argument list it describes, since
a schema printed raw is something to read past rather than read.

Invoking a tool is not here: agents reach tools over MCP, and this drives the
control API.

## License

Apache-2.0
