use anyhow::Result;
use berm_cli::{Client, cmd};
use clap::{Parser, Subcommand};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(name = "berm", version, about = "Deploy and inspect programs on bermd")]
struct Args {
    /// Where bermd is listening.
    #[arg(long, global = true, default_value = "http://127.0.0.1:7777")]
    host: String,

    /// Which program index to read and publish to: a directory, a `.git` URL
    /// to keep a copy of, or a service. `BERM_INDEX`, then the default list.
    #[arg(long, global = true)]
    index: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a program crate.
    New {
        name: String,
        /// What to build it for. RISC-V is experimental.
        #[arg(long, value_enum, default_value_t)]
        target: cmd::new::Target,
    },
    /// List deployed programs.
    Ls,
    /// Deploy an image, from a file or a registry, replacing whatever holds
    /// the name.
    Deploy { name: String, image: String },
    /// Run one of a program's tools.
    Run {
        program: String,
        tool: String,
        /// The argument object. Read from stdin when omitted.
        arguments: Option<String>,
    },
    /// Publish an image to a registry.
    Push { reference: String, image: PathBuf },
    /// List an already-pushed image in an index.
    Publish { reference: String },
    /// Find a published program.
    Search {
        #[arg(default_value = "")]
        term: String,
    },
    /// Show a program's tools and their arguments.
    Inspect { name: String },
    /// Remove a program.
    Rm { name: String },
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let client = Client::new(args.host);

    match &args.command {
        Command::New { name, target } => cmd::new::run(name, *target)?,
        Command::Ls => cmd::ls::run(&client)?,
        Command::Deploy { name, image } => cmd::deploy::run(&client, name, image)?,
        Command::Push { reference, image } => cmd::push::run(reference, image)?,
        Command::Publish { reference } => cmd::publish::run(args.index.as_deref(), reference)?,
        Command::Search { term } => cmd::search::run(args.index.as_deref(), term)?,
        Command::Inspect { name } => cmd::inspect::run(&client, name)?,
        Command::Rm { name } => cmd::rm::run(&client, name)?,
        // The one command whose exit code carries a result rather than a
        // verdict on berm.
        Command::Run {
            program,
            tool,
            arguments,
        } => return cmd::run::run(&client, program, tool, arguments.as_deref()),
    }
    Ok(ExitCode::SUCCESS)
}
