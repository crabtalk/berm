//! `berm.emit` — a log for whoever reads this program's logs.
//!
//! berm serves this one itself: a log is held in the chain's journal and
//! reaches the host in [`crate::Changes`], so there is nothing to hand an
//! embedder while the chain runs.

use crate::{Callsite, journal::Log, wire};
use anyhow::Result;

/// Hold the log `request` describes. Fields are the data, then each topic.
pub(crate) fn emit(at: &Callsite, request: &[u8]) -> Result<Vec<u8>> {
    let fields = wire::fields(request)?;
    let data = wire::text(&fields, 0, "data")?.to_owned();
    let topics = (1..fields.len())
        .map(|index| Ok(wire::text(&fields, index, "topic")?.to_owned()))
        .collect::<Result<_>>()?;
    at.journal.emit(Log {
        program: at.program.clone(),
        topics,
        data,
    })?;
    Ok(Vec::new())
}
