//! What a program invocation costs under concurrent load, the way bermd runs
//! one: each call on `tokio::task::spawn_blocking`, whose pool holds at most
//! 512 threads.
//!
//! Every call is split three ways — queued for a blocking thread, inside host
//! calls, and the rest (instantiate, guest code, teardown). `echo` makes no
//! host call worth timing; `wait` makes one that sleeps [`WAIT`], standing in
//! for an HTTP round trip.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target wasm32-unknown-unknown
//! cargo run --release --example load -p berm
//! ```

use anyhow::{Context, Result};
use berm::{Berm, Config, Engine, Program, syscall::store};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(feature = "wasm")]
const GUEST: (&str, &str) = (
    "target/wasm32-unknown-unknown/release/fixture.wasm",
    "wasm32-unknown-unknown",
);
#[cfg(not(feature = "wasm"))]
const GUEST: (&str, &str) = (
    "target/riscv64imac-unknown-none-elf/release/fixture",
    "riscv64imac-unknown-none-elf",
);

/// How long `wait`'s one host call takes.
const WAIT: Duration = Duration::from_millis(20);

thread_local! {
    /// Time this thread has spent inside host calls since the last take.
    static IN_HOST: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// One call, as the three parts it is split into.
struct Sample {
    latency: Duration,
    queued: Duration,
    host: Duration,
}

fn main() -> Result<()> {
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

    let engine = Engine::new(&Config { cache_dir: None })?;
    let syscalls = store::programs(
        |_, _| {
            let start = Instant::now();
            std::thread::sleep(WAIT);
            IN_HOST.set(IN_HOST.get() + start.elapsed());
            Ok(None)
        },
        |_, _, _| Ok(()),
    );
    let program =
        Berm::new(&engine, 0, syscalls, berm::storage::Memory::new()).deploy("fixture", &image)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    println!("cores: {}", std::thread::available_parallelism()?);
    header("echo");
    for concurrency in [1, 2, 4, 8, 16, 64] {
        row(&runtime, &program, "echo", concurrency, 4000)?;
    }
    header(&format!("wait ({WAIT:?} in one host call)"));
    for concurrency in [1, 64, 512, 1024, 2048] {
        row(&runtime, &program, "wait", concurrency, concurrency * 4)?;
    }
    Ok(())
}

fn header(tool: &str) {
    println!("\n{tool}");
    println!(
        "{:>6} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "conc", "calls/s", "p50", "p99", "queued", "host", "rest"
    );
}

/// Run `calls` invocations of `tool`, at most `concurrency` of them submitted
/// at once, and print what they cost.
fn row(
    runtime: &tokio::runtime::Runtime,
    program: &Arc<Program>,
    tool: &'static str,
    concurrency: usize,
    calls: usize,
) -> Result<()> {
    let start = Instant::now();
    let mut samples = runtime.block_on(async {
        let mut workers = Vec::with_capacity(concurrency);
        for worker in 0..concurrency {
            let program = program.clone();
            let mine = calls / concurrency + usize::from(worker < calls % concurrency);
            workers.push(tokio::spawn(async move {
                let mut samples = Vec::with_capacity(mine);
                for _ in 0..mine {
                    let submitted = Instant::now();
                    let program = program.clone();
                    let (queued, host) = tokio::task::spawn_blocking(move || {
                        let queued = submitted.elapsed();
                        IN_HOST.take();
                        program
                            .call(tool, b"{}".to_vec())?
                            .map_err(anyhow::Error::msg)?;
                        Ok::<_, anyhow::Error>((queued, IN_HOST.take()))
                    })
                    .await??;
                    samples.push(Sample {
                        latency: submitted.elapsed(),
                        queued,
                        host,
                    });
                }
                Ok::<_, anyhow::Error>(samples)
            }));
        }
        let mut samples = Vec::with_capacity(calls);
        for worker in workers {
            samples.extend(worker.await??);
        }
        Ok::<_, anyhow::Error>(samples)
    })?;
    let wall = start.elapsed();

    samples.sort_by_key(|sample| sample.latency);
    let n = samples.len() as u32;
    let mean = |part: fn(&Sample) -> Duration| samples.iter().map(part).sum::<Duration>() / n;
    let rest = mean(|s| s.latency.saturating_sub(s.queued + s.host));
    println!(
        "{:>6} {:>10.0} {:>10.3?} {:>10.3?} {:>10.3?} {:>10.3?} {:>10.3?}",
        concurrency,
        samples.len() as f64 / wall.as_secs_f64(),
        samples[samples.len() / 2].latency,
        samples[samples.len() * 99 / 100].latency,
        mean(|s| s.queued),
        mean(|s| s.host),
        rest,
    );
    Ok(())
}
