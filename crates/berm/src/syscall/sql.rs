//! `berm.sql` — one statement against the asking program's own database.
//!
//! berm reads the request; running it is the host's, and arrives as the
//! closure below. [`crate::database`] is one, over SQLite.

use crate::{Callsite, Syscall, abi, wire};
use anyhow::Result;
use std::{future::Future, sync::Arc};

/// One statement, as the host is handed it.
#[derive(Debug)]
pub struct Statement {
    pub sql: String,
    /// A JSON array bound to the statement's `?` placeholders.
    pub params: String,
}

/// Serve `berm.sql` against `exec`, handed the callsite and the statement and
/// answering the reply's JSON. A statement under a `query` ([`Callsite::query`])
/// may only read.
pub fn programs<E, Ran>(exec: E) -> Syscall
where
    E: Fn(Callsite, Statement) -> Ran + Send + Sync + 'static,
    Ran: Future<Output = Result<String>> + Send + 'static,
{
    let exec = Arc::new(exec);
    Syscall {
        name: abi::SQL.to_owned(),
        call: Arc::new(move |at: Callsite, request: Vec<u8>| {
            let exec = exec.clone();
            Box::pin(async move {
                let fields = wire::fields(&request)?;
                at.rows_left()?;
                let statement = Statement {
                    sql: wire::text(&fields, 0, "statement")?.to_owned(),
                    params: wire::text(&fields, 1, "parameters")?.to_owned(),
                };
                Ok(exec(at, statement).await?.into_bytes())
            })
        }),
    }
}
