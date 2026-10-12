//! news's image, run against every desk's feed recorded on 2026-10-12.

#![cfg(feature = "replay")]

use anyhow::{Context, Result};
use berm_host::test::{self, TestHost};
use std::{fs, path::Path};

/// When the last feed was recorded, in Unix milliseconds.
const RECORDED: u64 = 1_791_785_405_000;

/// Five minutes, between ticks.
const TICK: u64 = 5 * 60 * 1000;

/// Answer each desk's URL from `tests/recorded`, where `urls.tsv` names the
/// file each was recorded to, and the Anthropic articles its index links from
/// `anthropic/`. Answers how many URLs were arranged.
fn arrange(host: &TestHost) -> Result<usize> {
    let recorded = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/recorded");
    let urls = fs::read_to_string(recorded.join("urls.tsv"))?;
    let mut desks = 0;
    for line in urls.lines() {
        let (file, url) = line
            .split_once('\t')
            .context("urls.tsv: not file, tab, URL")?;
        host.arrange("GET", url, 200, fs::read(recorded.join(file))?);
        desks += 1;
    }
    for article in fs::read_dir(recorded.join("anthropic"))? {
        let article = article?;
        host.arrange(
            "GET",
            &format!(
                "https://www.anthropic.com/news/{}",
                article.file_name().to_string_lossy()
            ),
            200,
            fs::read(article.path())?,
        );
        desks += 1;
    }
    Ok(desks)
}

#[tokio::test]
async fn ticks_fetch_every_desk_once() -> Result<()> {
    let host = TestHost::open(RECORDED).await?;
    let arranged = arrange(&host)?;
    assert_eq!(arranged, 116);
    host.deploy("news", &test::build("news", "news")?).await?;
    assert_eq!(
        host.call("news", "start", b"{}").await?.result.as_deref(),
        Ok("started")
    );

    // Ten desks a tick, least recently fetched first, each within a call's
    // budget.
    for tick in 1..=11 {
        let fired = host.fire().await?;
        assert_eq!(fired.len(), 1, "tick {tick}");
        assert_eq!(fired[0].1.result.as_deref(), Ok(""), "tick {tick}");
        host.advance(TICK);
    }
    assert_eq!(host.fetched().len(), arranged);
    assert_eq!(host.logs("news", Some("Article")).await?.len(), 4444);
    Ok(())
}
