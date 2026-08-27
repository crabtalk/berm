# berm-oci

Publishing and fetching harnesses as OCI artifacts.

A harness is one layer and no tarball, so the layer's digest is sha256 of the
ELF — the same hash `berm ls` prints, carrying the registry's `sha256:` prefix.

```rust
let reference: berm_oci::Reference = "ghcr.io/org/example:v1".parse()?;
```

What the harness *is* rides in the config blob: the `.berm.abi` section
verbatim, so a registry can be listed without pulling any image.

## License

Apache-2.0
