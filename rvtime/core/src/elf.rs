//! Loading a static RISC-V ELF into a [`Program`]
//!
//! Two things are recovered here that cannot be recovered later:
//!
//! - **function boundaries**, from `STT_FUNC` symbols. Interior `.L*` labels
//!   share the address space of real functions and must be filtered out.
//! - **indirect jump targets**, from the relocations that name a `.text`
//!   address — `R_RISCV_64` for one stored in data, `R_RISCV_PCREL_HI20` for
//!   one materialised into a register. This is why the guest must be linked
//!   with `--emit-relocs`; the alternative is guessing, and a wrong guess traps
//!   on correct code.

use crate::{Function, MAX_MEMORY_SIZE, Perms, Program, Segment, decode};
use anyhow::{Context, Result, bail};
use object::{
    Object, ObjectSection, ObjectSegment, ObjectSymbol, RelocationFlags, RelocationTarget,
    SectionKind, SymbolKind,
};
use std::collections::{BTreeMap, BTreeSet};

/// Load a statically linked RV64 ELF.
pub fn load(bytes: &[u8]) -> Result<Program> {
    let elf = object::File::parse(bytes).context("failed to parse ELF")?;
    if elf.architecture() != object::Architecture::Riscv64 {
        bail!("expected a riscv64 object, found {:?}", elf.architecture());
    }

    let segments = segments(&elf)?;
    let text = text_range(&elf)?;
    let symbols = symbols(&elf);
    let functions = functions(&elf, &segments, &text)?;
    if !relocated(&elf) {
        bail!(
            "guest was linked without `--emit-relocs`, so nothing says where an \
             indirect jump may land; relink it with `-Clink-arg=--emit-relocs`"
        );
    }
    let indirect = indirect_targets(&elf, &text)?;

    Ok(Program {
        entry: elf.entry(),
        segments,
        text,
        functions,
        indirect,
        symbols,
    })
}

/// Collect loadable segments, rejecting anything outside the 4 GiB window.
fn segments(elf: &object::File<'_>) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    for segment in elf.segments() {
        let (addr, size) = (segment.address(), segment.size());
        if size == 0 {
            continue;
        }

        // No configuration can map a segment beyond the largest permitted
        // address space, so reject that here. Whether the image fits the size
        // actually configured is checked when memory is mapped.
        let end = addr
            .checked_add(size)
            .context("segment address overflows")?;
        if end > MAX_MEMORY_SIZE {
            bail!(
                "segment {addr:#x}..{end:#x} exceeds the largest guest address space \
                 ({MAX_MEMORY_SIZE:#x})"
            );
        }

        let perms = segment.permissions();
        segments.push(Segment {
            addr,
            data: segment.data()?.to_vec(),
            size,
            perms: Perms {
                read: perms.readable(),
                write: perms.writable(),
                exec: perms.executable(),
            },
        });
    }

    segments.sort_by_key(|s| s.addr);
    Ok(segments)
}

/// The executable address range, taken from `.text`.
fn text_range(elf: &object::File<'_>) -> Result<std::ops::Range<u64>> {
    let text = elf
        .sections()
        .find(|s| s.kind() == SectionKind::Text && s.size() > 0)
        .context("no executable .text section")?;
    let addr = text.address();
    Ok(addr..addr + text.size())
}

/// Exported symbol addresses, for `get_typed_func`.
fn symbols(elf: &object::File<'_>) -> BTreeMap<String, u64> {
    elf.symbols()
        .filter(|s| s.is_global())
        .filter_map(|s| Some((s.name().ok()?.to_string(), s.address())))
        .collect()
}

/// Recover functions from `STT_FUNC` symbols and decode their bodies.
fn functions(
    elf: &object::File<'_>,
    segments: &[Segment],
    text: &std::ops::Range<u64>,
) -> Result<BTreeMap<u64, Function>> {
    let mut functions = BTreeMap::new();
    for symbol in elf.symbols() {
        if symbol.kind() != SymbolKind::Text || symbol.size() == 0 {
            continue;
        }
        let Ok(name) = symbol.name() else { continue };
        // `.L*` labels mark positions inside a function, not functions.
        if name.starts_with(".L") {
            continue;
        }

        let start = symbol.address();
        let end = start + symbol.size();
        if !text.contains(&start) {
            continue;
        }

        let body = read(segments, start, symbol.size())
            .with_context(|| format!("function {name} at {start:#x} is not in any segment"))?;
        let code = disassemble(body, start)
            .with_context(|| format!("failed to decode function {name} at {start:#x}"))?;

        functions.insert(
            start,
            Function {
                name: name.to_string(),
                range: start..end,
                code,
            },
        );
    }

    Ok(functions)
}

/// Decode a function body into address-tagged instructions.
fn disassemble(body: &[u8], start: u64) -> Result<Vec<(u64, crate::Inst)>> {
    let mut code = Vec::new();
    let mut offset = 0usize;
    while offset < body.len() {
        let (inst, len) =
            decode(&body[offset..]).with_context(|| format!("at {:#x}", start + offset as u64))?;
        code.push((start + offset as u64, inst));
        offset += len;
    }
    Ok(code)
}

/// Whether the linker kept its relocations.
///
/// A fully linked executable carries none unless `--emit-relocs` asked for
/// them, and without them [`indirect_targets`] returns an empty set rather than
/// an error — so a guest built without the flag loads, runs, and traps at the
/// first indirect jump instead of failing here. Any relocation at all is proof
/// the flag reached the linker; a guest with none of its own would still carry
/// the ones its direct calls left behind.
fn relocated(elf: &object::File<'_>) -> bool {
    elf.sections()
        .any(|section| section.relocations().next().is_some())
}

/// Addresses reachable by an indirect jump, from the relocations that name a
/// code address.
///
/// Two relocation types do that, and both are needed:
///
/// - `R_RISCV_64` — an address stored in data. A vtable slot, or an entry in
///   the jump table LLVM emits for a dense `match`.
/// - `R_RISCV_PCREL_HI20` — an address materialised into a register by
///   `auipc`+`addi`. This is how `core::fmt` builds the formatter pointers it
///   later calls through, so a guest that formats anything depends on it.
///
/// The `LO12` halves are deliberately absent: their relocation names the
/// `auipc` instruction rather than the address being formed, so counting them
/// would add the call site to the set of things callable. `R_RISCV_CALL_PLT`
/// is absent because a direct call is resolved statically and never reaches
/// the dispatch table.
fn indirect_targets(elf: &object::File<'_>, text: &std::ops::Range<u64>) -> Result<BTreeSet<u64>> {
    let mut targets = BTreeSet::new();
    for section in elf.sections() {
        for (_, reloc) in section.relocations() {
            let RelocationFlags::Elf { r_type } = reloc.flags() else {
                continue;
            };
            if !matches!(
                r_type,
                object::elf::R_RISCV_64 | object::elf::R_RISCV_PCREL_HI20
            ) {
                continue;
            }
            let RelocationTarget::Symbol(index) = reloc.target() else {
                continue;
            };
            let Ok(symbol) = elf.symbol_by_index(index) else {
                continue;
            };

            let addr = symbol.address().wrapping_add(reloc.addend() as u64);
            if text.contains(&addr) {
                targets.insert(addr);
            }
        }
    }

    Ok(targets)
}

/// Read `len` bytes at guest address `addr` out of the loaded segments.
fn read(segments: &[Segment], addr: u64, len: u64) -> Option<&[u8]> {
    let segment = segments
        .iter()
        .find(|s| addr >= s.addr && addr + len <= s.addr + s.size)?;
    let offset = (addr - segment.addr) as usize;
    segment.data.get(offset..offset + len as usize)
}
