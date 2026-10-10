# The Manifest

What a program says it is: its ABI version, its tools, when to reach for them,
and what it reaches for itself.

The manifest is JSON, carried in a `.berm.abi` section of the image rather than
behind an export. That placement is the point — reading what an image claims
must not mean running it. `Manifest::from_image` parses the section with nothing
compiled and nothing executed, which is what an embedder assembling a prompt, or
a service listing a registry, actually needs.

The section is a wasm custom section, which is what `#[link_section]` emits on
`wasm32`.

```rust,ignore
let manifest = Manifest::from_image(&bytes)?;
for tool in &manifest.tools {
    println!("{}: {}", tool.name, tool.description);
}
```

A `ToolSpec` carries the tool's name, the description a model reads, and a JSON
Schema for its arguments. The `#[program]` macro writes all three at compile
time from the module it expands.

## Usage

`Manifest::usage` is the question no single tool's description answers: when to
reach for this program at all, and how its tools go together. An embedder puts
it in front of a model *before* it chooses between them, so it is paid on every
turn — a few lines, not a manual. `#[program(usage_file = "…")]` reads it from a
file at compile time when it outgrows an attribute.

## Dependencies

```rust,ignore
#[berm_lang::program(
    programs = ["weather"],
    syscalls = { "berm.get", "berm.http.fetch" = ["api.example.com"] },
    secrets = ["example"],
)]
```

is what an image says it will reach for once it runs: programs it calls by
name, syscalls it makes beyond the ABI's own, each beside what it reaches
through that call, and secrets it names as `{secret:NAME}` in a header. The
manifest carries them as
`"deps": {"programs": [...], "syscalls": {"name": [...]}, "secrets": [...], "logs": [...]}`.

A tool subscribes to another program's logs with `#[logs("hn" = ["item"])]`,
listed under `logs` as `{"program": "hn", "topics": ["item"], "tool": "..."}`.
The host calls that tool with a `Delivery` of the logs it has not had yet,
oldest first. A log is delivered when it carries any listed topic; no topics
takes every log.

`berm.http.fetch` holds a program to them: a host it does not declare, or a
secret it does not declare, is refused before anything is sent. A declared
secret is substituted by the host for the chain's caller, and the guest never
holds its value. These are runtime
and resolved wherever the image lands: nothing here is fetched, installed, or
compiled in, which is what separates them from the crate's own
`[dependencies]`.

Declared by the author rather than found by reading the image, because a target
the program computes at the call is invisible to any scan — a list built that
way would be quietly short, and the runtime `Refused` would still be needed
underneath it.

Nothing is refused for an unmet dependency. `berm deploy` says so and carries
on, and `berm inspect` marks the programs not deployed and the syscalls nothing
registers. What a served syscall reaches through it is that syscall's to grant.
Refusing would make deploy order significant, and a restart brings programs
back in whatever order the filesystem lists them — two that name each other
could then never come up at all.

## Two checks at load

`Berm::deploy` refuses an image on two grounds before it can ever be offered to a
model.

A manifest built against a different `abi_version` is refused outright, rather
than dispatched into a syscall its author did not mean.

A manifest that declares a tool the image does not export is also refused. The
symbol table and the manifest are both in hand at load, so the disagreement is
caught there instead of surfacing mid-conversation as a missing symbol. Exports
are matched by the `berm_tool_` prefix.

Both are the same idea as compiling at deploy rather than on first call: a
broken image should fail for whoever introduced it, not for whoever happens to
use it next.
