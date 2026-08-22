//! A harness calling a harness.
//!
//! The reference guest exports `nest`, which calls `echo` on whatever the host
//! answers for as `inner`. Here that is a second [`Berm`] built from the same
//! ELF, which is all a nested call is: a system harness that happens to enter
//! another guest.
//!
//! ```sh
//! cargo build --release -p berm-fixture --target riscv64imac-unknown-none-elf
//! cargo run --release --example nesting -p berm
//! ```

use anyhow::{Context, Result};
use berm::{Berm, Harness, Refused, wire};
use rvtime::{Config, Engine};
use std::{fs, path::PathBuf, sync::Arc};

const GUEST: &str = "target/riscv64imac-unknown-none-elf/release/fixture";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn,harness=info"))
        .init();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("no workspace root")?
        .to_path_buf();
    let elf = fs::read(root.join(GUEST)).with_context(|| {
        format!("build the guest first: cargo build --release -p berm-fixture --target riscv64imac-unknown-none-elf ({GUEST})")
    })?;

    let mut config = Config::new();
    config.cache_dir(std::env::temp_dir().join("berm-nesting"));
    let engine = Engine::new(&config)?;

    // What the name resolves to. A daemon looks this up in what it has deployed
    // on every call; here it is one harness, held.
    let inner = Arc::new(Berm::load(&engine, &elf, &[])?);

    let outer = Berm::load(&engine, &elf, &[echo(inner.clone())])?;
    let result = outer
        .call("nest", br#"{"query":"hi"}"#.to_vec())?
        .map_err(anyhow::Error::msg)?;
    println!("reached:  {result}");
    assert!(
        result.contains(r#"{"query":"hi"}"#),
        "the nested call lost the payload: {result}"
    );

    // The other half of the wire: a refusal is not the target's own failure,
    // and the calling guest is told which it got.
    let refused = Berm::load(&engine, &elf, &[missing()])?;
    let failure = refused
        .call("nest", br#"{"query":"hi"}"#.to_vec())?
        .expect_err("a refused call fails its caller");
    println!("refused:  {failure}");
    assert!(
        failure.starts_with("refused: "),
        "the guest could not tell a refusal from a failure: {failure}"
    );

    // And the target running and saying no reaches the caller as the other
    // kind, through the same call.
    let failing = Berm::load(&engine, &elf, &[boom()])?;
    let failure = failing
        .call("nest", br#"{"query":"hi"}"#.to_vec())?
        .expect_err("a failing target fails its caller");
    println!("failed:   {failure}");
    assert!(
        !failure.starts_with("refused: "),
        "a target that ran was reported as never having run: {failure}"
    );

    println!("\nok");
    Ok(())
}

/// `berm.call`, answered by another compiled harness. A daemon looks the name
/// up in what it has deployed; here there is one, so any name reaches it.
fn echo(inner: Arc<Berm>) -> Harness {
    Harness {
        name: berm::abi::CALL.to_owned(),
        call: Arc::new(move |request: &[u8]| {
            let fields = wire::fields(request)?;
            let tool = wire::text(&fields, 1, "tool")?;
            let args = wire::text(&fields, 2, "arguments")?;
            match inner.call(tool, args.as_bytes().to_vec())? {
                Ok(result) => Ok(result.into_bytes()),
                Err(failure) => anyhow::bail!(failure),
            }
        }),
    }
}

/// Nothing deployed under the name the guest asked for.
fn missing() -> Harness {
    Harness {
        name: berm::abi::CALL.to_owned(),
        call: Arc::new(|_: &[u8]| {
            Err(Refused("no harness named \"inner\" is deployed".into()).into())
        }),
    }
}

/// Something that ran and reported failure.
fn boom() -> Harness {
    Harness {
        name: berm::abi::CALL.to_owned(),
        call: Arc::new(|_: &[u8]| anyhow::bail!("the target said no")),
    }
}
