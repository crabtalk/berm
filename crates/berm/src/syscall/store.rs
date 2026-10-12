//! `berm.get` and `berm.set` — a program's own bytes, surviving its invocations.
//!
//! Neither door takes a program. The keyspace is whichever one is asking, read
//! off the [`Callsite`], so another program's keys are not refused — a guest
//! has no way to name them.
//!
//! The host takes a write through the closure below, or leaves it to the
//! chain's journal, which hands it back in [`crate::Changes`]. A read sees the
//! chain's journal first, then asks the host. Each read is metered as one row
//! read and each write as one row written.

use crate::{Callsite, Meter, Refused, Syscall, abi, wire};
use anyhow::{Result, bail};
use std::{future::Future, sync::Arc};

/// Serve both doors, reading through `read`, handed the asking site and the
/// key, and handing each write to `write`, which answers `false` to leave it
/// to the chain's journal.
///
/// `read` answers `None` for a key never written. The guest is suspended
/// until it resolves.
pub fn programs<R, Found, W, Taken>(read: R, write: W) -> Vec<Syscall>
where
    R: Fn(Callsite, String) -> Found + Send + Sync + 'static,
    Found: Future<Output = Result<Option<Vec<u8>>>> + Send + 'static,
    W: Fn(Callsite, String, Vec<u8>) -> Taken + Send + Sync + 'static,
    Taken: Future<Output = Result<bool>> + Send + 'static,
{
    let (read, write) = (Arc::new(read), Arc::new(write));
    let get = move |at: Callsite, request: Vec<u8>| {
        let read = read.clone();
        async move {
            let fields = wire::fields(&request)?;
            let key = wire::text(&fields, 0, "key")?;
            at.rows_left()?;
            let value = match at.journal.read(&at.program, key) {
                Some(held) => Some(held),
                None => read(at.clone(), key.to_owned()).await?,
            };
            at.meter(Meter {
                read: 1,
                ..Meter::default()
            });
            // One field for a value, none for an absent key. Framed rather
            // than bare, so a stored empty value is not read as one that was
            // never written.
            Ok(match value {
                Some(value) => wire::frame(&[&value]),
                None => Vec::new(),
            })
        }
    };
    let set = move |at: Callsite, request: Vec<u8>| {
        let write = write.clone();
        async move {
            let fields = wire::fields(&request)?;
            let key = wire::text(&fields, 0, "key")?;
            let Some(value) = fields.get(1) else {
                bail!("request has no value");
            };
            if at.query {
                return Err(Refused(format!("a query cannot write; {key:?} was not set")).into());
            }
            at.rows_left()?;
            if !write(at.clone(), key.to_owned(), value.to_vec()).await? {
                at.journal.write(&at.program, key, value)?;
            }
            at.meter(Meter {
                written: 1,
                ..Meter::default()
            });
            Ok(Vec::new())
        }
    };
    vec![
        Syscall {
            name: abi::GET.to_owned(),
            call: Arc::new(move |at: Callsite, request: Vec<u8>| Box::pin(get(at, request))),
        },
        Syscall {
            name: abi::SET.to_owned(),
            call: Arc::new(move |at: Callsite, request: Vec<u8>| Box::pin(set(at, request))),
        },
    ]
}
