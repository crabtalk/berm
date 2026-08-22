//! Stop a guest that will not stop itself.
//!
//! ```sh
//! cargo run --example interrupting
//! ```

// ANCHOR: example
use rvtime::{Config, Engine, Linker, Module, Store};
use std::{thread, time::Duration};

const GUEST: &[u8] = include_bytes!("../../../fixtures/hosted.elf");

fn main() -> anyhow::Result<()> {
    // On by default, but spelled out here because it is the point of the
    // example: it makes the translator emit a check on every backward edge.
    let mut config = Config::default();
    config.interruptible(true);

    let engine = Engine::new(&config)?;
    let module = Module::new(&engine, GUEST)?;
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine).instantiate(&mut store, &module)?;

    // The handle is `Send + Sync`, so a watchdog can hold one while the guest
    // runs somewhere else.
    let handle = store.interrupt_handle()?;

    // `spin` never returns on its own.
    let spin = instance.get_typed_func::<(u64,), u64>("spin")?;
    let guest = thread::spawn(move || spin.call(&mut store, (0,)));

    thread::sleep(Duration::from_millis(100));
    println!("asking the guest to stop");
    handle.interrupt();

    // The guest stops at its next loop iteration and the pending call fails.
    match guest.join().expect("the guest thread panicked") {
        Ok(value) => println!("returned {value} -- unexpected, `spin` should not return"),
        Err(error) => println!("stopped: {error}"),
    }

    Ok(())
}
// ANCHOR_END: example
