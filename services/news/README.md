# news

A Berm service that fetches 106 curated news desks — outlets and their sections — from their RSS and Atom feeds, or from an index page where a desk has none, and emits `Article` logs. Uses [`lake`](../../lib/lake) to schedule fetches and track items between calls.

## Tools

- `start`: account callers start polling every desk; repeated calls answer `running` while polling is active.
- `desks`: lists the desks, their outlets, descriptions and topics.
- `topics`: lists the topics desks are tagged with.
- `tick`: internal scheduled fetch; not a public tool.

The loop ticks every five minutes, fetches ten desks per tick and refreshes each desk every two hours. A feed's article is new when it was published after the newest one the feed had shown; an index page's article is new when it was not on the page at the last fetch.

## Build

From the workspace root:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release -p news --bin news --target wasm32-unknown-unknown
```

Deploy `target/wasm32-unknown-unknown/release/news.wasm` as `news`, the name its scheduled ticks use, then call `start`. The host must allow `berm.http.fetch` to the desks' hosts, which the manifest lists.

## License

Apache-2.0
