# berm

The command-line client for [bermd](../service).

```sh
berm new example
berm push ghcr.io/org/example:v1 ./program.elf
berm publish ghcr.io/org/example:v1
berm search "read a file"
berm deploy example ghcr.io/org/example:v1
berm ls
berm inspect example
berm rm example
```

`deploy` takes a file or a registry reference — a path that exists is read,
anything else is pulled and checked against the digest the registry advertised.

`new`, `push`, `publish` and `search` are the subcommands that do not talk to
bermd. `new` writes a program crate pinning the `berm-lang` that shipped with
this binary; `push` uploads an image to a registry.

`publish` and `search` reach an index — `https://github.com/crabtalk/berm-index.git`
unless `--index` or `BERM_INDEX` names another. An index is a git repository, so
that can be a directory to read as-is, a `.git` URL kept as a copy under
`~/.berm/index`, or the URL of a service. `search` reads a copy with no service
and no credential; `publish` needs the service, because appending to the list
means holding a credential for the repository.

`--host` points at a service somewhere other than `http://127.0.0.1:7777`.

`inspect` renders a tool's JSON Schema as the argument list it describes, since
a schema printed raw is something to read past rather than read.

Invoking a tool is not here: agents reach tools over MCP, and this drives the
control API.

## License

Apache-2.0
