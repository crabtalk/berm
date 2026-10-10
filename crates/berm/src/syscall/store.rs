//! `berm.get` and `berm.set` — a program's own bytes, surviving its invocations.
//!
//! Neither door takes a program. The keyspace is whichever one is asking, read
//! off the [`Callsite`], so another program's keys are not refused — a guest
//! has no way to name them.
//!
//! A write is held in the chain's journal and reaches the host in
//! [`crate::Changes`]; a read sees the chain's own writes first, then asks the
//! host through the closure below.

use crate::{Callsite, Refused, Syscall, abi, wire};
use anyhow::{Result, bail};
use std::{future::Future, sync::Arc};

/// Serve both doors, reading what is committed through `read`, handed the
/// asking program and the key.
///
/// `read` answers `None` for a key never written. The guest is suspended
/// until it resolves.
pub fn programs<R, Found>(read: R) -> Vec<Syscall>
where
    R: Fn(Arc<str>, String) -> Found + Send + Sync + 'static,
    Found: Future<Output = Result<Option<Vec<u8>>>> + Send + 'static,
{
    let read = Arc::new(read);
    let get = move |at: Callsite, request: Vec<u8>| {
        let read = read.clone();
        async move {
            let fields = wire::fields(&request)?;
            let key = wire::text(&fields, 0, "key")?;
            let value = match at.journal.read(&at.program, key) {
                Some(held) => Some(held),
                None => read(at.program.clone(), key.to_owned()).await?,
            };
            // One field for a value, none for an absent key. Framed rather
            // than bare, so a stored empty value is not read as one that was
            // never written.
            Ok(match value {
                Some(value) => wire::frame(&[&value]),
                None => Vec::new(),
            })
        }
    };
    let set = move |at: &Callsite, request: &[u8]| {
        let fields = wire::fields(request)?;
        let key = wire::text(&fields, 0, "key")?;
        let Some(value) = fields.get(1) else {
            bail!("request has no value");
        };
        if at.query {
            return Err(Refused(format!("a query cannot write; {key:?} was not set")).into());
        }
        at.journal.write(&at.program, key, value)?;
        Ok(Vec::new())
    };
    vec![
        Syscall {
            name: abi::GET.to_owned(),
            call: Arc::new(move |at: Callsite, request: Vec<u8>| Box::pin(get(at, request))),
        },
        Syscall {
            name: abi::SET.to_owned(),
            call: Arc::new(move |at: Callsite, request: Vec<u8>| {
                Box::pin(std::future::ready(set(&at, &request)))
            }),
        },
    ]
}
