//! Differential test: our decoder against LLVM's, over whole `.text` sections.
//!
//! The `.objdump` goldens are checked in so this needs no LLVM tooling; run
//! `fixtures/build.sh` to regenerate them after changing a fixture.
//!
//! Agreement on instruction *boundaries* is what catches the errors that
//! matter. A wrong compressed immediate or quadrant desynchronises the sweep
//! and every subsequent address diverges.

use object::{Object, ObjectSection, SectionKind};
use rvtime_core::{AluOp, AmoOp, Cond, Inst, LoadOp, MulOp, StoreOp, Width, decode};
use std::collections::BTreeMap;

/// Parse a golden disassembly into `addr -> (length, mnemonic)`.
fn golden(dump: &str) -> BTreeMap<u64, (usize, String)> {
    let mut out = BTreeMap::new();
    for line in dump.lines() {
        // "   111b0: 1101         \tc.addi\tsp, -0x20"
        let Some((addr, rest)) = line.split_once(':') else {
            continue;
        };
        let Ok(addr) = u64::from_str_radix(addr.trim(), 16) else {
            continue;
        };
        let mut fields = rest.split('\t');
        let Some(bytes) = fields.next().map(str::trim) else {
            continue;
        };
        if bytes.is_empty() || !bytes.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let Some(mnemonic) = fields.next().map(str::trim) else {
            continue;
        };

        out.insert(addr, (bytes.len() / 2, normalize(mnemonic)));
    }

    assert!(!out.is_empty(), "golden disassembly parsed as empty");
    out
}

/// Fold LLVM's mnemonic into the canonical form our decoder produces.
///
/// Compressed instructions expand into their 32-bit equivalents, and the
/// acquire/release suffix on atomics is decoded into a field rather than the
/// opcode.
fn normalize(mnemonic: &str) -> String {
    let m = mnemonic
        .trim_end_matches(".aqrl")
        .trim_end_matches(".aq")
        .trim_end_matches(".rl");

    match m {
        "c.addi" | "c.nop" | "c.li" | "c.addi16sp" | "c.addi4spn" => "addi",
        "c.addiw" => "addiw",
        "c.lui" => "lui",
        "c.slli" => "slli",
        "c.srli" => "srli",
        "c.srai" => "srai",
        "c.andi" => "andi",
        "c.mv" | "c.add" => "add",
        "c.sub" => "sub",
        "c.xor" => "xor",
        "c.or" => "or",
        "c.and" => "and",
        "c.subw" => "subw",
        "c.addw" => "addw",
        "c.j" => "jal",
        "c.jr" | "c.jalr" => "jalr",
        "c.beqz" => "beq",
        "c.bnez" => "bne",
        "c.ld" | "c.ldsp" => "ld",
        "c.lw" | "c.lwsp" => "lw",
        "c.sd" | "c.sdsp" => "sd",
        "c.sw" | "c.swsp" => "sw",
        "c.ebreak" => "ebreak",
        "c.unimp" => "unimp",
        "fence.i" | "fence.tso" => "fence",
        other => other,
    }
    .to_string()
}

/// The canonical mnemonic for a decoded instruction.
fn mnemonic(inst: &Inst) -> String {
    fn alu(op: AluOp, suffix: &str) -> String {
        let base = match op {
            AluOp::Add => "add",
            AluOp::Sub => "sub",
            AluOp::Sll => "sll",
            AluOp::Slt => "slt",
            AluOp::SltU => "sltu",
            AluOp::Xor => "xor",
            AluOp::Srl => "srl",
            AluOp::Sra => "sra",
            AluOp::Or => "or",
            AluOp::And => "and",
        };
        // `sltiu` keeps its trailing `u`: slt + i + u, not slt + u + i.
        match (op, suffix) {
            (AluOp::SltU, "i") => "sltiu".to_string(),
            _ => format!("{base}{suffix}"),
        }
    }

    fn mul(op: MulOp, suffix: &str) -> String {
        let base = match op {
            MulOp::Mul => "mul",
            MulOp::MulH => "mulh",
            MulOp::MulHSU => "mulhsu",
            MulOp::MulHU => "mulhu",
            MulOp::Div => "div",
            MulOp::DivU => "divu",
            MulOp::Rem => "rem",
            MulOp::RemU => "remu",
        };
        format!("{base}{suffix}")
    }

    fn width(w: Width) -> &'static str {
        match w {
            Width::W => "w",
            Width::D => "d",
        }
    }

    match inst {
        Inst::Lui { .. } => "lui".into(),
        Inst::Auipc { .. } => "auipc".into(),
        Inst::Jal { .. } => "jal".into(),
        Inst::Jalr { .. } => "jalr".into(),
        Inst::Branch { op, .. } => match op {
            Cond::Eq => "beq",
            Cond::Ne => "bne",
            Cond::Lt => "blt",
            Cond::Ge => "bge",
            Cond::LtU => "bltu",
            Cond::GeU => "bgeu",
        }
        .into(),
        Inst::Load { op, .. } => match op {
            LoadOp::B => "lb",
            LoadOp::H => "lh",
            LoadOp::W => "lw",
            LoadOp::D => "ld",
            LoadOp::Bu => "lbu",
            LoadOp::Hu => "lhu",
            LoadOp::Wu => "lwu",
        }
        .into(),
        Inst::Store { op, .. } => match op {
            StoreOp::B => "sb",
            StoreOp::H => "sh",
            StoreOp::W => "sw",
            StoreOp::D => "sd",
        }
        .into(),
        Inst::Alu { op, .. } => alu(*op, ""),
        Inst::AluImm { op, .. } => alu(*op, "i"),
        Inst::AluW { op, .. } => alu(*op, "w"),
        Inst::AluImmW { op, .. } => alu(*op, "iw"),
        Inst::Mul { op, .. } => mul(*op, ""),
        Inst::MulW { op, .. } => mul(*op, "w"),
        Inst::Amo { op, width: w, .. } => {
            let base = match op {
                AmoOp::Swap => "amoswap",
                AmoOp::Add => "amoadd",
                AmoOp::Xor => "amoxor",
                AmoOp::And => "amoand",
                AmoOp::Or => "amoor",
                AmoOp::Min => "amomin",
                AmoOp::Max => "amomax",
                AmoOp::MinU => "amominu",
                AmoOp::MaxU => "amomaxu",
            };
            format!("{base}.{}", width(*w))
        }
        Inst::LoadReserved { width: w, .. } => format!("lr.{}", width(*w)),
        Inst::StoreConditional { width: w, .. } => format!("sc.{}", width(*w)),
        Inst::Fence => "fence".into(),
        Inst::Ecall => "ecall".into(),
        Inst::Ebreak => "ebreak".into(),
        Inst::Unimp => "unimp".into(),
    }
}

/// Sweep every executable section and check each instruction against LLVM.
fn differential(name: &str, elf: &[u8], dump: &str) {
    let file = object::File::parse(elf).expect("parses");
    let mut expected = golden(dump);
    let mut decoded = 0usize;

    for section in file.sections() {
        if section.kind() != SectionKind::Text || section.size() == 0 {
            continue;
        }
        let base = section.address();
        let code = section.data().expect("readable");

        let mut offset = 0usize;
        while offset < code.len() {
            let addr = base + offset as u64;
            let (inst, len) = decode(&code[offset..])
                .unwrap_or_else(|e| panic!("[{name}] failed to decode at {addr:#x}: {e}"));

            // A missing address means the sweep desynchronised: we started
            // decoding at an offset LLVM never treated as an instruction.
            let (want_len, want_mnemonic) = expected.remove(&addr).unwrap_or_else(|| {
                panic!("[{name}] decoded {inst:?} at {addr:#x}, which LLVM did not list")
            });

            assert_eq!(
                len, want_len,
                "[{name}] length mismatch at {addr:#x} for {inst:?}"
            );
            assert_eq!(
                mnemonic(&inst),
                want_mnemonic,
                "[{name}] opcode mismatch at {addr:#x}: decoded {inst:?}"
            );

            offset += len;
            decoded += 1;
        }
    }

    assert!(
        expected.is_empty(),
        "[{name}] {} instruction(s) LLVM found were never decoded, first at {:#x}",
        expected.len(),
        expected.keys().next().unwrap()
    );
    assert!(decoded > 0, "[{name}] decoded nothing");
}

#[test]
fn basic_agrees_with_llvm() {
    differential(
        "basic",
        include_bytes!("../../../fixtures/basic.elf"),
        include_str!("../../../fixtures/basic.objdump"),
    );
}

#[test]
fn wide_agrees_with_llvm() {
    differential(
        "wide",
        include_bytes!("../../../fixtures/wide.elf"),
        include_str!("../../../fixtures/wide.objdump"),
    );
}
