# lake

A `no_std` runtime for Berm source services that fetch feeds and emit items
and discussions as logs. Used by [`hn`](../../services/hn) and
[`reddit`](../../services/reddit).

Implement `Source` to fetch and log items and threads, and set a `Schedule`
with fetch limits and intervals in milliseconds. `Source::NAME` must match the
deployed service name because scheduled ticks address it by name.

Use `State::follow` and `State::save` to persist feed selections. Expose
`start::<S>` through a tool and `tick::<S>` through an internal `tick` tool.
The service must declare `berm.get`, `berm.set`, `berm.call.after`, and the
HTTP access its source needs.

Each tick fetches due feeds and threads, logs new items and threads whose
comment counts changed, saves state, and schedules the next tick. Fetch
failures are traced and retried when next due. State is stored under the
service's `lake` key.

## License

Apache-2.0
