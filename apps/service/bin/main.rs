use anyhow::{Context, Result};
use berm::system::call;
use bermd::{Policy, Service};
use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(version, about = "Deploys harnesses and serves their tools over MCP")]
struct Args {
    /// Address to listen on. Loopback by default: the MCP endpoint carries no
    /// authorization yet, so anything wider would be an open one.
    #[arg(long, default_value = "127.0.0.1:7777")]
    addr: SocketAddr,

    /// Where deployed images and the code cache live.
    #[arg(long)]
    root: Option<PathBuf>,

    /// How deep a chain of harnesses calling harnesses may go. `0` refuses the
    /// first one, which is composition off.
    ///
    /// The bound is on runaway composition, not on the stack: a level costs
    /// ~720 bytes of it, and 64 MiB of guest address space.
    #[arg(long, default_value_t = call::DEFAULT_CALL_DEPTH)]
    max_call_depth: u32,

    /// A host a harness may open a connection to. Repeat for each one.
    ///
    /// Given none, no harness may dial at all.
    #[arg(long = "ws-allow", value_name = "HOST")]
    ws_allow: Vec<String>,

    /// How many connections this service may hold at once.
    ///
    /// Under a 1024-descriptor soft limit with room to spare, since each
    /// connection costs one and the images, state and listener want theirs.
    #[arg(long, value_name = "N|none", default_value = "256")]
    ws_max_connections: Cap,

    /// How many frames may wait to go out on one connection before a send is
    /// refused.
    ///
    /// Backpressure rather than buffering: a deep queue delays the news that a
    /// far end has stopped reading, and this is what reports it.
    #[arg(long, value_name = "N|none", default_value = "64")]
    ws_queue: Cap,

    /// Largest message a connection may carry, 64 MiB being tungstenite's own
    /// bound.
    #[arg(long, value_name = "BYTES|none", default_value = "67108864")]
    ws_max_frame: Cap,
}

/// A bound, or `none` for no bound at all.
#[derive(Clone, Copy)]
struct Cap(Option<usize>);

impl std::str::FromStr for Cap {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.eq_ignore_ascii_case("none") {
            return Ok(Self(None));
        }
        match value.parse() {
            // Zero would be a cap that refuses everything, which `none` is
            // already the word for the opposite of. Neither is a useful bound.
            Ok(0) => Err(String::from(
                "a bound of 0 admits nothing; say `none` for no bound",
            )),
            Ok(bound) => Ok(Self(Some(bound))),
            Err(_) => Err(format!("expected a number or `none`, not {value:?}")),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("bermd=info".parse().expect("the default directive parses"))
                // The runtime restores its own images now, so its target has
                // to be on by default or coming back says nothing.
                .add_directive("berm=info".parse().expect("the default directive parses")),
        )
        .init();

    let args = Args::parse();
    let root = match args.root {
        Some(root) => root,
        None => PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".berm"),
    };

    // The bounds always have values; an empty allowlist is what decides
    // whether any of them is ever reached.
    let policy = Policy {
        allow: args.ws_allow,
        max_frame: args.ws_max_frame.0,
        max_connections: args.ws_max_connections.0,
        queue: args.ws_queue.0,
    };

    let service = Service::new(root, args.max_call_depth, policy).await?;
    let listener = TcpListener::bind(args.addr)
        .await
        .with_context(|| format!("failed to bind {}", args.addr))?;

    let addr = args.addr;
    tracing::info!("listening on http://{addr}, mcp at http://{addr}/mcp");
    service.serve(listener).await
}
