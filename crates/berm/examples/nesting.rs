//! A program calling a program.
//!
//! The reference guest exports `nest`, which calls `echo` on whatever the
//! runtime answers for as `inner`. Here that is the same image, deployed twice
//! under two names — which is all a nested call is: berm resolving a name
//! against the set it already holds.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target wasm32-unknown-unknown
//! cargo run --release --example nesting -p berm
//! ```

use anyhow::{Context, Result};
use berm::{Berm, Calls, Config, Engine, storage, syscall::call};
use std::{fs, path::PathBuf, sync::Arc};

const GUEST: (&str, &str) = (
    "target/wasm32-unknown-unknown/release/fixture.wasm",
    "wasm32-unknown-unknown",
);

#[tokio::main]
async fn main() -> Result<()> {
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

    let engine = Engine::new(&Config {
        cache_dir: Some(std::env::temp_dir().join("berm-nesting")),
        ..Config::default()
    })?;

    let berm = Berm::new(
        &engine,
        Calls::by_name(call::DEFAULT_CALL_DEPTH),
        vec![],
        storage::Memory::new(),
    );
    berm.deploy("inner", &image)?;
    berm.deploy("outer", &image)?;

    let result = berm
        .call(None, "outer", "nest", br#"{"query":"hi"}"#.to_vec())
        .await?
        .result
        .map_err(anyhow::Error::msg)?;
    println!("reached:  {result}");
    assert!(
        result.contains(r#"{"query":"hi"}"#),
        "the nested call lost the payload: {result}"
    );

    // The other half of the wire: a refusal is not the target's own failure,
    // and the calling guest is told which it got. Nothing answers to `inner`
    // once it is gone.
    assert!(berm.remove("inner")?);
    let failure = berm
        .call(None, "outer", "nest", br#"{"query":"hi"}"#.to_vec())
        .await?
        .result
        .expect_err("a refused call fails its caller");
    println!("refused:  {failure}");
    assert!(
        failure.starts_with("refused: "),
        "the guest could not tell a refusal from a failure: {failure}"
    );

    // And the depth bound: `recurse` calls itself on `inner`, which is this
    // same image, until berm refuses to go deeper. What comes back is how many
    // levels got through — the runaway a limit exists to stop.
    berm.deploy("inner", &image)?;
    let reached = berm
        .call(None, "outer", "recurse", br#"{"depth":0}"#.to_vec())
        .await?
        .result
        .map_err(anyhow::Error::msg)?;
    println!("depth:    {reached} levels before the runtime refused");

    // A resolver decides what a name reaches. Here, as in a host that keeps
    // programs per owner, `inner` reaches the `inner` beside the caller.
    let owned = Berm::new(
        &engine,
        Calls {
            depth: call::DEFAULT_CALL_DEPTH,
            resolve: Arc::new(|at, target, _| {
                let (owner, _) = at
                    .program
                    .split_once('/')
                    .context("deployed without an owner")?;
                Ok(format!("{owner}/{target}"))
            }),
            // A callee is told the caller's name within the owner.
            named: Arc::new(|program| {
                program
                    .split_once('/')
                    .map_or(program, |(_, name)| name)
                    .to_owned()
            }),
            ..Calls::by_name(call::DEFAULT_CALL_DEPTH)
        },
        vec![],
        storage::Memory::new(),
    );
    owned.deploy("alice/outer", &image)?;
    owned.deploy("alice/inner", &image)?;
    owned.deploy("bob/outer", &image)?;
    let reached = owned
        .call(None, "alice/outer", "nest", br#"{"query":"hi"}"#.to_vec())
        .await?
        .result
        .map_err(anyhow::Error::msg)?;
    println!("resolved: {reached}");
    let failure = owned
        .call(None, "bob/outer", "nest", br#"{"query":"hi"}"#.to_vec())
        .await?
        .result
        .expect_err("bob has no inner");
    println!("unowned:  {failure}");
    assert!(failure.contains("bob/inner"), "{failure}");

    println!("\nok");
    Ok(())
}
