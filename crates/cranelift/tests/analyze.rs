//! Control-flow analysis: block boundaries and what each transfer resolves to.

use rvtime_cranelift::{Analysis, Target, analyze};
use std::collections::BTreeSet;

fn program() -> rv::Program {
    rv::elf::load(include_bytes!("../../../fixtures/basic.elf")).expect("loads")
}

fn entries() -> BTreeSet<u64> {
    program().functions.keys().copied().collect()
}

fn function(name: &str) -> rv::Function {
    let mut program = program();
    let addr = program.symbols[name];
    program.functions.remove(&addr).expect("function")
}

fn analyse(f: &rv::Function) -> Analysis {
    analyze(f, &entries())
}

#[test]
fn a_leaf_function_is_one_block_ending_in_a_return() {
    let op_add = function("op_add");
    let analysis = analyse(&op_add);

    assert_eq!(analysis.leaders.len(), 1);
    assert!(analysis.calls.is_empty());

    let (ret_addr, _) = op_add.code.last().expect("has code");
    assert_eq!(analysis.targets.get(ret_addr), Some(&Target::Return));
}

#[test]
fn auipc_jalr_pairs_resolve_to_direct_calls() {
    // `_start` calls seven functions, every one of them through an auipc/jalr
    // pair rather than a plain `jal`.
    let start = function("_start");
    let analysis = analyse(&start);
    let program = program();

    let mut called: Vec<_> = analysis
        .calls
        .iter()
        .map(|addr| {
            program
                .function_at(*addr)
                .map(|f| f.name.as_str())
                .unwrap_or("<unknown>")
        })
        .collect();
    called.sort_unstable();

    assert_eq!(
        called,
        vec![
            "bump", "dispatch", "divides", "fib", "recurse", "shifts", "switcher"
        ]
    );
}

#[test]
fn every_direct_call_lands_on_a_function_entry() {
    let program = program();
    let entries = entries();
    for function in program.functions.values() {
        let analysis = analyze(function, &entries);
        for addr in &analysis.calls {
            let callee = program
                .function_at(*addr)
                .unwrap_or_else(|| panic!("{} calls {addr:#x}, not a function", function.name));
            assert_eq!(
                *addr, callee.range.start,
                "{} calls into the middle of {}",
                function.name, callee.name
            );
        }
    }
}

#[test]
fn a_computed_jump_stays_indirect() {
    // `dispatch` loads a function pointer out of OPS and tail-jumps to it.
    // There is no constant to fold, so it must remain indirect.
    let dispatch = function("dispatch");
    let analysis = analyse(&dispatch);

    assert!(
        analysis
            .targets
            .values()
            .any(|t| matches!(t, Target::Indirect { tail: true })),
        "expected an indirect tail call, got {:?}",
        analysis.targets
    );
}

#[test]
fn recursion_resolves_to_a_call_to_itself() {
    // The self-call is reached through a *negative* auipc offset
    // (`auipc ra, 0x0` then `jalr ra, -0x1a(ra)`), so this also covers sign
    // extension in the pairing.
    let fib = function("fib");
    let analysis = analyse(&fib);
    assert!(
        analysis.calls.contains(&fib.range.start),
        "fib should call itself, calls: {:#x?}",
        analysis.calls
    );
}

#[test]
fn branches_split_a_function_into_blocks() {
    // LLVM flattens `recurse` into a loop, so it is a backward-branch test
    // rather than a recursion one.
    let recurse = function("recurse");
    let analysis = analyse(&recurse);

    assert!(
        analysis.calls.is_empty(),
        "the loop form of recurse makes no calls"
    );
    assert!(
        analysis.leaders.len() > 1,
        "a function with a branch must have more than one block"
    );
    for leader in &analysis.leaders {
        assert!(
            recurse.code.iter().any(|(addr, _)| addr == leader),
            "leader {leader:#x} is not an instruction boundary"
        );
    }
}

#[test]
fn a_loop_has_a_backward_edge() {
    let recurse = function("recurse");
    let analysis = analyse(&recurse);
    assert!(
        analysis
            .targets
            .iter()
            .any(|(addr, target)| matches!(target, Target::Local(dest) if dest <= addr)),
        "expected a backward branch, got {:#x?}",
        analysis.targets
    );
}
