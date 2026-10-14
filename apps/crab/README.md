# crab

The command-line client for [bermd](../service) and CrabTalk Cloud.

```sh
crab new example
crab push ghcr.io/org/example:v1 ./program.wasm
crab publish ghcr.io/org/example:v1
crab search "read a file"
crab deploy example ghcr.io/org/example:v1
crab ls
crab inspect example
crab idl example
crab logs example
crab usage example
crab rm example
```

## Contexts

`crab` talks to the host its context names, the way `docker context` picks a daemon. `default` is the bermd on `http://127.0.0.1:7777`; `--context` picks another for one command.

```sh
crab context add cloud https://api.crabtalk.ai --token "$KEY" --org acme
crab context use cloud
crab context ls
crab context use default
```

A context with a token and an `--org` or `--user` is Cloud, and its services are that account's. Against Cloud, `deploy` pushes a file to `--reference`, publishes the reference to Cloud's program index, and deploys it from there; a reference is published and deployed without a push. The key needs `deploy` and `publish` scope. Account operations — orgs, keys, billing — are not here.

Contexts are kept in `~/.crab/contexts.toml`.

`deploy` takes a file or a registry reference — a path that exists is read,
anything else is pulled and checked against the digest the registry advertised.

`new`, `push`, `publish` and `search` are the subcommands that do not talk to
the context's host. `new` writes a program crate pinning the `berm-lang` that shipped with
this binary; `push` uploads an image to a registry.

`publish` and `search` reach an index — `https://github.com/crabtalk/berm-index.git`
unless `--index` or `BERM_INDEX` names another. An index is a git repository, so
that can be a directory to read as-is, a `.git` URL kept as a copy under
`~/.berm/index`, or the URL of a service. `search` reads a copy with no service
and no credential; `publish` needs the service, because appending to the list
means holding a credential for the repository.

`inspect` renders a tool's JSON Schema as the argument list it describes, since
a schema printed raw is something to read past rather than read.

`inspect` shows the most one call of each tool may use and its price, `budget NAME TOOL --instructions N --rows N` sets the first, and `price NAME TOOL MICROS` the second, in USD micros a call that ends done. `run` calls one public tool and prints what the call used on stderr, and with `--dry` keeps nothing the call changes, its requests answered from `--answers FILE` where one matches and refused under `--offline` where none does; `usage` prints a program's meters over the last 30 days, a day each. Agents reach tools over MCP. On bermd, `--caller alice --issuer https://id.example --user 42` makes the call as that caller and user, unchecked, so a program reading `berm_lang::caller()` and `user()` runs as it would for them on Cloud.

## License

Apache-2.0
