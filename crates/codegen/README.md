# berm-codegen

The `#[program]` proc macro. Re-exported by
[`berm-lang`](https://crates.io/crates/berm-lang) — depend on that, not on this.

```rust
#[berm_lang::program]
mod tools {
    use berm_lang::Result;

    /// Greets `name`.
    #[public]
    pub fn greet(
        /// Who to greet.
        name: String,
    ) -> Result<String> {
        Ok(format!("hello, {name}"))
    }
}
```

Every `pub fn` becomes a tool; its doc comment is the description the model
reads, and a tool without one is a compile error. Its parameters are its
arguments and the `Ok` of its `Result` is its reply, each typed by `Schema`.
`#[derive(Schema)]`, `#[event]` and `#[error_code]` go on types anywhere in the
crate.

The expansion also carries the exports, the `.bss` argument buffer sized by
`buffer = N`, and the `.berm.abi` manifest — which is what lets a host learn
what a program is without running it.

## License

Apache-2.0
