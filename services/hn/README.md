# hn

A Berm service that fetches Hacker News through Algolia's API and emits
`Story` and `Thread` logs. Uses [`lake`](../../lib/lake) to schedule fetches
and track items between calls.

## Tools

- `start`: account callers start polling `frontpage`, `newest`, `ask`, and
  `show`; repeated calls answer `running` while polling is active.
- `lanes`: lists the lanes and their descriptions.
- `tick`: internal scheduled fetch; not a public tool.

Feeds refresh every five minutes. Discussions for front-page, Ask HN, and
Show HN stories are watched for 48 hours and fetched at most every two hours.
`Thread` logs are emitted when the fetched comment count changes.

## Build

From the workspace root:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release -p hn --bin hn --target wasm32-unknown-unknown
```

Deploy `target/wasm32-unknown-unknown/release/hn.wasm` as `hn`, the name its
scheduled ticks use, then call `start`. The host must allow HTTP fetches to
`hn.algolia.com`; no API key is needed.

## License

Apache-2.0
