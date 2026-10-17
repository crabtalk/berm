//! What a guest can reach.
//!
//! A syscall is native host code behind a name, the way a precompile is
//! native code behind an address. berm serves [`call`] itself, resolving
//! against the set it already holds.
//!
//! The standard calls — [`store`], [`emit`], [`after`], [`http`], [`sql`] —
//! are berm's to read and check, and a host's to carry out: each module turns
//! the request into a typed value and hands it to closures the embedder
//! registers with it.
//! Anything else is a [`crate::Syscall`] the embedder writes whole.
//!
//! Every one of them, berm's own included, is reached through the one table
//! below.

use crate::{Call, Caller as Asking, Callsite, Invocation, Refused, Syscall, abi, engine, wire};
use anyhow::{Result, bail};
use std::{collections::HashMap, sync::Arc};
use wasmtime::Caller;

pub mod after;
pub mod call;
pub mod emit;
pub mod http;
pub mod sql;
pub mod store;

/// One of the ABI's own doors: request bytes at `(ptr, len)` in, a staged
/// length out, answered before the guest resumes.
///
/// One signature for all of them, so the engine registers a table rather than
/// a list of shapes. A call that takes no arguments is handed a pair it
/// ignores.
pub(crate) type Handler =
    Arc<dyn Fn(&mut Caller<'_, Invocation>, u64, u64) -> Result<u64> + Send + Sync>;

/// What answers one syscall number.
#[derive(Clone)]
pub(crate) enum Door {
    /// The ABI's own, which touch nothing but the guest.
    Builtin(Handler),
    /// An embedder's, handed the request and awaited.
    Hosted(Call),
}

/// Every syscall one program can reach, by the number its name hashes to.
pub(crate) type Table = HashMap<u64, Door>;

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
        table.insert(number, Door::Hosted(syscall.call.clone()));
    }
    Ok(table)
}

/// Whether `number` is one of the ABI's own doors.
pub(crate) fn is_builtin(number: u64) -> bool {
    builtins().contains_key(&number)
}

/// Copy a hosted syscall's request out of the guest, beside the callsite it
/// came from.
pub(crate) fn request(
    guest: &mut Caller<'_, Invocation>,
    ptr: u64,
    len: u64,
) -> Result<(Callsite, Vec<u8>)> {
    let request = engine::read(guest, ptr, len)?;
    Ok((guest.data().at.clone(), request))
}

/// Leave a syscall's bytes for the guest to pull, and answer with their length.
///
/// Failure rides on the same return value: the [`abi::ERROR`] bit says the
/// staged bytes are a message. One that fails therefore costs the guest
/// nothing extra to find out about, and an empty result cannot be mistaken for
/// one. A [`Refused`] additionally sets [`abi::REFUSED`], which is how a guest
/// tells "it ran and said no" from "it never ran".
pub(crate) fn stage(guest: &mut Caller<'_, Invocation>, answer: Result<Vec<u8>>) -> u64 {
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
    guest.data_mut().staged = staged;
    length | outcome
}

/// The doors the ABI itself owns: the argument blob in, the result out, and
/// the few things a guest cannot learn any other way.
fn builtins() -> Table {
    let mut table = Table::new();

    table.insert(
        abi::HOST_TRACE,
        handler(|guest, ptr, len| {
            let message = engine::read(guest, ptr, len)?;
            tracing::info!(target: "program", "{}", String::from_utf8_lossy(&message));
            Ok(0)
        }),
    );

    table.insert(
        abi::HOST_CALLER,
        handler(|guest, _, _| {
            let reply = match &guest.data().at.caller {
                Some(Asking::Outside(name)) => wire::frame(&[b"outside", name.as_bytes()]),
                Some(Asking::Program(name)) => wire::frame(&[b"program", name.as_bytes()]),
                None => Vec::new(),
            };
            Ok(stage(guest, Ok(reply)))
        }),
    );

    table.insert(
        abi::HOST_USER,
        handler(|guest, _, _| {
            let reply = match &guest.data().at.user {
                Some(user) => wire::frame(&[user.issuer.as_bytes(), user.subject.as_bytes()]),
                None => Vec::new(),
            };
            Ok(stage(guest, Ok(reply)))
        }),
    );

    table.insert(
        abi::HOST_CLIENT,
        handler(|guest, _, _| {
            let reply = match &guest.data().at.client {
                Some(client) => wire::frame(&[client.as_bytes()]),
                None => Vec::new(),
            };
            Ok(stage(guest, Ok(reply)))
        }),
    );

    table.insert(
        abi::HOST_DENY,
        handler(|guest, _, _| {
            guest.data_mut().denied = true;
            Ok(0)
        }),
    );

    table.insert(
        abi::HOST_ARG_LEN,
        handler(|guest, _, _| Ok(guest.data().args.len() as u64)),
    );

    // Answers with the blob's full length rather than what fit, so a guest with
    // too small a buffer can tell it was truncated instead of acting on half a
    // request.
    table.insert(
        abi::HOST_ARG_READ,
        handler(|guest, ptr, capacity| {
            let args = &guest.data().args;
            let length = args.len();
            let taken = args[..length.min(capacity as usize)].to_vec();
            engine::write(guest, ptr, &taken)?;
            Ok(length as u64)
        }),
    );

    // The other half of every syscall. A program given none never stages
    // anything, so this is registered unconditionally and has nothing to hand
    // over.
    table.insert(
        abi::HOST_RESULT_READ,
        handler(|guest, ptr, capacity| {
            let staged = &guest.data().staged;
            let length = staged.len();
            let taken = staged[..length.min(capacity as usize)].to_vec();
            engine::write(guest, ptr, &taken)?;
            Ok(length as u64)
        }),
    );

    table.insert(
        abi::HOST_DONE,
        handler(|guest, ptr, len| {
            let result = engine::read(guest, ptr, len)?;
            guest.data_mut().outcome = Some(Ok(result));
            Ok(0)
        }),
    );

    table.insert(
        abi::HOST_FAIL,
        handler(|guest, ptr, len| {
            let message = engine::read(guest, ptr, len)?;
            let message = String::from_utf8_lossy(&message).into_owned();
            guest.data_mut().outcome = Some(Err(message));
            Ok(0)
        }),
    );

    table.insert(
        abi::HOST_NOW,
        handler(|guest, _, _| Ok((guest.data().now)())),
    );

    table
}

fn handler(
    call: impl Fn(&mut Caller<'_, Invocation>, u64, u64) -> Result<u64> + Send + Sync + 'static,
) -> Door {
    Door::Builtin(Arc::new(call))
}
