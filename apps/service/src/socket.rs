//! Connections as a source of invocations.
//!
//! A harness opens one and names the tool its events reach. From then on the
//! connection is what starts invocations: the dial's outcome, each frame that
//! arrives, and the close. Guest memory does not survive any of them, so a
//! harness holding a conversation keeps it in `berm.get`/`berm.set` and reads
//! it back on the next frame.
//!
//! berm serves none of this itself. A dialer needs an allowlist and a frame
//! cap to compile at all, and those are decisions about a host.

use crate::Service;
use anyhow::{Result, bail};
use berm::{Callsite, System, abi, wire};
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{runtime::Handle, sync::mpsc};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

/// What a host decides before a guest can reach the network.
///
/// [`Default`] is every door shut: an empty allowlist refuses each dial before
/// any of the caps below it is read, so an embedder that says nothing grants
/// nothing.
#[derive(Clone, Default)]
pub struct Policy {
    /// Hosts a harness may dial.
    pub allow: Vec<String>,
    pub max_frame: usize,
    pub max_connections: usize,
    /// How many frames may be waiting to go out before a send is refused.
    pub queue: usize,
}

/// Every open connection, and the counter that names the next one.
#[derive(Default)]
pub(crate) struct Sockets {
    /// A `std` lock because this is read from inside a guest's host call,
    /// where an async one cannot go.
    open: Mutex<HashMap<u64, Connection>>,
    next: AtomicU64,
}

struct Connection {
    /// Who opened it. The id of a connection another harness opened resolves
    /// to nothing, the way another harness's keys are unaddressable.
    harness: Arc<str>,
    /// Dropping this is what closes the connection: the task reading the far
    /// end sees its receiver end and shuts the socket down.
    outbound: mpsc::Sender<Message>,
}

impl Sockets {
    fn owned_by(&self, harness: &str, id: u64) -> Result<mpsc::Sender<Message>> {
        let Ok(open) = self.open.lock() else {
            bail!("the connection table is poisoned");
        };
        match open.get(&id) {
            Some(connection) if &*connection.harness == harness => Ok(connection.outbound.clone()),
            // One message for both, so a probe cannot tell an id that is not
            // yours from one that does not exist.
            _ => bail!("no connection {id} is open"),
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut open) = self.open.lock() {
            open.remove(&id);
        }
    }
}

/// `berm.ws.open`, `berm.ws.send` and `berm.ws.close`, against `service`.
///
/// `Weak` for the reason berm holds its own runtime that way: a connection
/// task reaches back into the service that owns the table it lives in.
pub(crate) fn system(service: Weak<Service>, policy: Policy, runtime: Handle) -> Vec<System> {
    let (sending, closing) = (service.clone(), service.clone());
    vec![
        System {
            name: abi::WS_OPEN.to_owned(),
            call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
                let fields = wire::fields(request)?;
                let url = wire::text(&fields, 0, "url")?;
                let harness = wire::text(&fields, 1, "harness")?;
                let tool = wire::text(&fields, 2, "tool")?;

                let Some(runtime_service) = service.upgrade() else {
                    bail!("the service is shutting down, so {url} was not dialled");
                };
                permitted(&policy, url)?;

                let id = open(
                    &runtime_service,
                    &policy,
                    &runtime,
                    at.harness,
                    url,
                    harness,
                    tool,
                )?;
                Ok(id.to_string().into_bytes())
            }),
        },
        System {
            name: abi::WS_SEND.to_owned(),
            call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
                let fields = wire::fields(request)?;
                let id = wire::text(&fields, 0, "connection")?.parse()?;
                let Some(payload) = fields.get(1) else {
                    bail!("request has no payload");
                };
                let Some(service) = sending.upgrade() else {
                    bail!("the service is shutting down, so nothing was sent");
                };

                // `try_send`, never a blocking one. This runs on the thread
                // inside the guest, and the task that would drain the queue is
                // the same one awaiting this very invocation — a send that
                // waited for room would wait for itself.
                service
                    .sockets
                    .owned_by(at.harness, id)?
                    .try_send(frame(payload))
                    .map_err(|_| {
                        anyhow::anyhow!("connection {id} is closed or its queue is full")
                    })?;
                Ok(Vec::new())
            }),
        },
        System {
            name: abi::WS_CLOSE.to_owned(),
            call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
                let id = wire::text(&wire::fields(request)?, 0, "connection")?.parse()?;
                let Some(service) = closing.upgrade() else {
                    bail!("the service is shutting down");
                };
                // Checked first, so one harness cannot close another's.
                service.sockets.owned_by(at.harness, id)?;
                service.sockets.forget(id);
                Ok(Vec::new())
            }),
        },
    ]
}

/// Text when the payload is UTF-8. Every API a harness is likely to hold a
/// connection to speaks JSON over text frames.
fn frame(payload: &[u8]) -> Message {
    match str::from_utf8(payload) {
        Ok(text) => Message::text(text),
        Err(_) => Message::binary(payload.to_vec()),
    }
}

fn permitted(policy: &Policy, url: &str) -> Result<()> {
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|authority| authority.rsplit('@').next().unwrap_or(authority))
        .and_then(|authority| authority.split(':').next())
        .unwrap_or_default();

    if host.is_empty() {
        bail!("{url} names no host");
    }
    if !policy.allow.iter().any(|allowed| allowed == host) {
        bail!("{host} is not a host this service may dial");
    }
    Ok(())
}

/// Register a connection and start the task that runs it.
fn open(
    service: &Arc<Service>,
    policy: &Policy,
    runtime: &Handle,
    owner: &str,
    url: &str,
    harness: &str,
    tool: &str,
) -> Result<u64> {
    let (outbound, inbound) = mpsc::channel(policy.queue);
    let id = {
        let Ok(mut sockets) = service.sockets.open.lock() else {
            bail!("the connection table is poisoned");
        };
        if sockets.len() >= policy.max_connections {
            bail!(
                "this service already holds its limit of {} connections",
                policy.max_connections
            );
        }
        let id = service.sockets.next.fetch_add(1, Ordering::Relaxed);
        sockets.insert(
            id,
            Connection {
                harness: owner.into(),
                outbound,
            },
        );
        id
    };

    runtime.spawn(run(
        Arc::downgrade(service),
        id,
        url.to_owned(),
        harness.to_owned(),
        tool.to_owned(),
        inbound,
        policy.max_frame,
    ));
    Ok(id)
}

/// One connection, from the dial to the close.
async fn run(
    service: Weak<Service>,
    id: u64,
    url: String,
    harness: String,
    tool: String,
    mut outbound: mpsc::Receiver<Message>,
    max_frame: usize,
) {
    let config = WebSocketConfig::default().max_message_size(Some(max_frame));
    let socket = match connect_async_with_config(&url, Some(config), false).await {
        Ok((socket, _)) => {
            deliver(&service, id, &harness, &tool, abi::WS_EVENT_OPEN, b"").await;
            socket
        }
        Err(error) => {
            let error = error.to_string();
            deliver(
                &service,
                id,
                &harness,
                &tool,
                abi::WS_EVENT_OPEN,
                error.as_bytes(),
            )
            .await;
            forget(&service, id);
            return;
        }
    };

    let (mut sink, mut stream) = socket.split();
    let mut why = String::new();
    loop {
        tokio::select! {
            frame = stream.next() => {
                let body = match frame {
                    Some(Ok(Message::Text(text))) => text.as_bytes().to_vec(),
                    Some(Ok(Message::Binary(bytes))) => bytes.to_vec(),
                    // Ping and Pong are the library's to answer.
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => {
                        why = error.to_string();
                        break;
                    }
                    None => break,
                };
                // Awaited before the next frame is read, which is what keeps
                // one connection from racing itself on its harness's own keys.
                deliver(&service, id, &harness, &tool, abi::WS_EVENT_MESSAGE, &body).await;
            }
            message = outbound.recv() => {
                // `None` is the harness having closed it, or the service going
                // away underneath.
                let Some(message) = message else { break };
                if let Err(error) = sink.send(message).await {
                    why = error.to_string();
                    break;
                }
            }
        }
    }

    deliver(
        &service,
        id,
        &harness,
        &tool,
        abi::WS_EVENT_CLOSE,
        why.as_bytes(),
    )
    .await;
    forget(&service, id);
}

/// Turn one event into an invocation. Gone if the service has shut down.
///
/// The id rides along because a harness may hold several connections onto one
/// tool, and a frame it cannot attribute is one it cannot answer.
async fn deliver(
    service: &Weak<Service>,
    id: u64,
    harness: &str,
    tool: &str,
    event: &str,
    body: &[u8],
) {
    if let Some(service) = service.upgrade() {
        let args = wire::frame(&[event.as_bytes(), id.to_string().as_bytes(), body]);
        service.dispatch(harness, tool, args).await;
    }
}

fn forget(service: &Weak<Service>, id: u64) {
    if let Some(service) = service.upgrade() {
        service.sockets.forget(id);
    }
}
