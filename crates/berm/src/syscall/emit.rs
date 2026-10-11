//! `berm.emit` — a log for whoever reads this program's logs.
//!
//! berm reads and checks the request. The host takes the log through the
//! closure below, or leaves it to the chain's journal, which hands it back in
//! [`crate::Changes`].

use crate::{Callsite, Refused, Syscall, abi, journal::Log, wire};
use anyhow::Result;
use std::{future::Future, sync::Arc};

/// Serve `berm.emit`, handing each log to `take`, which answers `false` to
/// leave it to the chain's journal.
pub fn programs<T, Taken>(take: T) -> Syscall
where
    T: Fn(Callsite, Log) -> Taken + Send + Sync + 'static,
    Taken: Future<Output = Result<bool>> + Send + 'static,
{
    let take = Arc::new(take);
    Syscall {
        name: abi::EMIT.to_owned(),
        call: Arc::new(move |at: Callsite, request: Vec<u8>| {
            let take = take.clone();
            Box::pin(async move {
                if at.query {
                    return Err(Refused("a query cannot emit logs".to_owned()).into());
                }
                let fields = wire::fields(&request)?;
                let data = wire::text(&fields, 0, "data")?.to_owned();
                let topics = (1..fields.len())
                    .map(|index| Ok(wire::text(&fields, index, "topic")?.to_owned()))
                    .collect::<Result<_>>()?;
                let log = Log {
                    program: at.program.clone(),
                    topics,
                    data,
                };
                if !take(at.clone(), log.clone()).await? {
                    at.journal.emit(log)?;
                }
                Ok(Vec::new())
            })
        }),
    }
}
