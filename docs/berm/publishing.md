---
title: Publishing a Program
description: "A program is one file, so it travels as one OCI layer with no tarball around it."
---

```toml
# berm.toml, beside the program's Cargo.toml
name = "example"
image = "ghcr.io/org/example"
public = []
```

```sh
crab deploy
```

`crab deploy` builds the program and, against a host that pulls from a
registry, pushes the image to `image` tagged with the crate's version and
deploys that reference. Against `bermd` it sends the image itself, so `bermd`
never talks to a registry, holds no credentials, and still restores from
`--root` on start without a network.

## The artifact

```json
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.manifest.v1+json",
  "artifactType": "application/vnd.berm.program.v1",
  "config": { "mediaType": "application/vnd.berm.manifest.v1+json", "digest": "sha256:…" },
  "layers": [ { "mediaType": "application/vnd.berm.program.v1", "digest": "sha256:…" } ]
}
```

The config blob is the [`.berm.abi`](./manifest.md) section carried out of the
image byte for byte, so what a registry serves cannot disagree with what the image
holds. The tools, their schemas and the README are one blob, and reading them
never means pulling the program.

Nothing about the program is repeated in annotations. Only
`org.opencontainers.image.source` and `.revision` are set, from
`GITHUB_REPOSITORY` and `GITHUB_SHA` when a build has them, which is what makes
GHCR show a package against its repository.

## One digest

Because the layer is the image and nothing else, the layer's digest is sha256 of
the image — the same hash `crab ls` reports, carrying the registry's `sha256:`
prefix:

```console
$ crab ls
NAME     DIGEST        TOOLS
fixture  222890c498ed  echo

$ shasum -a256 target/wasm32-unknown-unknown/release/fixture.wasm
222890c498ed28f4bf60670a223141489d9879020bd1890111b8c11ac79fa31d
```

A pulled image is checked against the digest the registry advertised before it
goes anywhere, so a registry that serves the wrong bytes is caught by the deploy
rather than by the model.

## What deploy refuses

`crab deploy` reads the manifest out of the image before it uploads anything,
which means a broken image is refused by whoever built it rather than at deploy
on someone else's machine. Reading it never runs the guest.

## From a workflow

`GITHUB_TOKEN` is the registry credential, and the deploy is two lines — there
is no action to install.

```yaml
permissions:
  packages: write

steps:
  - run: crab context add cloud https://api.crabtalk.ai --token "$KEY" --org acme
    env:
      KEY: ${{ secrets.CRABTALK_KEY }}
  - run: crab deploy --context cloud
    env:
      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

Locally, `GITHUB_TOKEN=$(gh auth token)` does the same. `~/.docker/config.json`
is not read.

`BERM_REGISTRY_MIRRORS` reaches a registry at another address, as
`name=endpoint` pairs separated by commas. A host in a container pulls what was
pushed to `localhost:5001` from `http://registry:5000` with
`BERM_REGISTRY_MIRRORS=localhost:5001=http://registry:5000`.
