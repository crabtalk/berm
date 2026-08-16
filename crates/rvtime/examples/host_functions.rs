//! Let a guest call back into the host.
//!
//! ```sh
//! cargo run --example host_functions
//! ```

// ANCHOR: example
use rvtime::{Caller, Config, Engine, Linker, Module, Store};

const GUEST: &[u8] = include_bytes!("../../../fixtures/hosted.elf");

/// Whatever the host wants to keep across calls. Host functions reach it
/// through `Caller::data`.
#[derive(Default)]
struct State {
    ticks: u64,
}

fn main() -> anyhow::Result<()> {
    let engine = Engine::new(&Config::default())?;
    let module = Module::new(&engine, GUEST)?;
    let mut store = Store::new(&engine, State::default());

    let mut linker = Linker::new(&engine);

    // A guest calls these with `ecall`, taking the number from `a7`. The key
    // is a number rather than a name because an ELF has no import table to
    // resolve names against -- you and the guest agree on the numbering.
    linker.func_wrap(1, |_: Caller<'_, State>, a: u64, b: u64| {
        Ok(a.wrapping_add(b))
    })?;

    linker.func_wrap(4, |mut caller: Caller<'_, State>| {
        caller.data_mut().ticks += 1;
        Ok(caller.data().ticks)
    })?;

    // Host functions can read and write guest memory. The guest passes a
    // buffer the usual way, as a pointer and a length.
    linker.func_wrap(2, |caller: Caller<'_, State>, ptr: u64, len: u64| {
        let bytes = caller.read(ptr, len)?;
        Ok(bytes.iter().map(|b| *b as u64).sum::<u64>())
    })?;

    let instance = linker.instantiate(&mut store, &module)?;

    let add = instance.get_typed_func::<(u64, u64), u64>("call_add")?;
    println!(
        "guest asked the host to add: {}",
        add.call(&mut store, (20, 22))?
    );

    let tick = instance.get_typed_func::<(), u64>("call_tick")?;
    tick.call(&mut store, ())?;
    tick.call(&mut store, ())?;
    println!("host state after two ticks: {}", store.data().ticks);

    // The guest fills a buffer and asks the host to sum it.
    let round_trip = instance.get_typed_func::<(u64,), u64>("round_trip")?;
    println!(
        "sum of 1..=10 computed by the host: {}",
        round_trip.call(&mut store, (10,))?
    );

    // Returning `Err` from a host function stops the guest rather than
    // handing it a value. Use `Ok(code)` for failures the guest should handle.
    Ok(())
}
// ANCHOR_END: example
