//! Connections, and the invocations they start.
//!
//! [`open`] names the tool a connection's events reach. From then on the
//! connection is what calls the harness: the dial's outcome, every frame that
//! arrives, and the close, each a fresh invocation of that tool. Guest memory
//! survives none of them, so a harness holding a conversation keeps it in
//! [`crate::get`]/[`crate::set`] and reads it back on the next frame.
//!
//! Whether a host serves these at all is its own decision, the same as
//! [`crate::call`]: under a plain `berm::Berm` the call traps as an unknown
//! host call.
#![cfg(feature = "alloc")]

use crate::abi::{self, host, host::CallError, wire};
use alloc::string::{String, ToString};

/// What a connection delivered, and which connection delivered it.
///
/// The id is here because one tool may serve several connections, and a frame
/// a harness cannot attribute is one it cannot answer.
pub struct Event<'a> {
    pub connection: u64,
    pub kind: Kind<'a>,
}

pub enum Kind<'a> {
    /// The dial finished. Empty when it came up, and what went wrong when it
    /// did not — a connection that never opened reports itself here, since
    /// nothing was holding the call that asked for it.
    Open(&'a str),
    /// One frame.
    Message(&'a [u8]),
    /// The far end went away, or the harness closed it.
    Close(&'a str),
}

/// Read the event out of an invocation's arguments.
///
/// `None` when this invocation came from somewhere else — a model calling the
/// tool directly, say, which a tool wired to a connection should say no to.
///
/// ```ignore
/// match berm_lang::socket::event(args) {
///     Some(Event { connection, kind: Kind::Message(frame) }) => { /* … */ }
///     _ => {}
/// }
/// ```
pub fn event(args: &[u8]) -> Option<Event<'_>> {
    let fields = wire::fields(args)?;
    let name = str::from_utf8(fields.first()?).ok()?;
    let connection = str::from_utf8(fields.get(1)?).ok()?.parse().ok()?;
    let body = fields.get(2).copied().unwrap_or_default();
    Some(Event {
        connection,
        kind: match name {
            abi::WS_EVENT_OPEN => Kind::Open(str::from_utf8(body).ok()?),
            abi::WS_EVENT_MESSAGE => Kind::Message(body),
            abi::WS_EVENT_CLOSE => Kind::Close(str::from_utf8(body).ok()?),
            _ => return None,
        },
    })
}

/// Dial `url`, delivering everything that happens on it to `harness`.`tool`.
///
/// `headers` ride on the handshake, for a service that authenticates there.
/// Pass `&[]` for one that does not, or that takes its token in the URL.
///
/// ```ignore
/// let id = socket::open(url, "me", "wire", &[("Authorization", &bearer)])?;
/// ```
///
/// Returns as soon as the connection is registered, with the id the other two
/// doors take. The dial itself outlives the call, so its outcome arrives as a
/// [`Kind::Open`] rather than as this function's error — what fails here is
/// the host refusing to dial at all.
pub fn open(
    url: &str,
    harness: &str,
    tool: &str,
    headers: &[(&str, &str)],
) -> Result<u64, CallError> {
    let mut request = wire::request(&[url.as_bytes(), harness.as_bytes(), tool.as_bytes()]);
    for (name, value) in headers {
        wire::field(&mut request, name.as_bytes());
        wire::field(&mut request, value.as_bytes());
    }
    let reply = host::call(abi::HOST_WS_OPEN, &request)?;
    str::from_utf8(&reply)
        .ok()
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| {
            CallError::Failed(String::from(
                "host named a connection this guest cannot read",
            ))
        })
}

/// Queue `payload` on a connection.
///
/// Queued, not delivered: waiting for the far end would hold this invocation
/// open for a network round trip. An `Err` means the connection is closed or
/// its queue is full.
pub fn send(id: u64, payload: &[u8]) -> Result<(), CallError> {
    let request = wire::request(&[id_text(id).as_bytes(), payload]);
    host::call(abi::HOST_WS_SEND, &request)?;
    Ok(())
}

/// Close a connection. Its [`Kind::Close`] still arrives.
pub fn close(id: u64) -> Result<(), CallError> {
    host::call(
        abi::HOST_WS_CLOSE,
        &wire::request(&[id_text(id).as_bytes()]),
    )?;
    Ok(())
}

/// An id crosses the wire as text, on the same framing every other field uses.
fn id_text(id: u64) -> String {
    id.to_string()
}
