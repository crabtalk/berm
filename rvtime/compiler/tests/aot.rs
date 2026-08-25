//! End-to-end: compile each fixture to an object, map it back, and run it.
//!
//! The point of every test here is that an artifact answers exactly what the
//! JIT answers. A relocation applied with the wrong addend still lands on a
//! real function, so "it ran" proves nothing on its own -- only the results do.

#![cfg(feature = "aot")]

use rv::Reg;
use rvtime_compiler::{Engine, Memory, Module, OptLevel, trap};
use translator::VmCtx;

const MEMORY: u64 = 16 << 20;
const STACK: u64 = 1 << 20;

/// A guest, what it can be asked, and what it needs to answer.
struct Fixture {
    name: &'static str,
    elf: &'static [u8],

    /// Calls to make, with what each must answer.
    ///
    /// The answers are spelled out rather than taken from the JIT, so that
    /// comparing the two backends cannot pass by agreeing on nonsense.
    ///
    /// Empty for `wide`, which spells out every encoding rather than a program
    /// that means anything; compiling and loading it is the whole test.
    calls: &'static [(&'static str, &'static [u64], u64)],

    /// Whether the guest reaches the host, and so needs [`host`] wired in.
    hosted: bool,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "basic",
        elf: include_bytes!("../../../fixtures/basic.elf"),
        calls: &[
            ("op_add", &[10, 3], 13),
            ("op_add", &[u64::MAX, 1], 0),
            ("op_sub", &[10, 3], 7),
            ("op_sub", &[0, 1], u64::MAX),
            ("op_mul", &[10, 3], 30),
            ("op_mul", &[u64::MAX, 2], u64::MAX.wrapping_mul(2)),
            // A loop computing a factorial.
            ("recurse", &[0], 1),
            ("recurse", &[10], 3_628_800),
            ("recurse", &[20], 2_432_902_008_176_640_000),
            ("fib", &[1], 1),
            ("fib", &[15], 610),
            // Reaches its target through the indirect table, which the loader
            // rebuilds from the artifact rather than inheriting from the JIT.
            // The index is taken modulo three, so 3 wraps back to `op_add`.
            ("dispatch", &[0, 10, 3], 13),
            ("dispatch", &[1, 10, 3], 7),
            ("dispatch", &[2, 10, 3], 30),
            ("dispatch", &[3, 10, 3], 13),
            ("switcher", &[0], 100),
            ("switcher", &[7], 807),
            ("switcher", &[9], 1009),
        ],
        hosted: false,
    },
    Fixture {
        name: "hosted",
        elf: include_bytes!("../../../fixtures/hosted.elf"),
        // `ecall` compiles to an indirect call on a pointer in `VmCtx`, so it
        // carries no relocation -- which is what makes running it worthwhile.
        calls: &[("call_add", &[10, 3], 13), ("call_tick", &[], 7)],
        hosted: true,
    },
    Fixture {
        name: "wide",
        elf: include_bytes!("../../../fixtures/wide.elf"),
        calls: &[],
        hosted: false,
    },
];

/// Stands in for a host, answering the two calls the fixture makes.
extern "C" fn host(ctx: *mut VmCtx) -> u64 {
    let ctx = unsafe { &mut *ctx };
    let (number, a0, a1) = (
        ctx.regs[Reg::A7.index()],
        ctx.regs[Reg::A0.index()],
        ctx.regs[Reg::A1.index()],
    );

    ctx.regs[Reg::A0.index()] = match number {
        1 => a0.wrapping_add(a1),
        4 => 7,
        _ => 0,
    };
    0
}

/// A compiled fixture with its memory, ready to call into.
struct Guest {
    module: Module,
    memory: Memory,
    ctx: VmCtx,
}

impl Guest {
    #[cfg(feature = "jit")]
    fn jit(fixture: &Fixture) -> Guest {
        let program = rv::elf::load(fixture.elf).expect("loads");
        let module = Module::new(&Engine::default(), program, MEMORY, false).expect("compiles");
        Guest::new(fixture, module)
    }

    fn aot(fixture: &Fixture) -> Guest {
        let module = Module::load(&Engine::default(), &artifact(fixture), MEMORY, false)
            .unwrap_or_else(|e| panic!("{} failed to load: {e:#}", fixture.name));
        Guest::new(fixture, module)
    }

    fn new(fixture: &Fixture, module: Module) -> Guest {
        let memory = Memory::new(module.program(), MEMORY, STACK).expect("maps");
        let ctx = VmCtx {
            regs: [0; 32],
            memory: memory.base(),
            dispatch: module.dispatch().as_ptr(),
            dispatch_len: module.dispatch().len() as u64,
            text_base: module.program().text.start,
            host_call: if fixture.hosted {
                host as *const u8
            } else {
                std::ptr::null()
            },
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

    /// Make every call the fixture defines, checking each answer.
    fn exercise(&mut self, fixture: &Fixture) -> Vec<u64> {
        fixture
            .calls
            .iter()
            .map(|(name, args, expected)| {
                let answer = self.call(name, args);
                assert_eq!(answer, *expected, "{}::{name}{args:?}", fixture.name);
                answer
            })
            .collect()
    }
}

fn artifact(fixture: &Fixture) -> Vec<u8> {
    let program = rv::elf::load(fixture.elf).expect("loads");
    Module::object(&Engine::default(), &program, fixture.elf, MEMORY, false)
        .unwrap_or_else(|e| panic!("{} failed to compile: {e:#}", fixture.name))
}

/// What a module exposes, in a form that does not depend on where it landed:
/// the pointers differ between backends, what they describe must not.
#[cfg(feature = "jit")]
fn shape(module: &Module) -> (Vec<u64>, Vec<bool>, u64) {
    let entries = module
        .program()
        .functions
        .keys()
        .filter(|addr| module.entry_at(**addr).is_some())
        .copied()
        .collect();
    let dispatch = module.dispatch().iter().map(|p| !p.is_null()).collect();
    (entries, dispatch, module.program().entry)
}

/// An artifact runs, and answers what the guest was written to answer.
#[test]
fn an_artifact_answers_correctly() {
    for fixture in FIXTURES.iter().filter(|f| !f.calls.is_empty()) {
        Guest::aot(fixture).exercise(fixture);
    }
}

/// The whole point: an artifact is not merely runnable, it is the same code.
#[cfg(feature = "jit")]
#[test]
fn an_artifact_answers_what_the_jit_answers() {
    for fixture in FIXTURES.iter().filter(|f| !f.calls.is_empty()) {
        let jit = Guest::jit(fixture).exercise(fixture);
        let aot = Guest::aot(fixture).exercise(fixture);
        assert_eq!(jit, aot, "{}", fixture.name);
    }
}

/// `wide` cannot be run, so this is what covers it: every RV64IMAC encoding
/// survives being written to an object and mapped back.
#[cfg(feature = "jit")]
#[test]
fn every_fixture_loads_the_module_the_jit_built() {
    for fixture in FIXTURES {
        assert_eq!(
            shape(&Guest::jit(fixture).module),
            shape(&Guest::aot(fixture).module),
            "{}",
            fixture.name
        );
    }
}

#[test]
fn the_artifact_carries_its_own_guest_image() {
    // Nothing but the artifact is handed to `load`, so the program it reports
    // has to have come from inside it.
    for fixture in FIXTURES {
        let direct = rv::elf::load(fixture.elf).expect("loads");
        let carried = Guest::aot(fixture);
        let carried = carried.module.program();

        assert_eq!(
            carried.functions.len(),
            direct.functions.len(),
            "{}",
            fixture.name
        );
        assert_eq!(carried.entry, direct.entry, "{}", fixture.name);
        assert_eq!(carried.symbols, direct.symbols, "{}", fixture.name);
    }
}

/// The fixture with the most call sites, so a refusal is tested against the
/// artifact that has the most to go wrong.
fn hosted() -> &'static Fixture {
    &FIXTURES[1]
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
    let error = refused(&Engine::default(), &artifact(hosted()), MEMORY * 2, false);
    assert!(error.contains("address space"), "unhelpful error: {error}");
}

#[test]
fn refuses_a_different_interrupt_setting() {
    let error = refused(&Engine::default(), &artifact(hosted()), MEMORY, true);
    assert!(error.contains("interruptible"), "unhelpful error: {error}");
}

#[test]
fn refuses_different_target_settings() {
    let engine = Engine::new(OptLevel::Speed).expect("builds");
    let error = refused(&engine, &artifact(hosted()), MEMORY, false);
    assert!(
        error.contains("target settings"),
        "unhelpful error: {error}"
    );
}

/// An artifact is a file, so it can be truncated or overwritten.
///
/// The guarantee is not that every damaged file is rejected -- lopping the last
/// byte off a string table changes nothing the loader reads. It is that a
/// damaged file never becomes code that runs and answers wrongly, which is the
/// failure that would go unnoticed.
#[test]
fn damage_never_answers_wrongly() {
    let fixture = &FIXTURES[0];
    let artifact = artifact(fixture);
    let expected = Guest::aot(fixture).exercise(fixture);

    let check = |bytes: &[u8], what: &str| {
        if let Ok(module) = Module::load(&Engine::default(), bytes, MEMORY, false) {
            assert_eq!(
                Guest::new(fixture, module).exercise(fixture),
                expected,
                "{what}"
            );
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
