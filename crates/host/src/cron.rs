//! Cron expressions: five fields, minute to day of week, read in UTC.

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use croner::{
    Cron,
    parser::{CronParser, Seconds, Year},
};

/// `expression` parsed, or why it is not one.
pub(crate) fn parse(expression: &str) -> Result<Cron> {
    CronParser::builder()
        .seconds(Seconds::Disallowed)
        .year(Year::Disallowed)
        .build()
        .parse(expression)
        .map_err(|e| {
            anyhow!("{expression:?} is not a cron expression such as \"*/5 * * * *\": {e}")
        })
}

/// When `expression` next fires after `now`, both in epoch milliseconds.
pub(crate) fn next(expression: &str, now: i64) -> Result<i64> {
    let after = DateTime::<Utc>::from_timestamp_millis(now)
        .ok_or_else(|| anyhow!("{now} is not a time"))?;
    let next = parse(expression)?
        .find_next_occurrence(&after, false)
        .map_err(|e| anyhow!("{expression:?} never fires: {e}"))?;
    Ok(next.timestamp_millis())
}
