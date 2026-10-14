# berm-index

A list of published programs, and how to search one.

A registry holds the bytes; an index holds the list, because no registry API
will tell you who published a program. The list is a git repository — one JSON
Lines file per program, one line per version — so a copy of it is a clone, and
reading one needs no service and no credential.

```rust
let source = berm_index::Source::new(None)?;
for entry in source.search("read a file")? {
    println!("{} {}", entry.reference, entry.digest);
}
```

`Index` itself reaches nothing: a caller fetches an artifact, builds an `Entry`
from what it said it was, and this holds and searches the result. `Source` is
what goes and gets one, from a clone or from a service.

## Local server example

Run a local index service over a directory from the workspace root:

```sh
cargo run -p berm-index --example server -- --index ./index
```

In another terminal, publish an artifact already pushed to a registry and search:

```sh
crab publish --index http://127.0.0.1:7788 ghcr.io/me/example:v1
crab search --index http://127.0.0.1:7788 "read a file"
crab search --index ./index "read a file"
```

The server verifies artifacts through anonymous registry access and records
their metadata locally. It has no authentication or git integration and is
intended for local development. Use `--addr` to change the default
`127.0.0.1:7788` address.

## License

Apache-2.0
