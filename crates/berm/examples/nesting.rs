//! A program calling a program.
//!
//! The reference guest exports `nest`, which calls `echo` on whatever the
//! runtime answers for as `inner`. Here that is the same ELF, deployed twice
//! under two names — which is all a nested call is: berm resolving a name
//! against the set it already holds.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target riscv64imac-unknown-none-elf
//! cargo run --release --example nesting -p berm
//! ```

use anyhow::{Context, Result};
use berm::{Berm, Config, Engine, storage, syscall::call};
use std::{fs, path::PathBuf};

const GUEST: (&str, &str) = (
    "target/riscv64imac-unknown-none-elf/release/fixture",
    "riscv64imac-unknown-none-elf",
);

fn main() -> Result<()> {
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

    let engine = Engine::new(&Config {
        cache_dir: Some(std::env::temp_dir().join("berm-nesting")),
    })?;

    let berm = Berm::new(
        &engine,
        call::DEFAULT_CALL_DEPTH,
        vec![],
        storage::Memory::new(),
    );
    berm.deploy("inner", &elf)?;
    berm.deploy("outer", &elf)?;

    let result = berm
        .call("outer", "nest", br#"{"query":"hi"}"#.to_vec())?
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
        .call("outer", "nest", br#"{"query":"hi"}"#.to_vec())?
        .expect_err("a refused call fails its caller");
    println!("refused:  {failure}");
    assert!(
        failure.starts_with("refused: "),
        "the guest could not tell a refusal from a failure: {failure}"
    );

    // And the depth bound: `recurse` calls itself on `inner`, which is this
    // same image, until berm refuses to go deeper. What comes back is how many
    // levels got through — the runaway a limit exists to stop.
    berm.deploy("inner", &elf)?;
    let reached = berm
        .call("outer", "recurse", b"0".to_vec())?
        .map_err(anyhow::Error::msg)?;
    println!("depth:    {reached} levels before the runtime refused");

    println!("\nok");
    Ok(())
}
