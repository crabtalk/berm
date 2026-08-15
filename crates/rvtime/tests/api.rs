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
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");

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
        .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| {
            Ok(a.wrapping_add(b))
        })
        .unwrap();
    linker
        .func_wrap(4, |mut caller: Caller<'_, Host>| {
            caller.data_mut().ticks += 1;
            Ok(caller.data().ticks)
        })
        .unwrap();

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");

    let add = instance
        .get_typed_func::<(u64, u64), u64>("call_add")
        .expect("call_add");
    assert_eq!(add.call(&mut store, (20, 22)).unwrap(), 42);

    let tick = instance
        .get_typed_func::<(), u64>("call_tick")
        .expect("call_tick");
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

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");
    let round_trip = instance
        .get_typed_func::<(u64,), u64>("round_trip")
        .expect("round_trip");

    // The guest fills a buffer with 1..=n, then asks the host to sum it.
    for n in [0u64, 1, 5, 32] {
        let expected = (1..=n).sum::<u64>();
        assert_eq!(
            round_trip.call(&mut store, (n,)).unwrap(),
            expected,
            "n={n}"
        );
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

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");
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
        .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| {
            Ok(a.wrapping_add(b))
        })
        .unwrap();

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");
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

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");
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

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");
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
        .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| {
            Ok(a.wrapping_add(b))
        })
        .unwrap();
    linker
        .func_wrap(4, |mut caller: Caller<'_, Host>| {
            caller.data_mut().ticks += 1;
            Ok(caller.data().ticks)
        })
        .unwrap();

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");
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
    let a = linker
        .instantiate(&mut first, &module)
        .expect("instantiates");
    let b = linker
        .instantiate(&mut second, &module)
        .expect("instantiates");

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
    let _instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiates");

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
        let read = instance
            .get_typed_func::<(u64,), u64>("read_at")
            .expect("read_at");

        // The guard page between the heap and the stack is the one gap left
        // uncommitted inside the address space.
        let hole = store.heap().expect("instantiated").end;
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
        let read = instance
            .get_typed_func::<(u64,), u64>("read_at")
            .expect("read_at");
        let write = instance
            .get_typed_func::<(u64, u64), u64>("write_at")
            .expect("write_at");

        let buffer = guest_symbol("BUFFER");
        write
            .call(&mut store, (buffer, 0xdead_beef))
            .expect("writes");

        // Every one of these is the same guest address once masked with
        // `memory_size - 1`. Without the mask they would run off the end of the
        // reservation and into host memory.
        let size = module.memory_size();
        for multiple in [1u64, 2, 1024, 0x1_0000] {
            let wild = buffer + multiple * size;
            assert_eq!(
                read.call(&mut store, (wild,))
                    .expect("wraps into the window"),
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
            .func_wrap(1, |_: Caller<'_, Host>, a: u64, b: u64| {
                Ok(a.wrapping_add(b))
            })
            .unwrap();
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("instantiates");

        let read = instance
            .get_typed_func::<(u64,), u64>("read_at")
            .expect("read_at");
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

        let read = instance
            .get_typed_func::<(u64,), u64>("read_at")
            .expect("read_at");
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

/// Properties an embedder relies on when hosting many guests.
mod embedding {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn modules_are_shareable_and_stores_are_sendable() {
        // A module is immutable once compiled, so one copy backs every
        // instance of a plugin.
        assert_send::<Module>();
        assert_sync::<Module>();

        // A store must be movable to a worker thread. It is deliberately not
        // `Sync`: entering a guest takes `&mut Store`.
        assert_send::<Store<u64>>();
    }

    #[test]
    fn a_guest_runs_on_another_thread() {
        let engine = Engine::default();
        let module = Module::new(&engine, BASIC).expect("compiles");
        let mut store = Store::new(&engine, ());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");
        let fib = instance.get_typed_func::<(u64,), u64>("fib").expect("fib");

        let result = std::thread::spawn(move || fib.call(&mut store, (20,)).unwrap())
            .join()
            .expect("thread completed");
        assert_eq!(result, 6765);
    }

    #[test]
    fn one_module_backs_many_concurrent_guests() {
        // The shape an embedder needs: compile once, instantiate per plugin,
        // run them on separate threads.
        let engine = Engine::default();
        let module = Module::new(&engine, BASIC).expect("compiles");

        let handles: Vec<_> = (0..8)
            .map(|n| {
                let module = module.clone();
                let engine = engine.clone();
                std::thread::spawn(move || {
                    let mut store = Store::new(&engine, ());
                    let instance = Linker::new(&engine)
                        .instantiate(&mut store, &module)
                        .expect("instantiates");
                    let fib = instance.get_typed_func::<(u64,), u64>("fib").expect("fib");
                    fib.call(&mut store, (n,)).unwrap()
                })
            })
            .collect();

        let results: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results, vec![0, 1, 1, 2, 3, 5, 8, 13]);
    }

    #[test]
    fn many_small_guests_coexist() {
        // Dynamically installed plugins mean many address spaces at once, so
        // the per-guest size has to be tunable down.
        let mut config = Config::new();
        config.memory_size(1 << 20).stack_size(64 << 10);
        let engine = Engine::new(&config).expect("engine");
        let module = Module::new(&engine, BASIC).expect("compiles");

        let mut guests: Vec<_> = (0..32)
            .map(|_| {
                let mut store = Store::new(&engine, ());
                let instance = Linker::new(&engine)
                    .instantiate(&mut store, &module)
                    .expect("instantiates");
                (store, instance)
            })
            .collect();

        // Each has its own memory: `bump` accumulates into a guest global.
        for (store, instance) in &mut guests {
            let bump = instance
                .get_typed_func::<(u64,), u64>("bump")
                .expect("bump");
            assert_eq!(bump.call(store, (5,)).unwrap(), 0);
            assert_eq!(bump.call(store, (5,)).unwrap(), 5);
        }
    }

    #[test]
    fn a_result_type_wider_than_the_abi_is_refused() {
        // Only a0 and a1 come back from a guest function. Accepting more would
        // report registers the callee never wrote as returned values.
        let (engine, module) = basic();
        let mut store = Store::new(&engine, ());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");

        assert!(
            instance
                .get_typed_func::<(u64, u64), (u64, u64)>("op_add")
                .is_ok()
        );

        let error = instance
            .get_typed_func::<(u64, u64), (u64, u64, u64)>("op_add")
            .expect_err("three results do not fit the ABI");
        assert!(error.to_string().contains("returns at most 2"), "{error}");
    }
}

/// The heap rvtime commits for a guest allocator to manage.
mod heap {
    use super::*;

    fn instantiate(size: u64, stack: u64) -> (Store<Host>, rvtime::Instance) {
        let mut config = Config::new();
        config.memory_size(size).stack_size(stack);
        let engine = Engine::new(&config).expect("engine");
        let module = Module::new(&engine, HOSTED).expect("compiles");
        let mut store = Store::new(&engine, Host::default());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");
        (store, instance)
    }

    #[test]
    fn sits_between_the_image_and_the_stack() {
        let (store, _) = instantiate(16 << 20, 64 << 10);
        let heap = store.heap().expect("instantiated");

        // Above every loaded segment.
        let image_end = rv::elf::load(HOSTED).unwrap().image_end();
        assert!(
            heap.start >= image_end,
            "heap {heap:#x?} overlaps the image"
        );

        // And below the stack, which the write test confirms is still reachable.
        assert!(heap.end < 16 << 20);
        assert!(!heap.is_empty(), "a 16 MiB space should leave a heap");
    }

    #[test]
    fn is_readable_and_writable_by_the_guest() {
        let (mut store, instance) = instantiate(16 << 20, 64 << 10);
        let heap = store.heap().expect("instantiated");

        let read = instance
            .get_typed_func::<(u64,), u64>("read_at")
            .expect("read_at");
        let write = instance
            .get_typed_func::<(u64, u64), u64>("write_at")
            .expect("write_at");

        for addr in [heap.start, heap.start + 4096, heap.end - 8] {
            write.call(&mut store, (addr, 0xfeed)).expect("writable");
            assert_eq!(read.call(&mut store, (addr,)).unwrap(), 0xfeed, "{addr:#x}");
        }
    }

    #[test]
    fn starts_zeroed() {
        let (mut store, instance) = instantiate(16 << 20, 64 << 10);
        let heap = store.heap().expect("instantiated");
        let read = instance
            .get_typed_func::<(u64,), u64>("read_at")
            .expect("read_at");

        // An allocator hands out memory assuming it is zeroed.
        for offset in [0u64, 1 << 12, 1 << 20] {
            assert_eq!(read.call(&mut store, (heap.start + offset,)).unwrap(), 0);
        }
    }

    #[test]
    fn running_off_the_top_faults() {
        let (mut store, instance) = instantiate(16 << 20, 64 << 10);
        let heap = store.heap().expect("instantiated");
        let write = instance
            .get_typed_func::<(u64, u64), u64>("write_at")
            .expect("write_at");

        // The guard page is what stops a runaway heap from reaching the stack.
        let error = write
            .call(&mut store, (heap.end, 1))
            .expect_err("past the heap must fault");
        assert!(
            error
                .downcast_ref::<Trap>()
                .is_some_and(|t| matches!(t, Trap::MemoryFault { .. })),
            "{error}"
        );
    }

    #[test]
    fn each_guest_gets_its_own() {
        let (mut first, first_inst) = instantiate(16 << 20, 64 << 10);
        let (mut second, second_inst) = instantiate(16 << 20, 64 << 10);
        let heap = first.heap().expect("instantiated");

        let write = first_inst
            .get_typed_func::<(u64, u64), u64>("write_at")
            .expect("write_at");
        let read = second_inst
            .get_typed_func::<(u64,), u64>("read_at")
            .expect("read_at");

        write
            .call(&mut first, (heap.start, 0xabcd))
            .expect("writes");
        assert_eq!(
            read.call(&mut second, (heap.start,)).unwrap(),
            0,
            "one guest's heap must not be visible to another"
        );
    }

    #[test]
    fn a_space_too_small_for_a_heap_is_refused() {
        let mut config = Config::new();
        config.memory_size(64 << 10).stack_size(16 << 10);
        let engine = Engine::new(&config).expect("engine");
        let module = Module::new(&engine, HOSTED).expect("compiles");

        let mut store = Store::new(&engine, Host::default());
        let error = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect_err("should not fit");
        assert!(
            format!("{error:#}").contains("no room for a heap"),
            "{error:#}"
        );
    }
}

/// The guest allocator from the `rvtime-guest` SDK, driven end to end.
mod guest_alloc {
    use super::*;

    fn plugin() -> (Store<Host>, rvtime::Instance) {
        let mut config = Config::new();
        config.memory_size(16 << 20).stack_size(64 << 10);
        let engine = Engine::new(&config).expect("engine");
        let module = Module::new(&engine, HOSTED).expect("compiles");
        let mut store = Store::new(&engine, Host::default());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");
        (store, instance)
    }

    /// The handshake an embedder performs: read the bounds rvtime committed and
    /// hand them to the guest, which gives them to its allocator.
    fn with_heap() -> (Store<Host>, rvtime::Instance) {
        let (mut store, instance) = plugin();
        let heap = store.heap().expect("instantiated");
        let init = instance
            .get_typed_func::<(u64, u64), u64>("init_heap")
            .expect("init_heap");
        init.call(&mut store, (heap.start, heap.end - heap.start))
            .expect("heap initialises");
        (store, instance)
    }

    #[test]
    fn allocating_works_once_the_heap_is_handed_over() {
        let (mut store, instance) = with_heap();
        let sum = instance
            .get_typed_func::<(u64,), u64>("alloc_sum")
            .expect("alloc_sum");

        // A growing Vec: allocation, reallocation, then free.
        for n in [0u64, 1, 10, 1000, 10_000] {
            assert_eq!(
                sum.call(&mut store, (n,)).unwrap(),
                n * n.saturating_sub(1) / 2
            );
        }
    }

    #[test]
    fn allocation_comes_out_of_the_committed_heap() {
        let (mut store, instance) = with_heap();
        let heap = store.heap().expect("instantiated");
        let free = instance.get_typed_func::<(), u64>("heap_free").expect("heap_free");
        let used = instance.get_typed_func::<(), u64>("heap_used").expect("heap_used");
        let sum = instance
            .get_typed_func::<(u64,), u64>("alloc_sum")
            .expect("alloc_sum");

        // The allocator was given the whole region rvtime committed.
        let available = free.call(&mut store, ()).unwrap();
        let size = heap.end - heap.start;
        assert!(
            available > size - (1 << 16) && available <= size,
            "allocator has {available:#x} of a {size:#x} heap"
        );

        // And it hands memory back, so a plugin can run indefinitely.
        assert_eq!(used.call(&mut store, ()).unwrap(), 0);
        sum.call(&mut store, (10_000,)).unwrap();
        assert_eq!(
            used.call(&mut store, ()).unwrap(),
            0,
            "the vector should have been freed"
        );
    }

    #[test]
    fn allocating_before_the_handover_traps() {
        // With no heap the allocator returns null and the guest writes through
        // it. Guest address 0 is never committed, so that faults instead of
        // corrupting whatever happens to sit at the bottom of the address
        // space -- a null dereference in a guest is a trap, not a silent write.
        let (mut store, instance) = plugin();
        let sum = instance
            .get_typed_func::<(u64,), u64>("alloc_sum")
            .expect("alloc_sum");

        let error = sum.call(&mut store, (100,)).expect_err("should trap");
        let trap = error.downcast_ref::<Trap>().expect("a Trap");
        assert!(
            matches!(trap, Trap::MemoryFault { address: Some(0) }),
            "{trap}"
        );
    }

    #[test]
    fn a_guest_panic_reaches_the_host_as_a_trap() {
        // Same mechanism, reached deliberately: exhausting the heap panics.
        let (mut store, instance) = with_heap();
        let sum = instance
            .get_typed_func::<(u64,), u64>("alloc_sum")
            .expect("alloc_sum");

        let error = sum
            .call(&mut store, (1 << 30,))
            .expect_err("an impossible allocation should trap");
        assert!(
            error.downcast_ref::<Trap>().is_some(),
            "expected a trap, got {error}"
        );
    }
}

/// Reusing generated code across runs.
mod cache {
    use super::*;
    use std::path::PathBuf;

    /// A scratch directory that cleans up after itself.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("rvtime-cache-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            Dir(path)
        }

        fn entries(&self) -> usize {
            std::fs::read_dir(&self.0).map(|d| d.count()).unwrap_or(0)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn engine(dir: &Dir, level: OptLevel) -> Engine {
        let mut config = Config::new();
        config.cache_dir(&dir.0).opt_level(level);
        Engine::new(&config).expect("engine")
    }

    #[test]
    fn a_second_run_is_served_from_disk() {
        let dir = Dir::new("reuse");

        // Cold: nothing on disk, so every function is generated and stored.
        let cold = engine(&dir, OptLevel::None);
        Module::new(&cold, HOSTED).expect("compiles");
        let (hits, misses) = cold.cache_stats();
        assert_eq!(hits, 0, "a cold cache cannot hit");
        assert!(misses > 0, "expected to compile something");
        assert!(dir.entries() > 0, "nothing was written");

        // Warm: a fresh engine over the same directory, as after a restart.
        let warm = engine(&dir, OptLevel::None);
        Module::new(&warm, HOSTED).expect("compiles");
        let (hits, misses) = warm.cache_stats();
        assert_eq!(misses, 0, "everything should have been cached");
        assert_eq!(hits, cold.cache_stats().1, "every function should hit");
    }

    #[test]
    fn cached_code_behaves_identically() {
        let dir = Dir::new("behaviour");

        // Populate, then run entirely from the cache.
        Module::new(&engine(&dir, OptLevel::None), BASIC).expect("compiles");

        let warm = engine(&dir, OptLevel::None);
        let module = Module::new(&warm, BASIC).expect("compiles");
        assert_eq!(warm.cache_stats().1, 0, "should be a full hit");

        let mut store = Store::new(&warm, ());
        let instance = Linker::new(&warm)
            .instantiate(&mut store, &module)
            .expect("instantiates");

        // The whole point: deserialised code must compute what generated code
        // computed.
        let fib = instance.get_typed_func::<(u64,), u64>("fib").expect("fib");
        assert_eq!(fib.call(&mut store, (20,)).unwrap(), 6765);

        let dispatch = instance
            .get_typed_func::<(u64, u64, u64), u64>("dispatch")
            .expect("dispatch");
        assert_eq!(dispatch.call(&mut store, (1, 10, 3)).unwrap(), 7);
    }

    #[test]
    fn changing_the_target_settings_does_not_reuse_stale_code() {
        let dir = Dir::new("settings");

        Module::new(&engine(&dir, OptLevel::None), BASIC).expect("compiles");

        // The key covers ISA settings, so a different optimisation level must
        // miss rather than hand back code built for the old one.
        let other = engine(&dir, OptLevel::Speed);
        Module::new(&other, BASIC).expect("compiles");
        assert_eq!(
            other.cache_stats().0,
            0,
            "code built at a different opt level must not be reused"
        );
    }

    #[test]
    fn an_engine_without_a_cache_reports_nothing() {
        let engine = Engine::default();
        Module::new(&engine, BASIC).expect("compiles");
        assert_eq!(engine.cache_stats(), (0, 0));
    }

    #[test]
    fn a_corrupt_entry_is_not_fatal() {
        let dir = Dir::new("corrupt");
        Module::new(&engine(&dir, OptLevel::None), BASIC).expect("compiles");

        // Truncate every entry. A cache is disk state that other things can
        // damage; the compiler must fall back rather than miscompile.
        for entry in std::fs::read_dir(&dir.0).expect("readable").flatten() {
            std::fs::write(entry.path(), b"not a compiled function").expect("writable");
        }

        let engine = engine(&dir, OptLevel::None);
        let module = Module::new(&engine, BASIC).expect("compiles despite a damaged cache");

        let mut store = Store::new(&engine, ());
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect("instantiates");
        let fib = instance.get_typed_func::<(u64,), u64>("fib").expect("fib");
        assert_eq!(fib.call(&mut store, (20,)).unwrap(), 6765);
    }
}
