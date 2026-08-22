use anyhow::Result;
use berm_registry::{Index, Store};
use clap::Parser;
use std::net::SocketAddr;

#[derive(Parser)]
#[command(version, about = "An index of published harnesses")]
struct Args {
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:7788")]
    addr: SocketAddr,

    /// The index: a GitHub repository as `owner/name`, or a directory, which
    /// is what a local index and a first look use.
    #[arg(long)]
    index: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(
                "berm_registry=info"
                    .parse()
                    .expect("the default directive parses"),
            ),
        )
        .init();

    let args = Args::parse();
    let store = Store::open(&args.index, std::env::var("GITHUB_TOKEN").ok())?;
    Index::new(store).await?.serve(args.addr).await
}
