# reddit

A Berm service that fetches subreddit RSS feeds and emits `Post` and
`Comments` logs. Uses [`lake`](../../lib/lake) to schedule fetches and track
items between calls.

## Tools

- `start`: account callers start the polling loop.
- `follow` / `unfollow`: service callers change followed listings with
  arguments such as `{"sub":"rust","sort":"new"}`.
- `follows`: lists the followed listings.
- `tick`: internal scheduled fetch; not a public tool.

Sorts are `new`, `hot`, `rising`, `top-{window}`, and
`controversial-{window}`, where the window is `hour`, `day`, `week`, `month`,
`year`, or `all`. No listings are followed by default.

The loop ticks every five minutes and refreshes listings at most hourly.
Posts are watched for 48 hours, with comments fetched at most every six hours.
`Comments` logs are emitted when the fetched comment count changes; RSS
comments are flat and carry no scores.

## Build

From the workspace root:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release -p reddit --bin reddit --target wasm32-unknown-unknown
```

Deploy `target/wasm32-unknown-unknown/release/reddit.wasm` as `reddit`, the
name its scheduled ticks use. Configure listings through a calling service
and call `start` from an account. The host must provide `berm.http.proxy`
access to `www.reddit.com`; deployment is refused without a configured proxy.

## License

Apache-2.0
