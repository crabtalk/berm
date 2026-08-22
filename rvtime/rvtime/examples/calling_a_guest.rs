//! Compile a guest and call one of its functions.
//!
//! ```sh
//! cargo run --example calling_a_guest
//! ```

// ANCHOR: example
use rvtime::{Config, Engine, Linker, Module, Store};

/// A statically linked RV64IMAC ELF, built with `--emit-relocs`.
const GUEST: &[u8] = include_bytes!("../../../fixtures/basic.elf");

fn main() -> anyhow::Result<()> {
    // An engine holds the target configuration. Compiled code is tied to it,
    // so a module and the store that runs it must come from the same one.
    let engine = Engine::new(&Config::default())?;

    // Compiling reads the ELF, decodes every function, and generates native
    // code for all of them up front.
    let module = Module::new(&engine, GUEST)?;

    // A store owns one guest's memory and registers, plus whatever data you
    // want host functions to see. Here there is none, so `()`.
    let mut store = Store::new(&engine, ());

    // Instantiating maps the guest's memory and wires it to the compiled code.
    let instance = Linker::new(&engine).instantiate(&mut store, &module)?;

    // Exports are looked up by symbol name. The type parameters say how many
    // argument registers to use and how many results to read back -- an ELF
    // carries no signature to check them against.
    let add = instance.get_typed_func::<(u64, u64), u64>("op_add")?;
    assert_eq!(add.call(&mut store, (10, 3))?, 13);

    // A handle is reusable, and calls are ordinary function calls.
    let fib = instance.get_typed_func::<(u64,), u64>("fib")?;
    for n in 0..10 {
        print!("{} ", fib.call(&mut store, (n,))?);
    }
    println!();

    Ok(())
}
// ANCHOR_END: example
