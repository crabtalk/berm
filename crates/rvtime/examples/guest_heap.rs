//! Give a guest a heap, so it can allocate.
//!
//! ```sh
//! cargo run --example guest_heap
//! ```

// ANCHOR: example
use rvtime::{Config, Engine, Linker, Module, Store};

const GUEST: &[u8] = include_bytes!("../../../fixtures/hosted.elf");

fn main() -> anyhow::Result<()> {
    let mut config = Config::default();
    config.memory_size(16 << 20).stack_size(64 << 10);

    let engine = Engine::new(&config)?;
    let module = Module::new(&engine, GUEST)?;
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine).instantiate(&mut store, &module)?;

    // rvtime commits a heap between the guest's image and its stack, but does
    // not tell the guest where it is: the bounds travel through whatever
    // interface you defined. Here the guest exports a function to receive them.
    let heap = store.heap()?;
    println!(
        "heap: {:#x}..{:#x} ({} KiB)",
        heap.start,
        heap.end,
        (heap.end - heap.start) / 1024
    );

    let init = instance.get_typed_func::<(u64, u64), u64>("init_heap")?;
    init.call(&mut store, (heap.start, heap.end - heap.start))?;

    // With the allocator fed, the guest can use `alloc` -- `Vec`, `String`,
    // and any crate that does not need `std`.
    let sum = instance.get_typed_func::<(u64,), u64>("alloc_sum")?;
    println!(
        "guest summed a heap-allocated vector: {}",
        sum.call(&mut store, (1000,))?
    );

    // The allocator hands memory back, so a long-running guest does not leak.
    let used = instance.get_typed_func::<(), u64>("heap_used")?;
    println!("bytes still allocated: {}", used.call(&mut store, ())?);

    Ok(())
}
// ANCHOR_END: example
