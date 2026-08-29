# berm-indexd

A local stand-in for an index service.

Publishing to a real index means appending to a git repository, which needs a
credential a laptop has no business holding. This serves the same two routes
over a plain directory, so the whole loop can be run without deploying
anything:

```sh
berm-indexd --index ./index &
berm push ghcr.io/me/example:v1 ./program.elf
berm publish --index http://127.0.0.1:7788 ghcr.io/me/example:v1
berm search --index http://127.0.0.1:7788 "read a file"
```

What it writes is what a clone of a real index holds — one JSON Lines file per
program — so the same directory reads back with no service at all:

```sh
berm search --index ./index "read a file"
```

## What it is not

No git, no credential, no identity, and it re-reads the directory on every
request rather than holding one. A deployment does those things; a stand-in
that did them would be the deployment.

It does do the one thing that matters for testing: `publish` pulls the artifact
from its registry, anonymously, and records what the image says it is. A
reference nobody pushed is refused here exactly as it would be in production.

## License

Apache-2.0
