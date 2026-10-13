//! hn's image, run against Algolia's answers recorded on 2026-10-12.

#![cfg(feature = "replay")]

use anyhow::Result;
use berm_host::test::{self, TestHost};
use std::{fs, path::Path};

/// When the answers were recorded, in Unix milliseconds.
const RECORDED: u64 = 1_791_747_341_000;

const API: &str = "https://hn.algolia.com/api/v1";

/// Each lane's Algolia tag and page size.
const LANES: [(&str, u32); 4] = [
    ("front_page", 200),
    ("story", 30),
    ("ask_hn", 30),
    ("show_hn", 30),
];

/// Answer every request hn makes from `tests/recorded`.
fn arrange(host: &TestHost) -> Result<()> {
    let recorded = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/recorded");
    for (tag, size) in LANES {
        host.arrange(
            "GET",
            &format!("{API}/search_by_date?tags={tag}&hitsPerPage={size}"),
            200,
            fs::read(recorded.join(format!("search_{tag}.json")))?,
        );
    }
    for item in fs::read_dir(recorded.join("items"))? {
        let item = item?;
        host.arrange(
            "GET",
            &format!("{API}/items/{}", item.file_name().to_string_lossy()),
            200,
            fs::read(item.path())?,
        );
    }
    Ok(())
}

/// Five minutes, between ticks.
const TICK: u64 = 5 * 60 * 1000;

#[tokio::test]
async fn ticks_log_stories_then_threads() -> Result<()> {
    let host = TestHost::open(RECORDED).await?;
    arrange(&host)?;
    host.deploy("hn", &test::build("hn", "hn")?).await?;

    let started = host.call("hn", "start", b"{}").await?;
    assert_eq!(started.result.as_deref(), Ok(r#""started""#));
    let wakes = host.wakes().await?;
    assert_eq!(wakes.len(), 1);
    assert_eq!((wakes[0].0.tool.as_str(), wakes[0].1), ("tick", RECORDED));

    // The first tick logs every story the lanes show, and the threads of the
    // first 30 watched; only the Ask HN threads were recorded.
    let fired = host.fire().await?;
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].1.result.as_deref(), Ok(""));
    assert!(fired[0].1.meter.instructions > 0);
    assert_eq!(host.logs("hn", Some("Story")).await?.len(), 120);
    assert_eq!(host.logs("hn", Some("Thread")).await?.len(), 1);
    assert_eq!(host.wakes().await?[0].1, RECORDED + TICK);
    assert!(host.fire().await?.is_empty());

    // The second fetches the threads the first did not reach.
    host.advance(TICK);
    assert_eq!(host.fire().await?.len(), 1);
    assert_eq!(host.logs("hn", Some("Story")).await?.len(), 120);
    assert_eq!(host.logs("hn", Some("Thread")).await?.len(), 21);

    // The third finds nothing new: the listings are unchanged and no thread
    // is due again for two hours.
    host.advance(TICK);
    assert_eq!(host.fire().await?.len(), 1);
    assert_eq!(host.logs("hn", None).await?.len(), 141);
    Ok(())
}
