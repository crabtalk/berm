# berm-api

What a harness says it is, and what bermd's control API speaks.

Here rather than in the service, so a client can read an image and talk to the
API without linking a compiler.

```rust
let manifest = berm_api::Manifest::from_elf(&elf)?;
for tool in &manifest.tools {
    println!("{}: {}", tool.name, tool.description);
}
```

`Manifest::from_elf` reads the `.berm.abi` section — a section rather than an
export, so learning what an image claims to be never means running it. An image
built against a different `ABI_VERSION` is refused here, before a host can
dispatch into a system harness its author did not mean.

`Harness`, `ToolSpec` and `Output` are the control API's own shapes.
`Output::Failed` is the harness's own report and arrives with the same `200` a
result does: the call was fine and the tool said no.

## License

Apache-2.0
