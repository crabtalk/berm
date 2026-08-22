# berm-fixture

The reference guest: the smallest real harness, and what berm is measured and
tested against. Its four tools each price or prove one thing — see the module
doc in `src/lib.rs`.

```sh
cargo test -p berm-fixture
cargo run -p berm --example measure    # reads the numbers off it
```

## License

Apache-2.0
