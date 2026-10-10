//! What a chain leaves for its host to commit.
//!
//! The reference guest's `note` writes and emits; `relay` calls `note` on
//! `inner` and carries on whether or not it failed. A chain that returns a
//! result hands back every write and log it kept, one that fails hands back
//! nothing, and a failed nested call takes its own with it.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target wasm32-unknown-unknown
//! cargo run --release --example commit -p berm
//! ```

use anyhow::{Context, Result, ensure};
use berm::{
    Berm, Changes, Config, Engine, storage,
    syscall::{call, store},
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

const GUEST: &str = "target/wasm32-unknown-unknown/release/fixture.wasm";

type Committed = Arc<Mutex<HashMap<(String, String), Vec<u8>>>>;

#[tokio::main]
async fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("no workspace root")?
        .to_path_buf();
    let image = fs::read(root.join(GUEST)).with_context(|| {
        format!("build the guest first: cargo build --release -p berm-fixture --target wasm32-unknown-unknown ({GUEST})")
    })?;

    let engine = Engine::new(&Config {
        cache_dir: Some(std::env::temp_dir().join("berm-commit")),
        ..Config::default()
    })?;
    let committed = Committed::default();
    let reading = committed.clone();
    let berm = Berm::new(
        &engine,
        call::DEFAULT_CALL_DEPTH,
        store::programs(move |program, key| {
            Ok(reading
                .lock()
                .expect("committed")
                .get(&(program.to_owned(), key.to_owned()))
                .cloned())
        }),
        storage::Memory::new(),
    );
    berm.deploy("outer", &image)?;
    berm.deploy("inner", &image)?;

    // Kept: the write is read back inside the chain, and both reach the host.
    let outcome = berm.call(None, "outer", "note", "one").await?;
    ensure!(
        outcome.result.as_deref() == Ok("one"),
        "{:?}",
        outcome.result
    );
    ensure!(
        said(&outcome.changes) == ["write outer note one", "log outer [note] one"],
        "{:?}",
        outcome.changes
    );
    commit(&committed, &outcome.changes);
    println!("kept:     {:?}", said(&outcome.changes));

    // Failed: nothing reaches the host.
    let outcome = berm.call(None, "outer", "note", "fail").await?;
    ensure!(outcome.result.is_err(), "{:?}", outcome.result);
    ensure!(said(&outcome.changes).is_empty(), "{:?}", outcome.changes);
    println!("failed:   {:?}", said(&outcome.changes));

    // A failed nested call takes its own; the caller's log stays, and its key
    // still reads what was committed.
    let outcome = berm.call(None, "outer", "relay", "fail").await?;
    ensure!(
        outcome.result.as_deref() == Ok("one"),
        "{:?}",
        outcome.result
    );
    ensure!(
        said(&outcome.changes) == ["log outer [relay] fail"],
        "{:?}",
        outcome.changes
    );
    println!("reverted: {:?}", said(&outcome.changes));

    // A nested call that returns keeps its own, in the order it emitted them.
    let outcome = berm.call(None, "outer", "relay", "two").await?;
    ensure!(
        said(&outcome.changes)
            == [
                "write inner note two",
                "log inner [note] two",
                "log outer [relay] two",
            ],
        "{:?}",
        outcome.changes
    );
    println!("nested:   {:?}", said(&outcome.changes));

    println!("\nok");
    Ok(())
}

/// Each write, then each log, as one line.
fn said(changes: &Changes) -> Vec<String> {
    let writes = changes.writes.iter().map(|write| {
        format!(
            "write {} {} {}",
            write.program,
            write.key,
            String::from_utf8_lossy(&write.value)
        )
    });
    let logs = changes.logs.iter().map(|log| {
        format!(
            "log {} [{}] {}",
            log.program,
            log.topics.join(","),
            log.data
        )
    });
    writes.chain(logs).collect()
}

fn commit(committed: &Committed, changes: &Changes) {
    let mut committed = committed.lock().expect("committed");
    for write in &changes.writes {
        committed.insert(
            (write.program.to_string(), write.key.clone()),
            write.value.clone(),
        );
    }
}
