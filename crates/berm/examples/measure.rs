//! What a program invocation costs.
//!
//! RFC 0205 puts an instance per invocation on the critical path for every tool
//! call, so this measures the parts that decide whether that holds: compiling
//! an image cold and warm, and one full invocation — instantiate, argument
//! transfer, guest call, result read, teardown.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target wasm32-unknown-unknown
//! cargo run --release --example measure -p berm
//! ```

use anyhow::{Context, Result};
use berm::{Berm, Calls, Config, Engine, Program, storage};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const GUEST: (&str, &str) = (
    "target/wasm32-unknown-unknown/release/fixture.wasm",
    "wasm32-unknown-unknown",
);
const ROUNDS: usize = 1000;

#[tokio::main]
async fn main() -> Result<()> {
    // Only the guest's own log; wasmtime is chatty at info.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn,program=info"))
        .init();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("no workspace root")?
        .to_path_buf();

    let (guest, target) = GUEST;
    let image = fs::read(root.join(guest)).with_context(|| {
        format!("build the guest first: cargo build --release -p berm-fixture --target {target} ({guest})")
    })?;
    println!("guest: {} bytes", image.len());

    let cache = std::env::temp_dir().join("berm-measure");
    let _ = fs::remove_dir_all(&cache);

    let cold = time(|| compile(&cache, &image))?;
    println!("compile (cold cache):{cold:>12.3?}");

    let warm = time(|| compile(&cache, &image))?;
    println!("compile (warm cache):{warm:>12.3?}");

    let program = compile(&cache, &image)?;
    println!("manifest:              {:?}", program.manifest());

    println!(
        "heap probe:            {:?}",
        program.call(None, "probe", b"".to_vec()).await?.result
    );

    // A payload in the range a real tool call carries.
    let args = format!(r#"{{"query":"{}"}}"#, "x".repeat(256));
    let echoed = program
        .call(None, "echo", args.as_bytes())
        .await?
        .result
        .map_err(anyhow::Error::msg)?;
    assert_eq!(echoed, args, "round trip lost the payload");
    println!(
        "failure path:          {:?}",
        program
            .call(None, "boom", b"".to_vec())
            .await?
            .result
            .unwrap_err()
    );

    let mut chatty = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let start = Instant::now();
        let _ = program.call(None, "chatty", b"".to_vec()).await?;
        chatty.push(start.elapsed());
    }
    chatty.sort();
    println!("  +100 host calls:     {:>10.3?}", chatty[ROUNDS / 2]);

    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let start = Instant::now();
        let _ = program.call(None, "echo", args.as_bytes()).await?;
        samples.push(start.elapsed());
    }
    samples.sort();

    // The same payload, read by `echo` and carried unread by `carry`.
    // Composition passes a tool's arguments from one guest to the next, so the
    // difference is what JSON costs per nesting level — and whether it tracks
    // the payload says which half is the parse and which is the allocation
    // behind it.
    let p50 = async |tool: &str, args: &[u8]| -> Result<Duration> {
        let mut samples = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let start = Instant::now();
            let _ = program.call(None, tool, args.to_vec()).await?;
            samples.push(start.elapsed());
        }
        samples.sort();
        Ok(samples[ROUNDS / 2])
    };

    println!("json parse, over the same payload carried:");
    for size in [16usize, 256, 4096] {
        let args = format!(r#"{{"query":"{}"}}"#, "x".repeat(size));
        let (raw, parsed) = (
            p50("carry", args.as_bytes()).await?,
            p50("echo", args.as_bytes()).await?,
        );
        println!(
            "  {size:>5} B:            {:>10.3?}  ({:.3?} over {:.3?})",
            parsed.saturating_sub(raw),
            parsed,
            raw
        );
    }

    // `carry` never allocates and `echo` does, so the flat delta above would be
    // the heap's first use rather than the parse. `probe` allocates without
    // parsing, which tells the two apart — and it costs the same for one byte
    // as for four thousand, so what is priced is the first use, not the bytes.
    println!(
        "  alloc, no parse:     {:>10.3?}",
        p50("probe", b"")
            .await?
            .saturating_sub(p50("carry", b"{}").await?)
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
        ..Config::default()
    })?;
    Berm::new(&engine, Calls::by_name(0), vec![], storage::Memory::new()).deploy("fixture", image)
}

fn time<T>(f: impl FnOnce() -> Result<T>) -> Result<Duration> {
    let start = Instant::now();
    f()?;
    Ok(start.elapsed())
}
