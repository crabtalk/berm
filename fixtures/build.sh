#!/usr/bin/env bash
# Rebuild the checked-in RISC-V ELF fixtures.
#
# The artifacts are committed so `cargo test` needs no RISC-V toolchain. Run
# this only when a fixture's source changes.
#
#   rustup target add riscv64imac-unknown-none-elf
#   ./fixtures/build.sh
set -euo pipefail

cd "$(dirname "$0")"
target=riscv64imac-unknown-none-elf

# --emit-relocs keeps the relocations that identify indirect-call targets.
# Without it the loader cannot tell which functions have their address taken.
flags='["-Clink-arg=--emit-relocs"]'

objdump="$(dirname "$(rustc --print sysroot)/x")/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-objdump"

for dir in */; do
    name="${dir%/}"
    [ -f "$name/Cargo.toml" ] || continue

    echo "building $name"
    (cd "$name" && cargo build --release --target "$target" \
        --config "target.$target.rustflags=$flags")
    cp "$name/target/$target/release/$name" "$name.elf"
    echo "  -> fixtures/$name.elf"

    # Golden disassembly for the decoder differential test. -M no-aliases keeps
    # canonical mnemonics instead of pseudo-instructions like `ret` and `li`.
    if [ -x "$objdump" ]; then
        "$objdump" -d -M no-aliases "$name.elf" > "$name.objdump"
        echo "  -> fixtures/$name.objdump"
    else
        echo "  !! llvm-objdump not found; run: rustup component add llvm-tools-preview" >&2
    fi
done
