//! reddit's image, run against r/rust's feeds recorded on 2026-10-12.

#![cfg(feature = "replay")]

use anyhow::Result;
use berm_host::test::{self, TestHost};
use std::{fs, path::Path};

/// When the listing was recorded, in Unix milliseconds.
const RECORDED: u64 = 1_791_750_451_000;

/// Five minutes, between ticks.
const TICK: u64 = 5 * 60 * 1000;

const SUB: &str = "https://www.reddit.com/r/rust";

/// Answer r/rust's hot listing and the comments recorded under it from
/// `tests/recorded`.
fn arrange(host: &TestHost) -> Result<()> {
    let recorded = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/recorded");
    host.arrange(
        "GET",
        &format!("{SUB}/hot.rss?limit=25"),
        200,
        fs::read(recorded.join("hot.rss"))?,
    );
    for comments in fs::read_dir(recorded.join("comments"))? {
        let path = comments?.path();
        let id = path.file_stem().unwrap_or_default().to_string_lossy();
        host.arrange(
            "GET",
            &format!("{SUB}/comments/{id}/.rss?sort=top&limit=100"),
            200,
            fs::read(&path)?,
        );
    }
    Ok(())
}

#[tokio::test]
async fn first_tick_logs_posts_and_comments() -> Result<()> {
    let host = TestHost::open(RECORDED).await?;
    arrange(&host)?;
    host.deploy("reddit", &test::build("reddit")?).await?;

    let follow = br#"{"sub":"rust","sort":"hot"}"#;
    assert_eq!(
        host.call("reddit", "follow", follow)
            .await?
            .result
            .as_deref(),
        Ok("followed")
    );
    assert_eq!(
        host.call("reddit", "follow", follow)
            .await?
            .result
            .as_deref(),
        Ok("following")
    );
    assert_eq!(
        host.call("reddit", "start", b"{}").await?.result.as_deref(),
        Ok("started")
    );

    let fired = host.fire().await?;
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].1.result.as_deref(), Ok(""));
    assert_eq!(host.logs("reddit", Some("Post")).await?.len(), 25);
    assert_eq!(host.logs("reddit", Some("Comments")).await?.len(), 3);
    assert_eq!(host.wakes().await?[0].1, RECORDED + TICK);

    // A tick ran within the last three, so starting again does nothing.
    assert_eq!(
        host.call("reddit", "start", b"{}").await?.result.as_deref(),
        Ok("running")
    );
    Ok(())
}
