//! A tool's terms, set live by its owner, on the fixture's image.

use anyhow::Result;
use berm_api::Budget;
use berm_host::test::{self, TestHost};

#[tokio::test]
async fn a_budget_is_published_in_the_terms() -> Result<()> {
    let host = TestHost::open(0).await?;
    host.deploy("fixture", &test::build("berm-fixture", "fixture")?)
        .await?;
    let host = host.host();

    assert_eq!(host.terms("fixture")?.tools["echo"].budget, host.budget);

    let budget = Budget {
        instructions: 100_000,
        rows: 10,
    };
    host.set_budget("fixture", "echo", budget).await?;
    assert_eq!(host.terms("fixture")?.tools["echo"].budget, budget);
    assert_eq!(host.terms("fixture")?.tools["carry"].budget, host.budget);
    Ok(())
}
