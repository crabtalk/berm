use anyhow::Result;
use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[command(version, about = "A local stand-in for an index service")]
struct Args {
    /// The directory to keep the list in. Created if it is not there.
    #[arg(long, default_value = "./index")]
    index: PathBuf,

    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:7788")]
    addr: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(
                "berm_indexd=info"
                    .parse()
                    .expect("the default directive parses"),
            ),
        )
        .init();

    let args = Args::parse();
    berm_indexd::serve(args.index, args.addr).await
}
