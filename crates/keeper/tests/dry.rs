//! Dry chains through services keeping a database: every link's writes are
//! read by the links after it, and rolled back when the chain ends.

use anyhow::Result;
use berm_api::Dry;
use berm_host::test::{self, TestHost};
use serde_json::json;

const WORD: &[u8] = br#"{"word":"hi"}"#;

#[tokio::test]
async fn a_dry_chain_rolls_back_every_database_it_wrote() -> Result<()> {
    let host = TestHost::open(0).await?;
    let image = test::build("berm-keeper", "keeper")?;
    host.deploy("outer", &image).await?;
    host.deploy("inner", &image).await?;

    let dry = host.dry("outer", "relay", WORD, Dry::default()).await?;
    assert_eq!(dry.result.as_deref(), Ok("1"));
    let words = host
        .sql("inner", "SELECT COUNT(*) FROM words", json!([]))
        .await?;
    assert_eq!(words["rows"][0][0], json!(0));
    assert_eq!(host.state("outer", "last").await?, None);
    assert_eq!(host.state("inner", "last").await?, None);
    assert!(host.logs("inner", None).await?.is_empty());

    let real = host.call("outer", "relay", WORD).await?;
    assert_eq!(real.result.as_deref(), Ok("1"));
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
