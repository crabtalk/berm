# berm-registry

An index of published harnesses, over the registries that hold them.

A registry holds the bytes; this holds the list, because no registry API will
tell you who published a harness — GitHub's Packages API refuses to enumerate
even public packages without a token. So membership is submitted, and this holds
the credential that records it.

```sh
berm-registry --index crabtalk/berm-index      # a GitHub repository
berm-registry --index ./index                  # or a directory
```

```sh
curl "http://127.0.0.1:7788/harnesses?q=read"
curl http://127.0.0.1:7788/harnesses/ghcr.io/clearloop/fs
curl -X POST http://127.0.0.1:7788/harnesses \
  -d '{"reference":"ghcr.io/clearloop/fs:v1.0.0"}'
```

## Mounting it

The routes carry no authentication. `router()` hands them over so a service can
own the policy over them, and say who is publishing by inserting a `Caller`:

```rust
.merge(berm_registry::router(index))
.route_layer(middleware::from_fn(auth::require_session))
```

Which is why the index validates no credential and holds no accounts: it never
sees one. Run bare, as the binary above does, it is open — and that is a real
mode, not a gap, because an entry is a full OCI reference and whoever holds that
namespace already proved it to the registry that issued their push token.

## What it stores

One JSON Lines file per harness repository, in a git repository. An entry is a
reference, its digest, who published it, and what the image said it was.

Every entry is keyed by a digest, and the bytes at a digest can never change —
so the tools and usage recorded beside it cannot go stale. Re-pushing a tag does
not rewrite an entry; it adds one, and the old line still truthfully describes
the old bytes. That is the whole reason this may cache anything at all.

Entries are full OCI references, so the index allocates no names and squatting
is impossible.

## What it does not store

Nothing else. The index holds no blobs, no README text, and no state this
process owns — it reads the repository at boot and holds it in memory, so
losing the process costs a restart. Publishing pulls the artifact *anonymously*
to fill an entry in, which means what gets listed is what a registry will
actually serve, and a harness nobody can pull cannot be listed.

No credentials of anyone else's, either. The index validates the artifact, not
the publisher, so nobody has to hand it a token that would let it act as them
somewhere else.

## Not in this yet

No rate limiting, no takedown route, no pagination. One process holding the
whole index in memory is good into the low thousands of entries.

## License

Apache-2.0
