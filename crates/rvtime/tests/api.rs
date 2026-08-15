//! The embedder-facing API, exercised the way a user would.

use rvtime::{Caller, Config, Engine, Linker, Module, OptLevel, Reg, Store, Trap};

const BASIC: &[u8] = include_bytes!("../../../fixtures/basic.elf");
const HOSTED: &[u8] = include_bytes!("../../../fixtures/hosted.elf");

/// What the host tracks across calls in these tests.
#[derive(Default)]
struct Host {
    ticks: u64,
    log: Vec<String>,
}

fn basic() -> (Engine, Module) {
    let engine = Engine::default();
    let module = Module::new(&engine, BASIC).expect("compiles");
    (engine, module)
}

#[test]
fn calling_an_exported_function() {
    let (engine, module) = basic();
    let mut store = Store::new(&engine, ());
    let linker = Linker::new(&engine);
    let instance = linker.instantiate(&mut store, &module).expect("instantiates");

    let add = instance
        .get_typed_func::<(u64, u64), u64>("op_add")
        .expect("op_add");
    assert_eq!(add.call(&mut store, (10, 3)).unwrap(), 13);

    let sub = instance
        .get_typed_func::<(u64, u64), u64>("op_sub")
        .expect("op_sub");
    assert_eq!(sub.call(&mut store, (10, 3)).unwrap(), 7);

    // A function handle is reusable across calls.
    assert_eq!(add.call(&mut store, (1, 2)).unwrap(), 3);
    assert_eq!(add.call(&mut store, (100, 200)).unwrap(), 300);
}

#[test]
fn a_missing_export_is_an_error() {
    let (engine, module) = basic();
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &module)
        .expect("instantiates");

    let error = instance
        .get_typed_func::<(u64,), u64>("nope")
        .expect_err("should not resolve");
    assert!(error.to_string().contains("nope"), "{error}");
}

#[test]
fn exports_are_listed() {
    let (_engine, module) = basic();
    let names: Vec<_> = module.exports().collect();
    for want in ["op_add", "op_sub", "op_mul", "dispatch", "fib"] {
        assert!(names.contains(&want), "missing {want} in {names:?}");
    }
}

#[test]
fn calling_before_instantiate_fails() {
    let (engine, module) = basic();
    let mut store = Store::new(&engine, ());
    let mut other = Store::new(&engine, ());

    let instance = Linker::new(&engine)
        .instantiate(&mut store, &module)
        .expect("instantiates");
    let add = instance
        .get_typed_func::<(u64, u64), u64>("op_add")
        .expect("op_add");

    // `other` was never instantiated.
    let error = add.call(&mut other, (1, 2)).expect_err("should refuse");
    assert!(error.to_string().contains("no instance"), "{error}");
}

#[test]
fn typed_host_functions() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let mut linker = Linker::new(&engine);

    linker
        .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| Ok(a.wrapping_add(b)))
        .unwrap();
    linker
        .func_wrap(4, |mut caller: Caller<'_, Host>| {
            caller.data_mut().ticks += 1;
            Ok(caller.data().ticks)
        })
        .unwrap();

    let instance = linker.instantiate(&mut store, &module).expect("instantiates");

    let add = instance
        .get_typed_func::<(u64, u64), u64>("call_add")
        .expect("call_add");
    assert_eq!(add.call(&mut store, (20, 22)).unwrap(), 42);

    let tick = instance.get_typed_func::<(), u64>("call_tick").expect("call_tick");
    assert_eq!(tick.call(&mut store, ()).unwrap(), 1);
    assert_eq!(tick.call(&mut store, ()).unwrap(), 2);
    assert_eq!(store.data().ticks, 2);
}

#[test]
fn host_functions_read_guest_memory() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let mut linker = Linker::new(&engine);

    linker
        .func_wrap(2, |caller: Caller<'_, Host>, ptr: u64, len: u64| {
            let bytes = caller.read(ptr, len)?;
            Ok(bytes.iter().map(|b| *b as u64).sum::<u64>())
        })
        .unwrap();

    let instance = linker.instantiate(&mut store, &module).expect("instantiates");
    let round_trip = instance
        .get_typed_func::<(u64,), u64>("round_trip")
        .expect("round_trip");

    // The guest fills a buffer with 1..=n, then asks the host to sum it.
    for n in [0u64, 1, 5, 32] {
        let expected = (1..=n).sum::<u64>();
        assert_eq!(round_trip.call(&mut store, (n,)).unwrap(), expected, "n={n}");
    }
}

#[test]
fn host_functions_write_guest_memory() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let mut linker = Linker::new(&engine);

    linker
        .func_wrap(3, |mut caller: Caller<'_, Host>, ptr: u64, len: u64| {
            caller.write(ptr, &vec![0xab; len as usize])?;
            Ok(len)
        })
        .unwrap();
    linker
        .func_wrap(2, |caller: Caller<'_, Host>, ptr: u64, len: u64| {
            let bytes = caller.read(ptr, len)?;
            Ok(bytes.iter().map(|b| *b as u64).sum::<u64>())
        })
        .unwrap();

    let instance = linker.instantiate(&mut store, &module).expect("instantiates");
    let fill = instance
        .get_typed_func::<(u64, u64), u64>("call_fill")
        .expect("call_fill");
    let sum = instance
        .get_typed_func::<(u64, u64), u64>("call_sum")
        .expect("call_sum");

    // Use the guest's BUFFER global as the target.
    let addr = guest_symbol("BUFFER");
    assert_eq!(fill.call(&mut store, (addr, 8)).unwrap(), 8);
    assert_eq!(sum.call(&mut store, (addr, 8)).unwrap(), 0xab * 8);

    // The host's write is visible through the store too.
    assert_eq!(store.read(addr, 8).unwrap(), &[0xab; 8]);
}

#[test]
fn registers_survive_a_host_call() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let mut linker = Linker::new(&engine);

    linker
        .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| Ok(a.wrapping_add(b)))
        .unwrap();

    let instance = linker.instantiate(&mut store, &module).expect("instantiates");
    let mixed = instance
        .get_typed_func::<(u64, u64), u64>("call_mixed")
        .expect("call_mixed");

    // call_mixed interleaves guest arithmetic with two host calls, so values
    // held in registers across `ecall` have to come back intact.
    let expected = |a: u64, b: u64| {
        let x = a.wrapping_mul(3);
        let y = a.wrapping_add(b);
        let z = b.wrapping_add(7);
        let w = y.wrapping_add(z);
        x.wrapping_add(w)
    };
    for (a, b) in [(1u64, 2u64), (100, 200), (u64::MAX, 3)] {
        assert_eq!(mixed.call(&mut store, (a, b)).unwrap(), expected(a, b));
    }
}

#[test]
fn a_failing_host_call_stops_the_guest() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let mut linker = Linker::new(&engine);

    linker
        .func_wrap(5, |_: Caller<'_, Host>, _x: u64| -> anyhow::Result<u64> {
            anyhow::bail!("refused on purpose")
        })
        .unwrap();

    let instance = linker.instantiate(&mut store, &module).expect("instantiates");
    let refused = instance
        .get_typed_func::<(u64,), u64>("call_refused")
        .expect("call_refused");

    let error = refused.call(&mut store, (1,)).expect_err("should fail");
    assert!(error.to_string().contains("refused on purpose"), "{error}");
}

#[test]
fn an_unregistered_call_number_traps() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &module)
        .expect("instantiates");

    let unknown = instance
        .get_typed_func::<(), u64>("call_unknown")
        .expect("call_unknown");

    let error = unknown.call(&mut store, ()).expect_err("should trap");
    let trap = error.downcast_ref::<Trap>().expect("a Trap");
    assert!(matches!(trap, Trap::UnknownHostCall(99)), "{trap}");
}

#[test]
fn raw_register_access() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let mut linker = Linker::new(&engine);

    // The untyped form, reading and writing registers directly.
    linker
        .func(1, |mut caller: Caller<'_, Host>| {
            let a = caller.reg(Reg::A0);
            let b = caller.reg(Reg::A1);
            caller.data_mut().log.push(format!("add({a}, {b})"));
            caller.set_reg(Reg::A0, a.wrapping_add(b));
            Ok(())
        })
        .unwrap();

    let instance = linker.instantiate(&mut store, &module).expect("instantiates");
    let add = instance
        .get_typed_func::<(u64, u64), u64>("call_add")
        .expect("call_add");

    assert_eq!(add.call(&mut store, (2, 3)).unwrap(), 5);
    assert_eq!(store.data().log, vec!["add(2, 3)".to_string()]);
}

#[test]
fn running_from_the_entry_point() {
    let engine = Engine::default();
    let module = Module::new(&engine, HOSTED).expect("compiles");
    let mut store = Store::new(&engine, Host::default());
    let mut linker = Linker::new(&engine);

    linker
        .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| Ok(a.wrapping_add(b)))
        .unwrap();
    linker
        .func_wrap(4, |mut caller: Caller<'_, Host>| {
            caller.data_mut().ticks += 1;
            Ok(caller.data().ticks)
        })
        .unwrap();

    let instance = linker.instantiate(&mut store, &module).expect("instantiates");
    instance.run(&mut store).expect("runs to completion");

    // `_start` computes call_add(20, 22) + call_tick() and stores it.
    let addr = guest_symbol("RESULT");
    let result = u64::from_le_bytes(store.read(addr, 8).unwrap().try_into().unwrap());
    assert_eq!(result, 43);
    assert_eq!(store.data().ticks, 1);
}

#[test]
fn optimisation_level_is_configurable() {
    for level in [OptLevel::None, OptLevel::Speed] {
        let mut config = Config::new();
        config.opt_level(level).stack_size(2 << 20);

        let engine = Engine::new(&config).expect("engine");
        let module = Module::new(&engine, BASIC).expect("compiles");
        let mut store = Store::new(&engine, ());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");

        let fib = instance.get_typed_func::<(u64,), u64>("fib").expect("fib");
        assert_eq!(fib.call(&mut store, (20,)).unwrap(), 6765, "{level:?}");
    }
}

#[test]
fn stores_are_independent() {
    let (engine, module) = basic();
    let linker = Linker::new(&engine);

    let mut first = Store::new(&engine, ());
    let mut second = Store::new(&engine, ());
    let a = linker.instantiate(&mut first, &module).expect("instantiates");
    let b = linker.instantiate(&mut second, &module).expect("instantiates");

    let bump_a = a.get_typed_func::<(u64,), u64>("bump").expect("bump");
    let bump_b = b.get_typed_func::<(u64,), u64>("bump").expect("bump");

    // `bump` accumulates into a guest global, so separate stores must not see
    // each other's writes.
    assert_eq!(bump_a.call(&mut first, (5,)).unwrap(), 0);
    assert_eq!(bump_a.call(&mut first, (5,)).unwrap(), 5);
    assert_eq!(bump_b.call(&mut second, (7,)).unwrap(), 0);
    assert_eq!(bump_a.call(&mut first, (0,)).unwrap(), 10);
}

#[test]
fn host_functions_cannot_be_added_after_instantiate() {
    let (engine, module) = basic();
    let mut store = Store::new(&engine, ());
    let mut linker = Linker::new(&engine);

    linker.func_wrap(1, |_: Caller<'_, ()>| Ok(())).unwrap();
    let _instance = linker.instantiate(&mut store, &module).expect("instantiates");

    let error = linker
        .func_wrap(2, |_: Caller<'_, ()>| Ok(()))
        .expect_err("should refuse");
    assert!(error.to_string().contains("after instantiate"), "{error}");
}

/// Resolve a guest symbol's address by reloading the ELF's symbol table.
fn guest_symbol(name: &str) -> u64 {
    let program = rv::elf::load(HOSTED).expect("loads");
    program.symbols[name]
}

/// The sandbox. A guest may compute any 64-bit address it likes; none of them
/// may reach host memory.
mod sandbox {
    use super::*;

    fn hosted() -> (Engine, Module) {
        let engine = Engine::default();
        let module = Module::new(&engine, HOSTED).expect("compiles");
        (engine, module)
    }

    #[test]
    fn reading_unmapped_guest_memory_traps() {
        let (engine, module) = hosted();
        let mut store = Store::new(&engine, Host::default());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");
        let read = instance.get_typed_func::<(u64,), u64>("read_at").expect("read_at");

        // Halfway up the address space: past the image, below the stack, so
        // nothing is committed there.
        let hole = module.memory_size() / 2;
        let error = read.call(&mut store, (hole,)).expect_err("should trap");
        let trap = error.downcast_ref::<Trap>().expect("a Trap");
        assert!(
            matches!(trap, Trap::MemoryFault { address: Some(addr) } if *addr == hole),
            "{trap}"
        );
    }

    #[test]
    fn code_is_write_protected_unless_it_shares_a_page_with_data() {
        // Guest permissions apply at *host* page granularity. A RISC-V image
        // typically places its read-only, executable and writable segments
        // 4 KiB apart, so on a 16 KiB-page host (macOS on arm64) all three land
        // in one page and take the union of their permissions -- which lets the
        // guest write to its own code. On a 4 KiB-page host they are separate
        // and the write traps.
        //
        // Either way the guest cannot reach host memory; the sandbox boundary
        // is the 4 GiB window, not this. Asserting the actual page layout keeps
        // the test honest on both.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
        let program = rv::elf::load(HOSTED).expect("loads");
        let code = guest_symbol("read_at");
        let shared = program.segments.iter().any(|segment| {
            segment.perms.write
                && segment.addr / page <= code / page
                && code / page < (segment.addr + segment.size).div_ceil(page)
        });

        let (engine, module) = hosted();
        let mut store = Store::new(&engine, Host::default());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");
        let write = instance
            .get_typed_func::<(u64, u64), u64>("write_at")
            .expect("write_at");

        let outcome = write.call(&mut store, (code, 0));
        if shared {
            assert!(
                outcome.is_ok(),
                "code shares a {page:#x} page with writable data, so the write should succeed"
            );
        } else {
            let error = outcome.expect_err("code has its own page, so this must trap");
            assert!(
                error
                    .downcast_ref::<Trap>()
                    .is_some_and(|t| matches!(t, Trap::MemoryFault { .. })),
                "{error}"
            );
        }
    }

    #[test]
    fn addresses_beyond_the_address_space_wrap_instead_of_escaping() {
        let (engine, module) = hosted();
        let mut store = Store::new(&engine, Host::default());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");
        let read = instance.get_typed_func::<(u64,), u64>("read_at").expect("read_at");
        let write = instance
            .get_typed_func::<(u64, u64), u64>("write_at")
            .expect("write_at");

        let buffer = guest_symbol("BUFFER");
        write.call(&mut store, (buffer, 0xdead_beef)).expect("writes");

        // Every one of these is the same guest address once masked with
        // `memory_size - 1`. Without the mask they would run off the end of the
        // reservation and into host memory.
        let size = module.memory_size();
        for multiple in [1u64, 2, 1024, 0x1_0000] {
            let wild = buffer + multiple * size;
            assert_eq!(
                read.call(&mut store, (wild,)).expect("wraps into the window"),
                0xdead_beef,
                "address {wild:#x} should alias {buffer:#x} in a {size:#x} space"
            );
        }
    }

    #[test]
    fn a_trap_leaves_the_store_usable() {
        let (engine, module) = hosted();
        let mut store = Store::new(&engine, Host::default());
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| Ok(a.wrapping_add(b)))
            .unwrap();
        let instance = linker.instantiate(&mut store, &module).expect("instantiates");

        let read = instance.get_typed_func::<(u64,), u64>("read_at").expect("read_at");
        let add = instance
            .get_typed_func::<(u64, u64), u64>("call_add")
            .expect("call_add");

        for _ in 0..3 {
            assert!(read.call(&mut store, (0x4000_0000,)).is_err());
            assert_eq!(add.call(&mut store, (2, 3)).unwrap(), 5);
        }
    }
}

/// The guest address space size is configurable, and the mask follows it.
mod memory_size {
    use super::*;

    fn engine_with(size: u64) -> Engine {
        let mut config = Config::new();
        config.memory_size(size).stack_size(64 << 10);
        Engine::new(&config).expect("engine")
    }

    #[test]
    fn a_small_address_space_still_runs() {
        // 1 MiB is enough for the fixture, whose image sits just above 64 KiB.
        let engine = engine_with(1 << 20);
        let module = Module::new(&engine, BASIC).expect("compiles");
        assert_eq!(module.memory_size(), 1 << 20);

        let mut store = Store::new(&engine, ());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");

        let fib = instance.get_typed_func::<(u64,), u64>("fib").expect("fib");
        assert_eq!(fib.call(&mut store, (20,)).unwrap(), 6765);
    }

    #[test]
    fn the_mask_follows_the_configured_size() {
        // The same wrap-around test, at a size that is not the default: an
        // address one whole space above a valid one must alias it, which only
        // holds if the baked-in mask matches the mapping.
        let engine = engine_with(1 << 20);
        let module = Module::new(&engine, HOSTED).expect("compiles");
        let mut store = Store::new(&engine, Host::default());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");

        let read = instance.get_typed_func::<(u64,), u64>("read_at").expect("read_at");
        let write = instance
            .get_typed_func::<(u64, u64), u64>("write_at")
            .expect("write_at");

        let buffer = guest_symbol("BUFFER");
        write.call(&mut store, (buffer, 0x1234)).expect("writes");
        assert_eq!(
            read.call(&mut store, (buffer + (1 << 20),)).unwrap(),
            0x1234
        );
    }

    #[test]
    fn rejects_a_size_that_is_not_a_power_of_two() {
        let engine = engine_with(3 << 20);
        let error = Module::new(&engine, BASIC).expect_err("should refuse");
        assert!(error.to_string().contains("power of two"), "{error}");
    }

    #[test]
    fn rejects_a_size_outside_the_permitted_range() {
        for size in [1u64 << 10, 1 << 40] {
            let engine = engine_with(size);
            assert!(Module::new(&engine, BASIC).is_err(), "size {size:#x}");
        }
    }

    #[test]
    fn an_image_too_large_for_the_space_is_a_clear_error() {
        // 64 KiB is the smallest permitted space, and the fixture's image
        // starts at 0x10000, so it cannot possibly fit.
        let mut config = Config::new();
        config.memory_size(64 << 10).stack_size(16 << 10);
        let engine = Engine::new(&config).expect("engine");
        let module = Module::new(&engine, BASIC).expect("compiles");

        let mut store = Store::new(&engine, ());
        let error = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect_err("should not fit");

        // `{:#}` renders the whole context chain; the top level is only
        // "failed to map guest memory".
        let message = format!("{error:#}");
        assert!(message.contains("no room for"), "{message}");
        assert!(message.contains("memory_size"), "{message}");
    }
}
