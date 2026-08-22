# Publishing a Harness

A harness is one file, so it travels as one OCI layer with no tarball around it.

```sh
berm push ghcr.io/org/example:v1 ./target/riscv64imac-unknown-none-elf/release/example
berm deploy example ghcr.io/org/example:v1
```

`deploy` takes a file or a reference, not two commands: a path that exists is
read, and anything else is resolved as a reference and pulled. The bytes reach
the service the same way either way, so `bermd` never talks to a registry, holds
no credentials, and still restores from `--root` on start without a network.

## The artifact

```json
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.manifest.v1+json",
  "artifactType": "application/vnd.berm.harness.v1",
  "config": { "mediaType": "application/vnd.berm.manifest.v1+json", "digest": "sha256:…" },
  "layers": [ { "mediaType": "application/vnd.berm.harness.v1", "digest": "sha256:…" } ]
}
```

The config blob is the [`.berm.abi`](./manifest.md) section carried out of the
ELF byte for byte, so what a registry serves cannot disagree with what the image
holds. It is also what makes a listing cheap: the tools, their schemas and the
usage are one small blob, and reading them never means pulling the harness.

Nothing about the harness is repeated in annotations. Only
`org.opencontainers.image.source` and `.revision` are set, from
`GITHUB_REPOSITORY` and `GITHUB_SHA` when a build has them, which is what makes
GHCR show a package against its repository.

## One digest

Because the layer is the ELF and nothing else, the layer's digest is sha256 of
the ELF — the same hash `berm ls` reports, carrying the registry's `sha256:`
prefix:

```console
$ berm push 127.0.0.1:5000/berm/fixture:v1 ./fixture
127.0.0.1:5000/berm/fixture:v1
  digest  sha256:222890c498ed28f4bf60670a223141489d9879020bd1890111b8c11ac79fa31d

$ shasum -a256 ./fixture
222890c498ed28f4bf60670a223141489d9879020bd1890111b8c11ac79fa31d
```

A pulled image is checked against the digest the registry advertised before it
goes anywhere, so a registry that serves the wrong bytes is caught by the deploy
rather than by the model.

## What push refuses

`berm push` reads the manifest out of the ELF before it uploads anything, which
means a broken image is refused by whoever built it rather than at deploy on
someone else's machine. Reading it never runs the guest.

## From a workflow

`GITHUB_TOKEN` is the credential, and with `berm push` the step is one line —
there is no action to install.

```yaml
permissions:
  packages: write

steps:
  - run: cargo build --release --target riscv64imac-unknown-none-elf
  - run: berm push ghcr.io/${{ github.repository }}:${{ github.ref_name }} \
           target/riscv64imac-unknown-none-elf/release/example
    env:
      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

Locally, `GITHUB_TOKEN=$(gh auth token)` does the same. `~/.docker/config.json`
is not read.
