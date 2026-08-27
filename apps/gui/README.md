# berm.app

A workbench for harnesses — deploy one, read its tools, run them.

The app *is* bermd. It owns the `Service`, binds the endpoint, and runs both on
a tokio runtime beside gpui's, so launching this window starts the daemon: a
`berm` CLI in a terminal and an agent's MCP client reach the same harnesses the
window paints.

```sh
cargo run -p berm-gui
./bundle.sh              # a macOS .app
```

Painted with [bezel](https://crates.io/crates/bezel), whose `gpui` is the one
to depend on: a second copy is a second type universe, whose failure is a window
that paints shapes but no text.

## License

Apache-2.0
