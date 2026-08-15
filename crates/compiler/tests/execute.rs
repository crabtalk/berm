//! End-to-end: compile the fixture and run guest functions.

use rv::Reg;
use rvtime_compiler::{Engine, Memory, Module, trap};
use translator::VmCtx;

const MEMORY: u64 = 16 << 20;
const STACK: u64 = 1 << 20;

/// A compiled fixture with its memory, ready to call into.
struct Guest {
    module: Module,
    memory: Memory,
    ctx: VmCtx,
}

impl Guest {
    fn new() -> Guest {
        let program = rv::elf::load(include_bytes!("../../../fixtures/basic.elf")).expect("loads");
        let memory = Memory::new(&program, MEMORY, STACK).expect("maps");
        let module = Module::new(&Engine::default(), program, MEMORY).expect("compiles");

        let ctx = VmCtx {
            regs: [0; 32],
            memory: memory.base(),
            dispatch: module.dispatch().as_ptr(),
            dispatch_len: module.dispatch().len() as u64,
            text_base: module.program().text.start,
            host_call: std::ptr::null(),
            host_data: std::ptr::null_mut(),
            trap: 0,
        };

        trap::set_guest_region(memory.base() as usize, memory.size());
        Guest {
            module,
            memory,
            ctx,
        }
    }

    /// Call an exported function with the given arguments, returning `a0`.
    fn call(&mut self, name: &str, args: &[u64]) -> u64 {
        let entry = self
            .module
            .entry(name)
            .unwrap_or_else(|| panic!("no compiled entry for {name}"));

        self.ctx.regs = [0; 32];
        self.ctx.trap = 0;
        self.ctx.regs[Reg::SP.index()] = self.memory.stack_pointer();
        for (index, arg) in args.iter().enumerate() {
            self.ctx.regs[Reg::A0.index() + index] = *arg;
        }

        let enter: extern "C" fn(*mut VmCtx, *const u8) =
            unsafe { std::mem::transmute(self.module.trampoline()) };
        let ctx = &raw mut self.ctx;

        trap::protect(|| enter(ctx, entry))
            .unwrap_or_else(|fault| panic!("{name} faulted at guest {:x?}", fault.guest));

        assert_eq!(self.ctx.trap, 0, "{name} trapped");
        self.ctx.regs[Reg::A0.index()]
    }
}

#[test]
fn leaf_arithmetic() {
    let mut guest = Guest::new();
    assert_eq!(guest.call("op_add", &[10, 3]), 13);
    assert_eq!(guest.call("op_sub", &[10, 3]), 7);
    assert_eq!(guest.call("op_mul", &[10, 3]), 30);
}

#[test]
fn wrapping_matches_the_guest_semantics() {
    let mut guest = Guest::new();
    assert_eq!(guest.call("op_add", &[u64::MAX, 1]), 0);
    assert_eq!(guest.call("op_sub", &[0, 1]), u64::MAX);
    assert_eq!(
        guest.call("op_mul", &[u64::MAX, 2]),
        u64::MAX.wrapping_mul(2)
    );
}

#[test]
fn branches_and_loops() {
    // `recurse` is a loop computing a factorial.
    let mut guest = Guest::new();
    let factorial = |n: u64| (1..=n).product::<u64>().max(1);

    for n in [0u64, 1, 2, 5, 10, 20] {
        assert_eq!(guest.call("recurse", &[n]), factorial(n), "recurse({n})");
    }
}

#[test]
fn direct_calls_and_real_recursion() {
    let mut guest = Guest::new();

    fn fib(n: u64) -> u64 {
        if n < 2 {
            n
        } else {
            fib(n - 1).wrapping_add(fib(n - 2))
        }
    }

    for n in [0u64, 1, 2, 10, 20] {
        assert_eq!(guest.call("fib", &[n]), fib(n), "fib({n})");
    }
}

#[test]
fn switch_heavy_code() {
    let mut guest = Guest::new();
    let expected = |x: u64| match x {
        0 => 100,
        1 => 201,
        2 => 302,
        3 => 403,
        4 => 504,
        5 => 605,
        6 => 706,
        7 => 807,
        8 => 908,
        9 => 1009,
        10 => 1110,
        11 => 1211,
        12 => 1312,
        13 => 1413,
        14 => 1514,
        15 => 1615,
        16 => 1716,
        17 => 1817,
        _ => 0,
    };

    for x in 0..20u64 {
        assert_eq!(guest.call("switcher", &[x]), expected(x), "switcher({x})");
    }
}

#[test]
fn shifts_match_riscv_semantics() {
    let mut guest = Guest::new();
    let expected = |a: u64, b: u64| {
        let x = a << (b & 63);
        let y = a >> (b & 63);
        let z = ((a as i64) >> (b & 63)) as u64;
        let w = (a as u32).wrapping_shl(b as u32) as u64;
        x ^ y ^ z ^ w
    };

    for (a, b) in [
        (0x1234_5678_9abc_def0u64, 13u64),
        (u64::MAX, 0),
        (u64::MAX, 63),
        (1, 64),
        (0x8000_0000_0000_0000, 1),
    ] {
        assert_eq!(
            guest.call("shifts", &[a, b]),
            expected(a, b),
            "shifts({a:#x}, {b})"
        );
    }
}

#[test]
fn division_matches_riscv_semantics() {
    let mut guest = Guest::new();
    let expected = |a: u64, b: u64| {
        let s = (a as i64).wrapping_div(b as i64 | 1) as u64;
        let u = a / (b | 1);
        let r = (a as i64).wrapping_rem(b as i64 | 1) as u64;
        let m = a % (b | 1);
        s ^ u ^ r ^ m
    };

    for (a, b) in [
        (1_000_003u64, 97u64),
        (0, 0),
        (u64::MAX, 0),
        (i64::MIN as u64, u64::MAX),
        (12345, 6789),
    ] {
        assert_eq!(
            guest.call("divides", &[a, b]),
            expected(a, b),
            "divides({a}, {b})"
        );
    }
}

#[test]
fn atomics_read_modify_write_guest_memory() {
    let mut guest = Guest::new();

    // `bump` is a fetch_add on a global, so the return value is the previous
    // total and the effect has to persist in guest memory across calls.
    assert_eq!(guest.call("bump", &[5]), 0);
    assert_eq!(guest.call("bump", &[7]), 5);
    assert_eq!(guest.call("bump", &[0]), 12);
}

#[test]
fn indirect_calls_reach_the_right_function() {
    let mut guest = Guest::new();

    // `dispatch` loads a function pointer out of OPS and tail-jumps to it.
    assert_eq!(guest.call("dispatch", &[0, 10, 3]), 13);
    assert_eq!(guest.call("dispatch", &[1, 10, 3]), 7);
    assert_eq!(guest.call("dispatch", &[2, 10, 3]), 30);
    // The index is taken modulo three, so this wraps back to op_add.
    assert_eq!(guest.call("dispatch", &[3, 10, 3]), 13);
}

#[test]
fn every_function_compiles() {
    let program = rv::elf::load(include_bytes!("../../../fixtures/basic.elf")).expect("loads");
    let expected = program.functions.len();
    let module = Module::new(&Engine::default(), program, MEMORY).expect("compiles");

    for addr in module.program().functions.keys() {
        assert!(
            module.entry_at(*addr).is_some(),
            "no code emitted for {addr:#x}"
        );
    }
    assert!(expected > 5, "fixture should have several functions");
}

#[test]
fn optimised_and_unoptimised_agree() {
    let load = || rv::elf::load(include_bytes!("../../../fixtures/basic.elf")).expect("loads");

    for opt in [
        rvtime_compiler::OptLevel::None,
        rvtime_compiler::OptLevel::Speed,
    ] {
        let program = load();
        let memory = Memory::new(&program, MEMORY, STACK).expect("maps");
        let engine = Engine::new(opt).expect("engine");
        let module = Module::new(&engine, program, MEMORY).expect("compiles");

        let mut ctx = VmCtx {
            regs: [0; 32],
            memory: memory.base(),
            dispatch: module.dispatch().as_ptr(),
            dispatch_len: module.dispatch().len() as u64,
            text_base: module.program().text.start,
            host_call: std::ptr::null(),
            host_data: std::ptr::null_mut(),
            trap: 0,
        };
        trap::set_guest_region(memory.base() as usize, memory.size());
        ctx.regs[Reg::SP.index()] = memory.stack_pointer();
        ctx.regs[Reg::A0.index()] = 20;

        let enter: extern "C" fn(*mut VmCtx, *const u8) =
            unsafe { std::mem::transmute(module.trampoline()) };
        let entry = module.entry("fib").expect("fib");
        let ptr = &raw mut ctx;
        trap::protect(|| enter(ptr, entry)).expect("no fault");

        assert_eq!(ctx.regs[Reg::A0.index()], 6765, "fib(20) at {opt:?}");
    }
}

#[test]
fn the_whole_instruction_set_compiles() {
    // The `wide` fixture spells out every RV64IMAC encoding, including ones
    // LLVM never emits. Compiling it exercises every `Inst` variant through
    // the translator; the code is nonsense as a program and is never run.
    let program = rv::elf::load(include_bytes!("../../../fixtures/wide.elf")).expect("loads");
    let functions = program.functions.len();
    let module = Module::new(&Engine::default(), program, MEMORY).expect("compiles");

    assert!(
        module
            .program()
            .functions
            .contains_key(&module.program().symbols["coverage"]),
        "the coverage function should be loaded"
    );
    assert!(functions > 0);
}
