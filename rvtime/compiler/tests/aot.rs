//! End-to-end: compile a fixture to an object, map it back, and run it.
//!
//! The point of every test here is that an artifact answers exactly what the
//! JIT answers. A relocation applied with the wrong addend still lands on a
//! real function, so "it ran" proves nothing on its own -- only the results do.

#![cfg(feature = "aot")]

use rv::Reg;
use rvtime_compiler::{Engine, Memory, Module, OptLevel, trap};
use translator::VmCtx;

const ELF: &[u8] = include_bytes!("../../../fixtures/basic.elf");
const MEMORY: u64 = 16 << 20;
const STACK: u64 = 1 << 20;

/// A compiled fixture with its memory, ready to call into.
struct Guest {
    module: Module,
    memory: Memory,
    ctx: VmCtx,
}

impl Guest {
    #[cfg(feature = "jit")]
    fn jit() -> Guest {
        let program = rv::elf::load(ELF).expect("loads");
        Guest::new(Module::new(&Engine::default(), program, MEMORY, false).expect("compiles"))
    }

    fn aot() -> Guest {
        Guest::new(Module::load(&Engine::default(), &artifact(), MEMORY, false).expect("loads"))
    }

    fn new(module: Module) -> Guest {
        let memory = Memory::new(module.program(), MEMORY, STACK).expect("maps");
        let ctx = VmCtx {
            regs: [0; 32],
            memory: memory.base(),
            dispatch: module.dispatch().as_ptr(),
            dispatch_len: module.dispatch().len() as u64,
            text_base: module.program().text.start,
            host_call: std::ptr::null(),
            host_data: std::ptr::null_mut(),
            interrupt: std::ptr::null(),
            trap: 0,
            detail: 0,
        };

        trap::set_guest_region(memory.base() as usize, memory.size());
        Guest {
            module,
            memory,
            ctx,
        }
    }

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

fn artifact() -> Vec<u8> {
    let program = rv::elf::load(ELF).expect("loads");
    Module::object(&Engine::default(), &program, ELF, MEMORY, false).expect("compiles")
}

/// Every call these make crosses a relocation: `recurse` and `fib` recurse,
/// and `dispatch` reaches its target through the indirect table.
fn exercise(guest: &mut Guest) -> Vec<u64> {
    let mut out = Vec::new();
    for args in [[10, 3], [u64::MAX, 1], [0, 1]] {
        for op in ["op_add", "op_sub", "op_mul"] {
            out.push(guest.call(op, &args));
        }
    }
    for n in [0u64, 1, 2, 5, 10, 20] {
        out.push(guest.call("recurse", &[n]));
    }
    for n in [0u64, 1, 5, 15] {
        out.push(guest.call("fib", &[n]));
    }
    // `dispatch` tail-jumps through the indirect table, which the loader
    // rebuilds from the artifact rather than inheriting from the JIT.
    for index in 0..4u64 {
        out.push(guest.call("dispatch", &[index, 10, 3]));
    }
    for x in 0..10u64 {
        out.push(guest.call("switcher", &[x]));
    }
    out
}

/// The whole point: an artifact is not merely runnable, it is the same code.
#[cfg(feature = "jit")]
#[test]
fn an_artifact_answers_what_the_jit_answers() {
    let jit = exercise(&mut Guest::jit());
    let aot = exercise(&mut Guest::aot());
    assert_eq!(jit, aot);
}

#[test]
fn the_artifact_carries_its_own_guest_image() {
    // Nothing but the artifact is handed to `load`, so the program it reports
    // has to have come from inside it.
    let direct = rv::elf::load(ELF).expect("loads");
    let carried = Guest::aot();
    let carried = carried.module.program();

    assert_eq!(carried.functions.len(), direct.functions.len());
    assert_eq!(carried.entry, direct.entry);
    assert_eq!(carried.symbols, direct.symbols);
}

/// Load an artifact that must not be accepted, and report why it was not.
fn refused(engine: &Engine, artifact: &[u8], memory_size: u64, interruptible: bool) -> String {
    match Module::load(engine, artifact, memory_size, interruptible) {
        Ok(_) => panic!("the artifact should have been refused"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn refuses_a_different_address_space() {
    let error = refused(&Engine::default(), &artifact(), MEMORY * 2, false);
    assert!(error.contains("address space"), "unhelpful error: {error}");
}

#[test]
fn refuses_a_different_interrupt_setting() {
    let error = refused(&Engine::default(), &artifact(), MEMORY, true);
    assert!(error.contains("interruptible"), "unhelpful error: {error}");
}

#[test]
fn refuses_different_target_settings() {
    let engine = Engine::new(OptLevel::Speed).expect("builds");
    let error = refused(&engine, &artifact(), MEMORY, false);
    assert!(
        error.contains("target settings"),
        "unhelpful error: {error}"
    );
}

/// An artifact is a file, so other things can truncate or overwrite it.
///
/// The guarantee is not that every damaged file is rejected -- lopping the last
/// byte off a string table changes nothing the loader reads. It is that a
/// damaged file never becomes code that runs and answers wrongly, which is the
/// failure that would go unnoticed.
#[test]
fn damage_never_answers_wrongly() {
    let artifact = artifact();
    let expected = exercise(&mut Guest::aot());

    let check = |bytes: &[u8], what: &str| {
        if let Ok(module) = Module::load(&Engine::default(), bytes, MEMORY, false) {
            assert_eq!(exercise(&mut Guest::new(module)), expected, "{what}");
        }
    };

    for cut in [0, 1, 64, artifact.len() / 2, artifact.len() - 1] {
        check(&artifact[..cut], &format!("truncated to {cut}"));
    }
    check(&vec![0xa5u8; artifact.len()], "overwritten with garbage");

    let mut damaged = artifact.clone();
    for byte in (0..damaged.len()).step_by(17) {
        damaged[byte] ^= 0xff;
        check(&damaged, &format!("byte {byte} flipped"));
        damaged[byte] = artifact[byte];
    }
}
