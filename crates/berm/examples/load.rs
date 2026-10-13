//! What a program invocation costs under concurrent load.
//!
//! Every call is split two ways — inside host calls, and the rest
//! (instantiate, guest code, teardown, and waiting for a thread). `carry`
//! makes no host call worth timing; `wait` makes one that sleeps [`WAIT`] on the
//! runtime's timer, standing in for an HTTP round trip.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target wasm32-unknown-unknown
//! cargo run --release --example load -p berm
//! ```

use anyhow::{Context, Result};
use berm::{Berm, Callsite, Config, Engine, Program, Syscall, abi};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const GUEST: (&str, &str) = (
    "target/wasm32-unknown-unknown/release/fixture.wasm",
    "wasm32-unknown-unknown",
);

/// How long `wait`'s one host call takes.
const WAIT: Duration = Duration::from_millis(20);

/// Nanoseconds spent inside host calls since the last row.
static IN_HOST: AtomicU64 = AtomicU64::new(0);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
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

    // `berm.get`, answering every key as never written after [`WAIT`].
    let get = Syscall {
        name: abi::GET.to_owned(),
        call: Arc::new(|_: Callsite, _: Vec<u8>| {
            Box::pin(async {
                let start = Instant::now();
                tokio::time::sleep(WAIT).await;
                IN_HOST.fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
                Ok(Vec::new())
            })
        }),
    };
    let engine = Engine::new(&Config {
        instances: 10_000,
        ..Config::default()
    })?;
    let program = Berm::new(
        &engine,
        berm::Calls::by_name(0),
        vec![get],
        berm::storage::Memory::new(),
    )
    .deploy("fixture", &image)?;

    println!("cores: {}", std::thread::available_parallelism()?);
    header("carry");
    for concurrency in [1, 2, 4, 8, 16, 64] {
        row(&program, "carry", concurrency, 4000).await?;
    }
    header(&format!("wait ({WAIT:?} in one host call)"));
    for concurrency in [1, 64, 512, 1024, 2048, 8192] {
        row(&program, "wait", concurrency, concurrency * 4).await?;
    }
    Ok(())
}

fn header(tool: &str) {
    println!("\n{tool}");
    println!(
        "{:>6} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "conc", "calls/s", "p50", "p99", "host", "rest"
    );
}

/// Run `calls` invocations of `tool`, `concurrency` of them in flight at once,
/// and print what they cost.
async fn row(
    program: &Arc<Program>,
    tool: &'static str,
    concurrency: usize,
    calls: usize,
) -> Result<()> {
    IN_HOST.store(0, Ordering::Relaxed);
    let start = Instant::now();
    let mut workers = Vec::with_capacity(concurrency);
    for worker in 0..concurrency {
        let program = program.clone();
        let mine = calls / concurrency + usize::from(worker < calls % concurrency);
        workers.push(tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(mine);
            for _ in 0..mine {
                let submitted = Instant::now();
                program
                    .call(None, tool, b"{}".to_vec())
                    .await?
                    .result
                    .map_err(anyhow::Error::msg)?;
                latencies.push(submitted.elapsed());
            }
            Ok::<_, anyhow::Error>(latencies)
        }));
    }
    let mut latencies = Vec::with_capacity(calls);
    for worker in workers {
        latencies.extend(worker.await??);
    }
    let wall = start.elapsed();

    latencies.sort();
    let n = latencies.len() as u32;
    let mean = latencies.iter().sum::<Duration>() / n;
    let host = Duration::from_nanos(IN_HOST.load(Ordering::Relaxed)) / n;
    println!(
        "{:>6} {:>10.0} {:>10.3?} {:>10.3?} {:>10.3?} {:>10.3?}",
        concurrency,
        latencies.len() as f64 / wall.as_secs_f64(),
        latencies[latencies.len() / 2],
        latencies[latencies.len() * 99 / 100],
        host,
        mean.saturating_sub(host),
    );
    Ok(())
}
