//! `berm.call.after` — a tool called later, by the program's own arming.
//!
//! berm reads the request; holding the wake, writing it down so a restart
//! keeps it, and firing it are the host's, and arrive as the closure below.
//! Each wake armed is metered as one row written.

use crate::{Callsite, Meter, Refused, Syscall, abi, wire};
use anyhow::{Context, Result};
use std::{future::Future, sync::Arc};

/// One wake a program asked for.
#[derive(Debug)]
pub struct Wake {
    /// Milliseconds from the call.
    pub delay: u64,
    pub program: String,
    pub tool: String,
    pub args: String,
}

/// Serve `berm.call.after` against `arm`, handed the callsite and the wake.
///
/// One wake per program that arms it: arming again replaces what that program
/// had pending. `arm` answers once the wake is held; an `Err` reaches the
/// guest, which is still running and can act on it.
pub fn programs<A, Armed>(arm: A) -> Syscall
where
    A: Fn(Callsite, Wake) -> Armed + Send + Sync + 'static,
    Armed: Future<Output = Result<()>> + Send + 'static,
{
    Syscall {
        name: abi::CALL_AFTER.to_owned(),
        call: Arc::new(move |at: Callsite, request: Vec<u8>| match read(&request) {
            Ok(_) if at.query => Box::pin(std::future::ready(Err(Refused(
                "a query cannot arm a wake".to_owned(),
            )
            .into()))),
            Ok(wake) => {
                if let Err(error) = at.rows_left() {
                    return Box::pin(std::future::ready(Err(error)));
                }
                let armed = arm(at.clone(), wake);
                Box::pin(async move {
                    armed.await?;
                    at.meter(Meter {
                        written: 1,
                        ..Meter::default()
                    });
                    Ok(Vec::new())
                })
            }
            Err(error) => Box::pin(std::future::ready(Err(error))),
        }),
    }
}

fn read(request: &[u8]) -> Result<Wake> {
    let fields = wire::fields(request)?;
    Ok(Wake {
        delay: wire::text(&fields, 0, "delay")?
            .parse()
            .context("a delay is milliseconds")?,
        program: wire::text(&fields, 1, "program")?.to_owned(),
        tool: wire::text(&fields, 2, "tool")?.to_owned(),
        args: wire::text(&fields, 3, "arguments")?.to_owned(),
    })
}
