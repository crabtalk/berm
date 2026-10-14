# berm-host

The shared host for Berm daemons, built on the Wasmtime-backed
[`berm`](../berm) runtime.

`Host<E>` manages deployed services, persistent state, logs, subscriptions,
scheduled calls, per-service databases, and usage accounting. The daemon
authenticates callers and serves its own routes.

Implement `Env` to supply HTTP transport, image fetching, peer calls, secret
resolution, admission policy, and the clock. Pass those implementations as
`Edges` to `Host::open`, then call `Host::boot` inside a Tokio runtime to
restore services and start background work.

The `test` feature adds `berm_host::test`: a host whose edges a test holds,
running a service's built image under the real engine. Fetches answer what the
test arranged, the clock stands where the test set it, and the test fires the
wakes the service armed.

`Config` controls instance capacity, instruction and database-row budgets,
call depth, the compiled-code cache, and the database directory. See
[`bermd`](../daemon) for a daemon using this host.

## License

Apache-2.0
