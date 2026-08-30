//! What a guest can reach.
//!
//! A syscall is native host code behind a name, the way a precompile is
//! native code behind an address. berm serves the ones whose whole behaviour is
//! the program model — [`call`], which resolves against the set berm already
//! holds, and [`store`], whose keyspace is whoever is asking. Where the bytes
//! land is still a host's, and arrives as an argument.
//!
//! Everything else is a [`crate::Syscall`] the embedder registers. berm ships no
//! filesystem, no command runner and no network: each needs a policy invented
//! to compile — a root, a size cap, an allowlist — and those are decisions
//! about a host.
//!
//! Every one of them, berm's own included, is written once here and reached
//! through the one table below. A backend carries that table across its own
//! boundary and supplies nothing else, which is what keeps two of them from
//! growing two ABIs.

use crate::{Callsite, Refused, Syscall, abi, backend::Guest};
use anyhow::{Result, bail};
use std::{collections::HashMap, sync::Arc};

pub mod call;
pub mod store;

/// One syscall: request bytes at `(ptr, len)` in, a staged length out.
///
/// One signature for all of them, so a backend registers a table rather than a
/// list of shapes. A call that takes no arguments is handed a pair it ignores.
pub(crate) type Handler = Arc<dyn Fn(&mut dyn Guest, u64, u64) -> Result<u64> + Send + Sync>;

/// Every syscall one program can reach, by the number its name hashes to.
pub(crate) type Table = HashMap<u64, Handler>;

/// Assemble what a program is instantiated with: the ABI's own doors, then
/// whatever the embedder registered.
///
/// A name already spoken for is refused rather than replaced. An embedder
/// cannot check for a collision itself without knowing every name berm
/// reserves, and a silent replacement would send a guest's calls somewhere its
/// author never named.
pub(crate) fn table(syscalls: &[Syscall]) -> Result<Table> {
    let mut table = builtins();
    for syscall in syscalls {
        let number = abi::hash(&syscall.name);
        if table.contains_key(&number) {
            bail!("syscall {:?} is already served", syscall.name);
        }
        let call = syscall.call.clone();
        table.insert(
            number,
            handler(move |guest, ptr, len| {
                let request = guest.read(ptr, len)?;
                let at = guest.invocation();
                let (name, depth) = (at.name.clone(), at.depth);
                let answer = call(
                    &Callsite {
                        program: &name,
                        depth,
                    },
                    &request,
                );
                Ok(stage(guest, answer))
            }),
        );
    }
    Ok(table)
}

/// Leave a syscall's bytes for the guest to pull, and answer with their length.
///
/// Failure rides on the same return value: the [`abi::ERROR`] bit says the
/// staged bytes are a message. One that fails therefore costs the guest
/// nothing extra to find out about, and an empty result cannot be mistaken for
/// one. A [`Refused`] additionally sets [`abi::REFUSED`], which is how a guest
/// tells "it ran and said no" from "it never ran".
fn stage(guest: &mut dyn Guest, answer: Result<Vec<u8>>) -> u64 {
    let (staged, outcome) = match answer {
        Ok(result) => (result, 0),
        Err(error) => {
            let refused = error
                .chain()
                .any(|cause| cause.downcast_ref::<Refused>().is_some());
            let outcome = match refused {
                true => abi::ERROR | abi::REFUSED,
                false => abi::ERROR,
            };
            (error.to_string().into_bytes(), outcome)
        }
    };
    let length = staged.len() as u64;
    guest.invocation().staged = staged;
    length | outcome
}

/// The doors the ABI itself owns: the argument blob in, the result out, and
/// the few things a guest cannot learn any other way.
fn builtins() -> Table {
    let mut table = Table::new();

    table.insert(
        abi::HOST_LOG,
        handler(|guest, ptr, len| {
            let message = guest.read(ptr, len)?;
            tracing::info!(target: "program", "{}", String::from_utf8_lossy(&message));
            Ok(0)
        }),
    );

    table.insert(
        abi::HOST_ARG_LEN,
        handler(|guest, _, _| Ok(guest.invocation().args.len() as u64)),
    );

    // Answers with the blob's full length rather than what fit, so a guest with
    // too small a buffer can tell it was truncated instead of acting on half a
    // request.
    table.insert(
        abi::HOST_ARG_READ,
        handler(|guest, ptr, capacity| {
            let args = &guest.invocation().args;
            let length = args.len();
            let taken = args[..length.min(capacity as usize)].to_vec();
            guest.write(ptr, &taken)?;
            Ok(length as u64)
        }),
    );

    // The other half of every syscall. A program given none never stages
    // anything, so this is registered unconditionally and has nothing to hand
    // over.
    table.insert(
        abi::HOST_RESULT_READ,
        handler(|guest, ptr, capacity| {
            let staged = &guest.invocation().staged;
            let length = staged.len();
            let taken = staged[..length.min(capacity as usize)].to_vec();
            guest.write(ptr, &taken)?;
            Ok(length as u64)
        }),
    );

    table.insert(
        abi::HOST_DONE,
        handler(|guest, ptr, len| {
            let result = guest.read(ptr, len)?;
            guest.invocation().outcome = Some(Ok(result));
            Ok(0)
        }),
    );

    table.insert(
        abi::HOST_FAIL,
        handler(|guest, ptr, len| {
            let message = guest.read(ptr, len)?;
            let message = String::from_utf8_lossy(&message).into_owned();
            guest.invocation().outcome = Some(Err(message));
            Ok(0)
        }),
    );

    // Saturating at the epoch: a syscall is reached across a boundary an
    // unwind would abort the process at, so it cannot be allowed to panic.
    table.insert(
        abi::HOST_NOW,
        handler(|_, _, _| {
            Ok(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_millis() as u64))
        }),
    );

    // Asked for on the guest's first allocation, from inside the entry it is
    // already in. Pushing these in would mean entering the guest a second
    // time, which costs ~13µs against ~30ns for a syscall.
    table.insert(
        abi::HOST_HEAP_START,
        handler(|guest, _, _| Ok(guest.heap()?.start)),
    );

    table.insert(
        abi::HOST_HEAP_SIZE,
        handler(|guest, _, _| {
            let heap = guest.heap()?;
            Ok(heap.end - heap.start)
        }),
    );

    table
}

fn handler(
    call: impl Fn(&mut dyn Guest, u64, u64) -> Result<u64> + Send + Sync + 'static,
) -> Handler {
    Arc::new(call)
}
