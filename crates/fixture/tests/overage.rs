//! What each invocation executes past its first `INCLUDED_INSTRUCTIONS`.

use anyhow::Result;
use berm_api::INCLUDED_INSTRUCTIONS;
use berm_host::test::{self, TestHost};

#[tokio::test]
async fn overage_is_summed_over_invocations_not_taken_off_the_day() -> Result<()> {
    let host = TestHost::open(0).await?;
    host.deploy("fixture", &test::build("berm-fixture", "fixture")?)
        .await?;

    let small = host.call("fixture", "spin", br#"{"rounds":10}"#).await?;
    assert!(small.meter.instructions < INCLUDED_INSTRUCTIONS);
    assert_eq!(small.meter.overage, 0);

    let large = host
        .call("fixture", "spin", br#"{"rounds":1000000}"#)
        .await?;
    assert!(large.result.is_ok(), "{:?}", large.result);
    assert!(large.meter.instructions > INCLUDED_INSTRUCTIONS);
    assert_eq!(
        large.meter.overage,
        large.meter.instructions - INCLUDED_INSTRUCTIONS
    );

    // A dry call is metered as the call is.
    let dry = host
        .dry(
            "fixture",
            "spin",
            br#"{"rounds":1000000}"#,
            Default::default(),
        )
        .await?;
    assert_eq!(dry.meter, large.meter);

    let days = host.host().meters("fixture").await?;
    assert_eq!(days.len(), 1);
    let day = &days[0];
    assert_eq!(
        day.instructions,
        small.meter.instructions + 2 * large.meter.instructions
    );
    assert_eq!(day.overage, 2 * large.meter.overage);
    Ok(())
}
