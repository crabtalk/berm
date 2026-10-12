//! Dry chains through services keeping a database: every link's writes are
//! read by the links after it, and rolled back when the chain ends.

use anyhow::Result;
use berm_api::Dry;
use berm_host::test::{self, TestHost};
use serde_json::{Value, json};

/// The one count in what `count` answered.
fn counted(result: &str) -> Result<Value> {
    let answer: Value = serde_json::from_str(result)?;
    Ok(answer["rows"][0][0].clone())
}

#[tokio::test]
async fn a_dry_chain_rolls_back_every_database_it_wrote() -> Result<()> {
    let host = TestHost::open(0).await?;
    let image = test::build("berm-keeper", "keeper")?;
    host.deploy("outer", &image).await?;
    host.deploy("inner", &image).await?;

    let dry = host.dry("outer", "relay", b"hi", Dry::default()).await?;
    assert_eq!(counted(dry.result.as_deref().unwrap())?, json!(1));
    let words = host
        .sql("inner", "SELECT COUNT(*) FROM words", json!([]))
        .await?;
    assert_eq!(words["rows"][0][0], json!(0));
    assert_eq!(host.state("outer", "last").await?, None);
    assert_eq!(host.state("inner", "last").await?, None);
    assert!(host.logs("inner", None).await?.is_empty());

    let real = host.call("outer", "relay", b"hi").await?;
    assert_eq!(counted(real.result.as_deref().unwrap())?, json!(1));
    assert_eq!(
        host.state("outer", "last").await?.as_deref(),
        Some(&b"hi"[..])
    );
    assert_eq!(
        host.state("inner", "last").await?.as_deref(),
        Some(&b"hi"[..])
    );
    assert_eq!(host.logs("inner", None).await?.len(), 1);
    Ok(())
}
