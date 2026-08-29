# berm-codegen

The `#[program]` proc macro. Re-exported by
[`berm-lang`](https://crates.io/crates/berm-lang) — depend on that, not on this.

```rust
#[berm_lang::program]
mod tools {
    use berm_lang::{Failed, Out};

    /// Echo the argument blob back.
    #[args(Echo)]
    pub fn echo(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        out.write(args);
        Ok(())
    }

    /// Arguments for `echo`.
    pub struct Echo {
        /// The text to echo back.
        pub query: String,
    }
}
```

Every `pub fn` becomes a tool; its doc comment is the description the model
reads, and a tool without one is a compile error. `#[args(Struct)]` derives the
JSON Schema from that struct's fields. The handler still receives raw bytes, so
a program that wants no JSON parser links none.

The expansion also carries the exports, the `.bss` buffers sized by `buffer = N`,
and the `.berm.abi` manifest — which is what lets a host learn what a program is
without running it.

## License

Apache-2.0
