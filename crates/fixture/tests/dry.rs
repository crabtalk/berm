//! Dry calls on the fixture's image: nothing kept, and the meter a real call
//! answers.

use anyhow::Result;
use berm_api::{Answer, Dry};
use berm_host::test::{self, TestHost};

async fn deployed() -> Result<TestHost> {
    let host = TestHost::open(0).await?;
    host.deploy("fixture", &test::build("berm-fixture", "fixture")?)
        .await?;
    Ok(host)
}

#[tokio::test]
async fn a_dry_call_keeps_nothing_and_costs_what_the_call_does() -> Result<()> {
    let host = deployed().await?;

    let dry = host.dry("fixture", "note", b"hi", Dry::default()).await?;
    assert_eq!(host.state("fixture", "note").await?, None);
    assert!(host.logs("fixture", None).await?.is_empty());

    let real = host.call("fixture", "note", b"hi").await?;
    assert_eq!(dry.result, real.result);
    assert_eq!(dry.meter, real.meter);
    assert_eq!(
        host.state("fixture", "note").await?.as_deref(),
        Some(&b"hi"[..])
    );
    assert_eq!(host.logs("fixture", None).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_dry_call_arms_no_wake() -> Result<()> {
    let host = deployed().await?;
    let dry = host.dry("fixture", "tick", b"1000", Dry::default()).await?;
    assert!(dry.result.is_ok(), "{:?}", dry.result);
    assert!(host.wakes().await?.is_empty());
    assert_eq!(host.state("fixture", "ticks").await?, None);

    host.call("fixture", "tick", b"1000").await?;
    assert_eq!(host.wakes().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_dry_fetch_is_answered_sent_or_refused() -> Result<()> {
    let host = deployed().await?;
    let url = "http://127.0.0.1/feed";
    let answered = Dry {
        answers: vec![Answer {
            method: "GET".into(),
            url: url.into(),
            status: 200,
            headers: Vec::new(),
            body: "hello".into(),
        }],
        offline: false,
    };
    let ran = host
        .dry("fixture", "fetch", url.as_bytes(), answered)
        .await?;
    assert_eq!(ran.result.as_deref(), Ok(r#"{"status":200,"bytes":5}"#));
    assert!(host.fetched().is_empty());

    let offline = Dry {
        answers: Vec::new(),
        offline: true,
    };
    let ran = host
        .dry("fixture", "fetch", url.as_bytes(), offline)
        .await?;
    assert!(ran.result.is_err());
    assert!(host.fetched().is_empty());

    // Unanswered and not offline, the request is sent as a call sends it.
    host.dry("fixture", "fetch", url.as_bytes(), Dry::default())
        .await?;
    assert_eq!(host.fetched(), vec![("GET".to_owned(), url.to_owned())]);
    Ok(())
}

#[tokio::test]
async fn a_dry_call_from_outside_is_not_counted_as_a_use() -> Result<()> {
    let host = deployed().await?;
    let host = host.host();
    let (_, output) = host
        .dry_run(
            "fixture",
            None,
            "echo",
            br#"{"query":"hi"}"#.to_vec(),
            Dry::default(),
        )
        .await?;
    assert!(matches!(output, berm_api::Output::Done(_)));
    assert!(host.usage("fixture").await?.is_empty());

    host.call(
        "fixture",
        None,
        "echo",
        br#"{"query":"hi"}"#.to_vec(),
        false,
    )
    .await?;
    assert_eq!(host.usage("fixture").await?.len(), 1);
    Ok(())
}
