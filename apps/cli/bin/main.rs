use anyhow::Result;
use berm_cli::{Client, cmd};
use clap::{Parser, Subcommand};
use std::{path::PathBuf, process::ExitCode};

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

    /// Which harness index to read and publish to. No default: a built-in one
    /// would make berm ship an opinion about whose list you read.
    #[arg(long, global = true)]
    index: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a harness crate.
    New { name: String },
    /// List deployed harnesses.
    Ls,
    /// Deploy an image, from a file or a registry, replacing whatever holds
    /// the name.
    Deploy { name: String, image: String },
    /// Run one of a harness's tools.
    Run {
        harness: String,
        tool: String,
        /// The argument object. Read from stdin when omitted.
        arguments: Option<String>,
    },
    /// Publish an image to a registry.
    Push { reference: String, image: PathBuf },
    /// List an already-pushed image in an index.
    Publish { reference: String },
    /// Find a published harness.
    Search {
        #[arg(default_value = "")]
        term: String,
    },
    /// Show a harness's tools and their arguments.
    Inspect { name: String },
    /// Remove a harness.
    Rm { name: String },
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let client = Client::new(args.host);

    match &args.command {
        Command::New { name } => cmd::new::run(name)?,
        Command::Ls => cmd::ls::run(&client)?,
        Command::Deploy { name, image } => cmd::deploy::run(&client, name, image)?,
        Command::Push { reference, image } => cmd::push::run(reference, image)?,
        Command::Publish { reference } => cmd::publish::run(args.index.as_ref(), reference)?,
        Command::Search { term } => cmd::search::run(args.index.as_ref(), term)?,
        Command::Inspect { name } => cmd::inspect::run(&client, name)?,
        Command::Rm { name } => cmd::rm::run(&client, name)?,
        // The one command whose exit code carries a result rather than a
        // verdict on berm.
        Command::Run {
            harness,
            tool,
            arguments,
        } => return cmd::run::run(&client, harness, tool, arguments.as_deref()),
    }
    Ok(ExitCode::SUCCESS)
}
