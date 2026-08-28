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
    /// Given none, no harness may dial at all. Opening the door means saying
    /// how wide, so this requires the two bounds below.
    #[arg(
        long = "ws-allow",
        value_name = "HOST",
        requires_all = ["ws_max_connections", "ws_queue"]
    )]
    ws_allow: Vec<String>,

    /// How many connections this service may hold at once.
    #[arg(long)]
    ws_max_connections: Option<usize>,

    /// How many frames may wait to go out on one connection before a send is
    /// refused.
    #[arg(long)]
    ws_queue: Option<usize>,

    /// Largest message a connection may carry, tungstenite's own bound.
    #[arg(long, default_value_t = 64 << 20)]
    ws_max_frame: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("bermd=info".parse().expect("the default directive parses")),
        )
        .init();

    let args = Args::parse();
    let root = match args.root {
        Some(root) => root,
        None => PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".berm"),
    };

    // Both are present or both absent: clap ties them to `--ws-allow`, which
    // is what decides whether any of it is reachable.
    let policy = match (args.ws_max_connections, args.ws_queue) {
        (Some(max_connections), Some(queue)) => Policy {
            allow: args.ws_allow,
            max_frame: args.ws_max_frame,
            max_connections,
            queue,
        },
        _ => Policy::default(),
    };

    let service = Service::new(root, args.max_call_depth, policy).await?;
    let listener = TcpListener::bind(args.addr)
        .await
        .with_context(|| format!("failed to bind {}", args.addr))?;

    let addr = args.addr;
    tracing::info!("listening on http://{addr}, mcp at http://{addr}/mcp");
    service.serve(listener).await
}
