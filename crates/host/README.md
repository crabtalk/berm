# berm-host

The shared host for Berm daemons, built on the Wasmtime-backed
[`berm`](../berm) runtime.

`Host<E>` manages deployed services, persistent state, logs, subscriptions,
scheduled calls, per-service databases, and usage accounting. The daemon
authenticates callers and serves its own routes.

Implement `Env` to supply HTTP transport, image fetching, peer calls, secret
resolution, and admission policy. Pass those implementations as `Edges` to
`Host::open`, then call `Host::boot` inside a Tokio runtime to restore services
and start background work.

`Config` controls instance capacity, instruction and database-row budgets,
call depth, the compiled-code cache, and the database directory. See
[`bermd`](../../apps/service) for a daemon using this host.

## License

Apache-2.0
