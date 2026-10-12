use anyhow::{Context, Result};
use berm::syscall::call;
use bermd::Service;
use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(version, about = "Deploys programs and serves their tools over MCP")]
struct Args {
    /// Address to listen on. Loopback by default: the MCP endpoint carries no
    /// authorization yet, so anything wider would be an open one.
    #[arg(long, default_value = "127.0.0.1:7777")]
    addr: SocketAddr,

    /// Where deployed images and the code cache live.
    #[arg(long)]
    root: Option<PathBuf>,

    /// How deep a chain of programs calling programs may go. `0` refuses the
    /// first one, which is composition off.
    ///
    /// The bound is on runaway composition, not on the stack: a level costs
    /// ~720 bytes of it, and 64 MiB of guest address space.
    #[arg(long, default_value_t = call::DEFAULT_CALL_DEPTH)]
    max_call_depth: u32,

    /// The proxy `berm.http.proxy` goes through, e.g.
    /// `http://user:pass@gate.example.com:3120`. Without one it goes direct.
    #[arg(long)]
    proxy: Option<String>,
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

    let service = Service::new(root, args.max_call_depth, args.proxy.as_deref()).await?;
    let listener = TcpListener::bind(args.addr)
        .await
        .with_context(|| format!("failed to bind {}", args.addr))?;

    let addr = args.addr;
    tracing::info!("listening on http://{addr}, mcp at http://{addr}/mcp");
    // A daemon in a terminal is what Ctrl+C is for, so this is the process that
    // gets to take SIGINT.
    service
        .serve(listener, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
