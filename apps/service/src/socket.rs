//! Connections as a source of invocations.
//!
//! A harness opens one and names the tool its events reach. From then on the
//! connection is what starts invocations: the dial's outcome, each frame that
//! arrives, and the close. Guest memory does not survive any of them, so a
//! harness holding a conversation keeps it in `berm.get`/`berm.set` and reads
//! it back on the next frame.
//!
//! What is open is written down, so a restart brings it back under the same id
//! — a harness that stored one is still holding a name that works. A drop the
//! harness is alive for is its own to answer: it gets the close and decides
//! whether to dial again, because how long to wait is a fact about the service
//! at the far end, and this daemon knows none of them.
//!
//! berm serves none of this itself. A dialer needs an allowlist and a frame
//! cap to compile at all, and those are decisions about a host.

use crate::Service;
use anyhow::{Context, Result, bail};
use berm::{Callsite, Records, System, abi, wire};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{net::TcpStream, runtime::Handle, sync::mpsc};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
        protocol::WebSocketConfig,
    },
};

/// What a host decides before a guest can reach the network.
///
/// [`Default`] is every door shut: an empty allowlist refuses each dial before
/// any of the caps below it is read, so an embedder that says nothing grants
/// nothing. Each cap is `None` for no bound at all.
#[derive(Clone, Default)]
pub struct Policy {
    /// Hosts a harness may dial.
    pub allow: Vec<String>,
    pub max_frame: Option<usize>,
    pub max_connections: Option<usize>,
    /// How many frames may be waiting to go out before a send is refused.
    pub queue: Option<usize>,
}

/// The sending half of a connection's queue.
///
/// Two shapes because an unbounded queue is a different tokio channel, and the
/// alternative — a bounded one at some enormous capacity — would be a cap
/// pretending not to be one.
enum Outbound {
    Bounded(mpsc::Sender<Message>),
    Unbounded(mpsc::UnboundedSender<Message>),
}

/// Its receiving half.
enum Inbound {
    Bounded(mpsc::Receiver<Message>),
    Unbounded(mpsc::UnboundedReceiver<Message>),
}

impl Outbound {
    /// Never waits for room. This runs on the thread inside the guest, and the
    /// task that would drain the queue is the same one awaiting this very
    /// invocation — a send that waited would wait for itself.
    fn try_send(&self, message: Message) -> Result<()> {
        match self {
            Self::Bounded(sender) => sender.try_send(message).map_err(|error| error.into()),
            Self::Unbounded(sender) => sender.send(message).map_err(|error| error.into()),
        }
    }
}

impl Inbound {
    async fn recv(&mut self) -> Option<Message> {
        match self {
            Self::Bounded(inbound) => inbound.recv().await,
            Self::Unbounded(inbound) => inbound.recv().await,
        }
    }
}

fn queue(depth: Option<usize>) -> (Outbound, Inbound) {
    match depth {
        Some(depth) => {
            let (sender, receiver) = mpsc::channel(depth);
            (Outbound::Bounded(sender), Inbound::Bounded(receiver))
        }
        None => {
            let (sender, receiver) = mpsc::unbounded_channel();
            (Outbound::Unbounded(sender), Inbound::Unbounded(receiver))
        }
    }
}

/// What is written down about one connection: everything needed to dial it
/// again, and nothing about the socket itself, which does not survive.
#[derive(Serialize, Deserialize)]
struct Record {
    /// Who opened it, and whose alone it is to send on or close.
    owner: String,
    url: String,
    /// Where its events land.
    harness: String,
    tool: String,
    /// Sent on the handshake, and kept so a reopen after a restart presents
    /// them again. A credential put here is a credential on this host's disk,
    /// the same as one a harness writes with `berm.set`.
    #[serde(default)]
    headers: Vec<(String, String)>,
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
    /// The id of a connection another harness opened resolves to nothing, the
    /// way another harness's keys are unaddressable.
    owner: Arc<str>,
    /// Dropping this is what closes the connection: the task reading the far
    /// end sees its receiver end and shuts the socket down.
    outbound: Arc<Outbound>,
}

impl Sockets {
    fn owned_by(&self, harness: &str, id: u64) -> Result<Arc<Outbound>> {
        let Ok(open) = self.open.lock() else {
            bail!("the connection table is poisoned");
        };
        match open.get(&id) {
            Some(connection) if &*connection.owner == harness => Ok(connection.outbound.clone()),
            // One message for both, so a probe cannot tell an id that is not
            // yours from one that does not exist.
            _ => bail!("no connection {id} is open"),
        }
    }
}

/// `berm.ws.open`, `berm.ws.send` and `berm.ws.close`, against `service`.
///
/// `Weak` for the reason berm holds its own runtime that way: a connection
/// task reaches back into the service that owns the table it lives in.
pub(crate) fn system(service: Weak<Service>, runtime: Handle) -> Vec<System> {
    let (sending, closing) = (service.clone(), service.clone());
    vec![
        System {
            name: abi::WS_OPEN.to_owned(),
            call: Arc::new(move |at: &Callsite<'_>, request: &[u8]| {
                let fields = wire::fields(request)?;
                let url = wire::text(&fields, 0, "url")?;
                let harness = wire::text(&fields, 1, "harness")?;
                let tool = wire::text(&fields, 2, "tool")?;

                // Whatever follows the three is header names and values in
                // turn, which is how the codegen already flattens a trailing
                // list of pairs.
                let pairs = fields.get(3..).unwrap_or_default();
                if pairs.len() % 2 != 0 {
                    bail!("a header needs a name and a value");
                }
                let mut headers = Vec::with_capacity(pairs.len() / 2);
                for (at, pair) in pairs.chunks_exact(2).enumerate() {
                    headers.push((
                        str::from_utf8(pair[0])
                            .with_context(|| format!("header {at} has no name"))?
                            .to_owned(),
                        str::from_utf8(pair[1])
                            .with_context(|| format!("header {at} has no value"))?
                            .to_owned(),
                    ));
                }

                let Some(service) = service.upgrade() else {
                    bail!("the service is shutting down, so {url} was not dialled");
                };
                let record = Record {
                    owner: at.harness.to_owned(),
                    url: url.to_owned(),
                    harness: harness.to_owned(),
                    tool: tool.to_owned(),
                    headers,
                };
                Ok(service
                    .dial(record, None, &runtime)?
                    .to_string()
                    .into_bytes())
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
                service.shut(id);
                Ok(Vec::new())
            }),
        },
    ]
}

impl Service {
    /// Register a connection, write it down, and start the task that runs it.
    ///
    /// `id` is `Some` only when bringing one back after a restart, where
    /// keeping the number matters: a harness that stored it is still holding
    /// it.
    fn dial(self: &Arc<Self>, record: Record, id: Option<u64>, runtime: &Handle) -> Result<u64> {
        self.permitted(&record.url)?;

        let (outbound, inbound) = queue(self.policy.queue);
        let id = {
            let Ok(mut open) = self.sockets.open.lock() else {
                bail!("the connection table is poisoned");
            };
            if self
                .policy
                .max_connections
                .is_some_and(|most| open.len() >= most)
            {
                bail!("this service already holds every connection it may");
            }
            let id = id.unwrap_or_else(|| self.sockets.next.fetch_add(1, Ordering::Relaxed));
            open.insert(
                id,
                Connection {
                    owner: record.owner.as_str().into(),
                    outbound: Arc::new(outbound),
                },
            );
            id
        };

        // Written before the dial, so a connection that comes up and is never
        // heard from again is still one a restart knows to reopen.
        self.berm
            .storage()
            .put(
                Records::Sockets,
                &id.to_string(),
                &serde_json::to_vec(&record)?,
            )
            .context("failed to write down a connection")?;

        runtime.spawn(run(
            Arc::downgrade(self),
            id,
            record,
            inbound,
            self.policy.max_frame,
        ));
        Ok(id)
    }

    /// Forget a connection and stop reopening it.
    fn shut(&self, id: u64) {
        if let Ok(mut open) = self.sockets.open.lock() {
            open.remove(&id);
        }
        if let Err(error) = self
            .berm
            .storage()
            .remove(Records::Sockets, &id.to_string())
        {
            tracing::warn!(id, "failed to forget a connection: {error:#}");
        }
    }

    /// Dial everything that was open when this process last stopped.
    ///
    /// One that will not come up is reported through its harness's own event,
    /// the same as any other failed dial, so a far end that is down does not
    /// keep the service from starting.
    pub(crate) async fn reopen(self: &Arc<Self>) -> Result<()> {
        let runtime = Handle::current();
        let mut highest = 0;
        for (key, bytes) in self.berm.storage().list(Records::Sockets)? {
            let Ok(id) = key.parse::<u64>() else {
                continue;
            };
            let record: Record = match serde_json::from_slice(&bytes) {
                Ok(record) => record,
                Err(error) => {
                    tracing::error!(id, "unreadable connection record: {error}");
                    continue;
                }
            };

            highest = highest.max(id + 1);
            match self.dial(record, Some(id), &runtime) {
                Ok(_) => tracing::info!(id, "reopening"),
                Err(error) => tracing::error!(id, "{error:#}"),
            }
        }

        // Past every id that came back, so a new connection cannot be handed a
        // number a harness is still using.
        self.sockets.next.fetch_max(highest, Ordering::Relaxed);
        Ok(())
    }

    fn permitted(&self, url: &str) -> Result<()> {
        let Some(host) = host(url) else {
            bail!("{url} names no host");
        };
        if !self.policy.allow.iter().any(|allowed| allowed == host) {
            bail!("{host} is not a host this service may dial");
        }
        Ok(())
    }
}

/// Dial `url`, presenting `headers` on the handshake.
async fn connect(
    url: &str,
    headers: &[(String, String)],
    max_frame: Option<usize>,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let mut request = url.into_client_request()?;
    for (name, value) in headers {
        request.headers_mut().insert(
            HeaderName::try_from(name.as_str())
                .with_context(|| format!("{name} is not a header name"))?,
            HeaderValue::try_from(value.as_str())
                .with_context(|| format!("{name} has a value no header may carry"))?,
        );
    }

    let config = WebSocketConfig::default().max_message_size(max_frame);
    let (socket, _) = connect_async_with_config(request, Some(config), false).await?;
    Ok(socket)
}

/// The host a URL names, or `None` when it carries no scheme — which is what
/// tells a dependency on a harness from one on somewhere to dial.
pub(crate) fn host(url: &str) -> Option<&str> {
    let authority = url.split("://").nth(1)?.split('/').next()?;
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let host = authority.split(':').next().unwrap_or(authority);
    (!host.is_empty()).then_some(host)
}

/// Text when the payload is UTF-8. Every API a harness is likely to hold a
/// connection to speaks JSON over text frames.
fn frame(payload: &[u8]) -> Message {
    match str::from_utf8(payload) {
        Ok(text) => Message::text(text),
        Err(_) => Message::binary(payload.to_vec()),
    }
}

/// One connection, from the dial to the close.
async fn run(
    service: Weak<Service>,
    id: u64,
    record: Record,
    mut outbound: Inbound,
    max_frame: Option<usize>,
) {
    let (harness, tool) = (record.harness, record.tool);
    let socket = match connect(&record.url, &record.headers, max_frame).await {
        Ok(socket) => {
            deliver(&service, id, &harness, &tool, abi::WS_EVENT_OPEN, b"").await;
            socket
        }
        Err(error) => {
            let error = format!("{error:#}");
            deliver(
                &service,
                id,
                &harness,
                &tool,
                abi::WS_EVENT_OPEN,
                error.as_bytes(),
            )
            .await;
            shut(&service, id);
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
    shut(&service, id);
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

fn shut(service: &Weak<Service>, id: u64) {
    if let Some(service) = service.upgrade() {
        service.shut(id);
    }
}
