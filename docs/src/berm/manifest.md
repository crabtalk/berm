# The Manifest

What a harness says it is: its ABI version, its tools, and when to reach for
them.

The manifest is JSON, carried in a `.berm.abi` section of the ELF rather than
behind an export. That placement is the point — reading what an image claims
must not mean running it. `Manifest::from_elf` parses the section with nothing
compiled and nothing executed, which is what an embedder assembling a prompt, or
a service listing a registry, actually needs.

```rust,ignore
let manifest = Manifest::from_elf(&elf)?;
for tool in &manifest.tools {
    println!("{}: {}", tool.name, tool.description);
}
```

A `ToolSpec` carries the tool's name, the description a model reads, and a JSON
Schema for its arguments. The `#[harness]` macro writes all three at compile
time from the module it expands.

## Usage

`Manifest::usage` is the question no single tool's description answers: when to
reach for this harness at all, and how its tools go together. An embedder puts
it in front of a model *before* it chooses between them, so it is paid on every
turn — a few lines, not a manual. `#[harness(usage_file = "…")]` reads it from a
file at compile time when it outgrows an attribute.

## Two checks at load

`Berm::deploy` refuses an image on two grounds before it can ever be offered to a
model.

A manifest built against a different `abi_version` is refused outright, rather
than dispatched into a system harness its author did not mean.

A manifest that declares a tool the ELF does not export is also refused. The
symbol table and the manifest are both in hand at load, so the disagreement is
caught there instead of surfacing mid-conversation as a missing symbol. Exports
are matched by the `berm_tool_` prefix.

Both are the same idea as compiling at deploy rather than on first call: a
broken image should fail for whoever introduced it, not for whoever happens to
use it next.
