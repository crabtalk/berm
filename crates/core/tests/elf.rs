//! Loading the `basic` fixture.
//!
//! The two things checked here cannot be recovered later in the pipeline:
//! function boundaries, and the set of addresses an indirect jump may reach.

use rvtime_core::{Inst, MAX_MEMORY_SIZE, Reg, elf};

fn program() -> rvtime_core::Program {
    elf::load(include_bytes!("../../../fixtures/basic.elf")).expect("loads")
}

#[test]
fn recovers_functions_from_the_symbol_table() {
    let program = program();
    let names: Vec<_> = program.functions.values().map(|f| f.name.as_str()).collect();

    for want in [
        "_start", "op_add", "op_sub", "op_mul", "dispatch", "switcher", "bump", "recurse",
        "shifts", "divides",
    ] {
        assert!(names.contains(&want), "missing function {want}, found {names:?}");
    }
}

#[test]
fn skips_interior_local_labels() {
    // `.Lpcrel_hi0` and friends are symbols inside real functions. Treating
    // them as functions would split a body in half.
    let program = program();
    for function in program.functions.values() {
        assert!(
            !function.name.starts_with(".L"),
            "local label {} was treated as a function",
            function.name
        );
    }
}

#[test]
fn functions_do_not_overlap() {
    let program = program();
    let mut previous: Option<(&str, std::ops::Range<u64>)> = None;
    for function in program.functions.values() {
        if let Some((name, range)) = &previous {
            assert!(
                range.end <= function.range.start,
                "{name} {range:#x?} overlaps {} {:#x?}",
                function.name,
                function.range
            );
        }
        previous = Some((&function.name, function.range.clone()));
    }
}

#[test]
fn decodes_the_leaf_functions_exactly() {
    let program = program();

    // op_add is `add a0, a0, a1` + `ret`, and nothing else.
    let add = program
        .functions
        .values()
        .find(|f| f.name == "op_add")
        .expect("op_add");
    let ops: Vec<_> = add.code.iter().map(|(_, i)| *i).collect();
    assert_eq!(
        ops,
        vec![
            Inst::Alu {
                op: rvtime_core::AluOp::Add,
                rd: Reg::A0,
                rs1: Reg::A0,
                rs2: Reg::A1
            },
            Inst::Jalr { rd: Reg::ZERO, rs1: Reg::RA, imm: 0 },
        ]
    );
}

#[test]
fn indirect_targets_come_from_relocations() {
    let program = program();

    // OPS is `[op_add, op_sub, op_mul]`, so exactly those three functions have
    // their address taken and are reachable by the `jr` in `dispatch`.
    let mut named: Vec<_> = program
        .indirect
        .iter()
        .filter_map(|addr| program.function_at(*addr).map(|f| f.name.as_str()))
        .collect();
    named.sort_unstable();

    assert_eq!(named, vec!["op_add", "op_mul", "op_sub"]);
}

#[test]
fn every_indirect_target_is_a_function_entry() {
    let program = program();
    for addr in &program.indirect {
        let function = program
            .function_at(*addr)
            .unwrap_or_else(|| panic!("indirect target {addr:#x} is not inside any function"));
        assert_eq!(
            *addr, function.range.start,
            "indirect target {addr:#x} lands inside {} rather than at its entry",
            function.name
        );
    }
}

#[test]
fn entry_and_segments_fit_the_guest_address_space() {
    let program = program();
    assert!(program.text.contains(&program.entry));

    for segment in &program.segments {
        assert!(
            segment.addr + segment.size <= MAX_MEMORY_SIZE,
            "segment at {:#x} escapes the largest guest address space",
            segment.addr
        );
    }
    assert!(!program.segments.is_empty());
}

#[test]
fn exposes_symbols_for_lookup() {
    let program = program();
    let add = program.symbols.get("op_add").copied().expect("op_add symbol");
    assert_eq!(program.function_at(add).map(|f| f.name.as_str()), Some("op_add"));
}
