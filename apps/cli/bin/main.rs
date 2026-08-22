use anyhow::Result;
use berm_cli::{Client, cmd};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "berm",
    version,
    about = "Deploy and inspect harnesses on bermd"
)]
struct Args {
    /// Where bermd is listening.
    #[arg(long, global = true, default_value = "http://127.0.0.1:7777")]
    host: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a harness crate.
    New { name: String },
    /// List deployed harnesses.
    Ls,
    /// Deploy an ELF, replacing whatever holds the name.
    Deploy { name: String, image: PathBuf },
    /// Show a harness's tools and their arguments.
    Inspect { name: String },
    /// Remove a harness.
    Rm { name: String },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::new(args.host);

    match &args.command {
        Command::New { name } => cmd::new::run(name),
        Command::Ls => cmd::ls::run(&client),
        Command::Deploy { name, image } => cmd::deploy::run(&client, name, image),
        Command::Inspect { name } => cmd::inspect::run(&client, name),
        Command::Rm { name } => cmd::rm::run(&client, name),
    }
}
