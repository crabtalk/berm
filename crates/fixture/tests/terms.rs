//! A tool's terms, set live by its owner, on the fixture's image.

use anyhow::Result;
use berm_api::Budget;
use berm_host::test::{self, TestHost};

#[tokio::test]
async fn a_price_is_published_beside_the_budget() -> Result<()> {
    let host = TestHost::open(0).await?;
    host.deploy("fixture", &test::build("berm-fixture", "fixture")?)
        .await?;
    let host = host.host();

    let echo = host.terms("fixture")?.tools["echo"];
    assert_eq!(echo.price_micros, 0);
    assert_eq!(echo.budget, host.budget);

    host.set_price("fixture", "echo", 250).await?;
    let budget = Budget {
        instructions: 100_000,
        rows: 10,
    };
    host.set_budget("fixture", "echo", budget).await?;
    let echo = host.terms("fixture")?.tools["echo"];
    assert_eq!((echo.price_micros, echo.budget), (250, budget));
    assert_eq!(host.terms("fixture")?.tools["carry"].price_micros, 0);

    assert!(host.set_price("fixture", "nothing", 1).await.is_err());
    assert!(host.set_price("fixture", "echo", u64::MAX).await.is_err());
    assert_eq!(host.terms("fixture")?.tools["echo"].price_micros, 250);
    Ok(())
}
