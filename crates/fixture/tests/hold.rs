//! A held scope's services: no wakes and no calls from services or peers, and
//! everything they had waiting runs once the scope is released.

use anyhow::Result;
use berm_api::host::Link;
use berm_host::test::{self, TestHost};

async fn deployed(services: &[&str]) -> Result<TestHost> {
    let host = TestHost::open(0).await?;
    let image = test::build("berm-fixture", "fixture")?;
    for service in services {
        host.deploy(service, &image).await?;
    }
    Ok(host)
}

#[tokio::test]
async fn a_held_wake_stays_and_fires_once_released() -> Result<()> {
    let host = deployed(&["acct/fixture", "other/fixture"]).await?;
    host.call("acct/fixture", "tick", br#"{"delay":0}"#).await?;
    host.call("other/fixture", "tick", br#"{"delay":0}"#)
        .await?;

    host.host().hold(vec!["acct".into()]).await?;
    assert_eq!(host.host().held(), vec!["acct".to_owned()]);
    let fired = host.fire().await?;
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].0.service, "other/fixture");
    assert_eq!(host.wakes().await?.len(), 2);

    host.host().hold(Vec::new()).await?;
    let fired = host.fire().await?;
    assert_eq!(fired.len(), 2);
    assert!(fired.iter().any(|(wake, _)| wake.service == "acct/fixture"));
    Ok(())
}

#[tokio::test]
async fn a_held_scope_takes_no_calls_from_services_or_peers() -> Result<()> {
    let host = deployed(&["acct/fixture", "acct/inner"]).await?;
    host.host().hold(vec!["acct".into()]).await?;

    let nested = host
        .call("acct/fixture", "nest", br#"{"query":"hi"}"#)
        .await?;
    let failure = nested
        .result
        .expect_err("a call into a held scope is refused");
    assert!(failure.contains("held"), "{failure}");

    let link = Link {
        caller: None,
        from: "acct/elsewhere".into(),
        user: None,
        client: None,
        depth: 1,
        query: false,
        timeout: 5_000,
        stack: vec!["acct/fixture".into()],
        dry: None,
    };
    let refused = host
        .host()
        .link(
            "acct/fixture",
            "echo",
            br#"{"query":"hi"}"#.to_vec(),
            link.clone(),
        )
        .await
        .expect_err("a link into a held scope is refused");
    assert_eq!(refused.0.as_u16(), 403);

    host.host().hold(Vec::new()).await?;
    assert!(
        host.call("acct/fixture", "nest", br#"{"query":"hi"}"#)
            .await?
            .result
            .is_ok()
    );
    assert!(
        host.host()
            .link("acct/fixture", "echo", br#"{"query":"hi"}"#.to_vec(), link)
            .await
            .is_ok()
    );
    Ok(())
}
