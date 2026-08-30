//! What a program invocation costs.
//!
//! RFC 0205 puts an instance per invocation on the critical path for every tool
//! call, so this measures the parts that decide whether that holds: compiling
//! an image cold and warm, and one full invocation — instantiate, argument
//! transfer, guest call, result read, teardown.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target riscv64imac-unknown-none-elf
//! cargo run --release --example measure -p berm
//! ```

use anyhow::{Context, Result};
use berm::{Berm, Config, Engine, Program, storage};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const GUEST: (&str, &str) = (
    "target/riscv64imac-unknown-none-elf/release/fixture",
    "riscv64imac-unknown-none-elf",
);
const ROUNDS: usize = 1000;

fn main() -> Result<()> {
    // Only the guest's own log; cranelift is chatty at info.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn,program=info"))
        .init();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("no workspace root")?
        .to_path_buf();

    let (guest, target) = GUEST;
    let elf = fs::read(root.join(guest)).with_context(|| {
        format!("build the guest first: cargo build --release -p berm-fixture --target {target} ({guest})")
    })?;
    println!("guest: {} bytes", elf.len());

    let cache = std::env::temp_dir().join("berm-measure");
    let _ = fs::remove_dir_all(&cache);

    let cold = time(|| compile(&cache, &elf))?;
    println!("compile (cold cache):{cold:>12.3?}");

    let warm = time(|| compile(&cache, &elf))?;
    println!("compile (warm cache):{warm:>12.3?}");

    let program = compile(&cache, &elf)?;
    println!("manifest:              {:?}", program.manifest());

    println!(
        "heap probe:            {:?}",
        program.call("probe", b"".to_vec())?
    );

    // A payload in the range a real tool call carries.
    let args = format!(r#"{{"query":"{}"}}"#, "x".repeat(256));
    let echoed = program
        .call("echo", args.as_bytes())?
        .map_err(anyhow::Error::msg)?;
    assert!(echoed.contains(&args), "round trip lost the payload");
    println!(
        "failure path:          {:?}",
        program.call("boom", b"".to_vec())?.unwrap_err()
    );

    let mut chatty = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let start = Instant::now();
        let _ = program.call("chatty", b"".to_vec())?;
        chatty.push(start.elapsed());
    }
    chatty.sort();
    println!("  +100 host calls:     {:>10.3?}", chatty[ROUNDS / 2]);

    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let start = Instant::now();
        let _ = program.call("echo", args.as_bytes())?;
        samples.push(start.elapsed());
    }
    samples.sort();

    // The same payload, deserialized instead of copied. Composition passes a
    // tool's arguments from one guest to the next, so the difference is what
    // JSON costs per nesting level — and whether it tracks the payload says
    // which half is the parse and which is the allocation behind it.
    let p50 = |tool: &str, args: &[u8]| -> Result<Duration> {
        let mut samples = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let start = Instant::now();
            let _ = program.call(tool, args.to_vec())?;
            samples.push(start.elapsed());
        }
        samples.sort();
        Ok(samples[ROUNDS / 2])
    };

    println!("json parse, over the same payload echoed:");
    for size in [16usize, 256, 4096] {
        let args = format!(r#"{{"query":"{}"}}"#, "x".repeat(size));
        let (raw, parsed) = (
            p50("echo", args.as_bytes())?,
            p50("typed", args.as_bytes())?,
        );
        println!(
            "  {size:>5} B:            {:>10.3?}  ({:.3?} over {:.3?})",
            parsed.saturating_sub(raw),
            parsed,
            raw
        );
    }

    // `echo` never allocates and `typed` does, so the flat delta above would be
    // the heap's first use rather than the parse. `probe` allocates without
    // parsing, which tells the two apart — and it costs the same for one byte
    // as for four thousand, so what is priced is the first use, not the bytes.
    println!(
        "  alloc, no parse:     {:>10.3?}",
        p50("probe", b"")?.saturating_sub(p50("echo", b"{}")?)
    );

    println!("invocations:           {ROUNDS}");
    println!("  min:                 {:>10.3?}", samples[0]);
    println!("  p50:                 {:>10.3?}", samples[ROUNDS / 2]);
    println!(
        "  p99:                 {:>10.3?}",
        samples[ROUNDS * 99 / 100]
    );
    println!("  max:                 {:>10.3?}", samples[ROUNDS - 1]);
    println!(
        "  mean:                {:>10.3?}",
        samples.iter().sum::<Duration>() / ROUNDS as u32
    );

    Ok(())
}

/// Compiling a guest, reusing whatever the previous run left behind.
fn compile(dir: &std::path::Path, image: &[u8]) -> Result<Arc<Program>> {
    let engine = Engine::new(&Config {
        cache_dir: Some(dir.to_path_buf()),
    })?;
    Berm::new(&engine, 0, vec![], storage::Memory::new()).deploy("fixture", image)
}

fn time<T>(f: impl FnOnce() -> Result<T>) -> Result<Duration> {
    let start = Instant::now();
    f()?;
    Ok(start.elapsed())
}
