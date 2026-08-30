//! `berm.get` and `berm.set` — a program's own bytes, surviving its invocations.
//!
//! Neither door takes a program. The keyspace is whichever one is asking, read
//! off the [`Callsite`], so another program's keys are not refused — a guest
//! has no way to name them.
//!
//! Where the bytes land is the host's, and arrives as the two closures below.

use crate::{Callsite, Syscall, abi, wire};
use anyhow::{Result, bail};
use std::sync::Arc;

/// Serve both doors against `read` and `write`, each handed the asking program
/// and the key.
///
/// `read` answers `None` for a key never written. Neither may panic, being
/// reached from compiled guest code across an `extern "C"` boundary where an
/// unwind aborts the process.
pub fn programs(
    read: impl Fn(&str, &str) -> Result<Option<Vec<u8>>> + Send + Sync + 'static,
    write: impl Fn(&str, &str, &[u8]) -> Result<()> + Send + Sync + 'static,
) -> Vec<Syscall> {
    vec![
        Syscall {
            name: abi::GET.to_owned(),
            call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
                let fields = wire::fields(request)?;
                let key = wire::text(&fields, 0, "key")?;
                // One field for a value, none for an absent key. Framed rather
                // than bare, so a stored empty value is not read as one that
                // was never written.
                Ok(match read(at.program, key)? {
                    Some(value) => wire::frame(&[&value]),
                    None => Vec::new(),
                })
            }),
        },
        Syscall {
            name: abi::SET.to_owned(),
            call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
                let fields = wire::fields(request)?;
                let key = wire::text(&fields, 0, "key")?;
                let Some(value) = fields.get(1) else {
                    bail!("request has no value");
                };
                write(at.program, key, value)?;
                Ok(Vec::new())
            }),
        },
    ]
}
