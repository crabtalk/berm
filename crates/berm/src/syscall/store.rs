//! `berm.get` and `berm.set` — a program's own bytes, surviving its invocations.
//!
//! Neither door takes a program. The keyspace is whichever one is asking, read
//! off the [`Callsite`], so another program's keys are not refused — a guest
//! has no way to name them.
//!
//! A write is held in the chain's journal and reaches the host in
//! [`crate::Changes`]; a read sees the chain's own writes first, then asks the
//! host through the closure below.

use crate::{Callsite, Syscall, abi, wire};
use anyhow::{Result, bail};
use std::sync::Arc;

/// Serve both doors, reading what is committed through `read`, handed the
/// asking program and the key.
///
/// `read` answers `None` for a key never written. It runs to completion
/// before the guest resumes, on the thread the guest was suspended on, and may
/// not panic, being reached from compiled guest code across an `extern "C"`
/// boundary where an unwind aborts the process.
pub fn programs(
    read: impl Fn(&str, &str) -> Result<Option<Vec<u8>>> + Send + Sync + 'static,
) -> Vec<Syscall> {
    let get = move |at: &Callsite, request: &[u8]| {
        let fields = wire::fields(request)?;
        let key = wire::text(&fields, 0, "key")?;
        let value = match at.journal.read(&at.program, key) {
            Some(held) => Some(held),
            None => read(&at.program, key)?,
        };
        // One field for a value, none for an absent key. Framed rather than
        // bare, so a stored empty value is not read as one that was never
        // written.
        Ok(match value {
            Some(value) => wire::frame(&[&value]),
            None => Vec::new(),
        })
    };
    let set = move |at: &Callsite, request: &[u8]| {
        let fields = wire::fields(request)?;
        let key = wire::text(&fields, 0, "key")?;
        let Some(value) = fields.get(1) else {
            bail!("request has no value");
        };
        at.journal.write(&at.program, key, value)?;
        Ok(Vec::new())
    };
    vec![
        Syscall {
            name: abi::GET.to_owned(),
            call: Arc::new(move |at: Callsite, request: Vec<u8>| {
                Box::pin(std::future::ready(get(&at, &request)))
            }),
        },
        Syscall {
            name: abi::SET.to_owned(),
            call: Arc::new(move |at: Callsite, request: Vec<u8>| {
                Box::pin(std::future::ready(set(&at, &request)))
            }),
        },
    ]
}
