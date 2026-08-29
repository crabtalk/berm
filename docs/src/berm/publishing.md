# Publishing a Program

A program is one file, so it travels as one OCI layer with no tarball around it.

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
  "artifactType": "application/vnd.berm.program.v1",
  "config": { "mediaType": "application/vnd.berm.manifest.v1+json", "digest": "sha256:…" },
  "layers": [ { "mediaType": "application/vnd.berm.program.v1", "digest": "sha256:…" } ]
}
```

The config blob is the [`.berm.abi`](./manifest.md) section carried out of the
ELF byte for byte, so what a registry serves cannot disagree with what the image
holds. It is also what makes a listing cheap: the tools, their schemas and the
usage are one small blob, and reading them never means pulling the program.

Nothing about the program is repeated in annotations. Only
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

## Being found

Pushing makes a program fetchable, not findable. Nothing can enumerate the
programs on a registry — GitHub's Packages API refuses to list even public
packages without a token — so an index has to be told a program exists.

```sh
berm publish ghcr.io/org/example:v1
berm search "read a file"
```

`--index <url>`, or `BERM_INDEX`, or the default list below.

`publish` records a reference and nothing else. The index pulls the artifact
itself, anonymously, and fills in the digest, tools and usage from the config
blob — so a listing describes what a registry will actually serve rather than
what a publisher claimed, and a program nobody can pull cannot be listed.

Entries are keyed by digest, which is what makes them safe to keep: the bytes at
a digest never change, so a recorded description of them cannot go stale.
Re-pushing a tag adds an entry rather than rewriting one.

## The list is a git repository

One JSON Lines file per program, one line per version — so a copy of an index is
a clone, and searching your copy needs no service, no credential and no network.
`berm search` keeps that copy for you under `~/.berm/index`, cloning it the
first time and not touching the network again:

```sh
berm search "read a file"                     # the default list
git -C ~/.berm/index/github.com/crabtalk/berm-index pull    # refresh it
```

The default is `https://github.com/crabtalk/berm-index.git`. `--index` or
`BERM_INDEX` names another, and takes three shapes: a directory to read as-is, a
`.git` URL to keep a copy of, or the URL of a service. Publishing needs the
service, because appending to the list means holding a credential for the
repository; reading never does.

`BERM_TOKEN` carries a credential when the service in front of you wants one.
Never a GitHub token: an index has no business holding something that could act
as you elsewhere.

[`berm-indexd`](https://github.com/crabtalk/berm/tree/main/apps/index) serves
those two routes over a directory, so the loop can be run without deploying
anything. What it writes is what a clone holds, so the same directory reads back
with no service at all.

`berm push` talks to no index at all — publishing is a separate act, so an
upload that succeeded is never undone by a listing that failed.
