use anyhow::Result;
use berm_api::host::{Caller, User};
use berm_cli::{
    Client, cmd,
    context::{Contexts, Owner},
};
use clap::{Args as Arguments, Parser, Subcommand};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "berm",
    version,
    about = "Deploy and inspect programs on bermd or Cloud"
)]
struct Args {
    /// The context to talk to, in place of the one in use.
    #[arg(long, global = true)]
    context: Option<String>,

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
    New { name: String },
    /// List deployed programs.
    Ls,
    /// Deploy an image, from a file or a registry, replacing whatever holds
    /// the name.
    Deploy {
        name: String,
        image: String,
        /// Where Cloud pulls a file from: it is pushed and published there
        /// first.
        #[arg(long)]
        reference: Option<String>,
        /// A tool this deploy may make public, without asking.
        #[arg(long)]
        public: Vec<String>,
        /// Deploy even when it breaks what callers used.
        #[arg(long)]
        force: bool,
    },
    /// Run one of a program's tools.
    Run {
        program: String,
        tool: String,
        /// The argument object. Read from stdin when omitted.
        arguments: Option<String>,
        /// On bermd, the caller the program is told called it.
        #[arg(long)]
        caller: Option<String>,
        /// On bermd, the end user's issuer the program is told it is called for.
        #[arg(long, requires = "user", requires = "caller")]
        issuer: Option<String>,
        /// On bermd, the end user's subject the program is told it is called
        /// for.
        #[arg(long, requires = "issuer")]
        user: Option<String>,
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
    /// Write a deployed program's IDL, which `declare_program!` reads.
    Idl {
        name: String,
        /// Where to write `{name}.json`.
        #[arg(long, default_value = "idls")]
        out: PathBuf,
    },
    /// Remove a program.
    Rm { name: String },
    /// Print what a program emitted, oldest first.
    Logs {
        name: String,
        /// Only logs after this sequence number.
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
    /// Name the hosts `berm` talks to, and pick one.
    #[command(subcommand)]
    Context(ContextCommand),
}

#[derive(Subcommand)]
enum ContextCommand {
    /// Name a host: a bermd, or Cloud with a token and the account to deploy
    /// under.
    Add {
        name: String,
        host: String,
        /// A Cloud key with `deploy` and `publish` scope.
        #[arg(long)]
        token: Option<String>,
        #[command(flatten)]
        owner: OwnerArgs,
    },
    /// Talk to `name` from now on.
    Use { name: String },
    /// List the contexts, marking the one in use.
    Ls,
    /// Forget a context.
    Rm { name: String },
}

/// The account a Cloud context deploys under.
#[derive(Arguments)]
#[group(multiple = false)]
struct OwnerArgs {
    /// An org, by handle or id.
    #[arg(long)]
    org: Option<String>,
    /// A person, by handle or id.
    #[arg(long)]
    user: Option<String>,
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let client =
        || Ok::<_, anyhow::Error>(Client::new(Contexts::load()?.get(args.context.as_deref())?));

    match &args.command {
        Command::Context(ContextCommand::Add {
            name,
            host,
            token,
            owner,
        }) => {
            let owner = match (&owner.org, &owner.user) {
                (Some(org), _) => Some(Owner::Org(org.clone())),
                (_, Some(user)) => Some(Owner::User(user.clone())),
                _ => None,
            };
            cmd::context::add(name, host, token.as_deref(), owner)?
        }
        Command::Context(ContextCommand::Use { name }) => cmd::context::use_(name)?,
        Command::Context(ContextCommand::Ls) => cmd::context::ls()?,
        Command::Context(ContextCommand::Rm { name }) => cmd::context::rm(name)?,
        Command::New { name } => cmd::new::run(name)?,
        Command::Ls => cmd::ls::run(&client()?)?,
        Command::Deploy {
            name,
            image,
            reference,
            public,
            force,
        } => cmd::deploy::run(
            &client()?,
            name,
            image,
            reference.as_deref(),
            public,
            *force,
        )?,
        Command::Push { reference, image } => cmd::push::run(reference, image)?,
        Command::Publish { reference } => cmd::publish::run(args.index.as_deref(), reference)?,
        Command::Search { term } => cmd::search::run(args.index.as_deref(), term)?,
        Command::Inspect { name } => cmd::inspect::run(&client()?, name)?,
        Command::Idl { name, out } => cmd::idl::run(&client()?, name, out)?,
        Command::Rm { name } => cmd::rm::run(&client()?, name)?,
        Command::Logs { name, after } => cmd::logs::run(&client()?, name, *after)?,
        // The one command whose exit code carries a result rather than a
        // verdict on berm.
        Command::Run {
            program,
            tool,
            arguments,
            caller,
            issuer,
            user,
        } => {
            let caller = caller.as_ref().map(|id| Caller {
                id: id.clone(),
                app: None,
                user: issuer
                    .clone()
                    .zip(user.clone())
                    .map(|(issuer, subject)| User { issuer, subject }),
                client: None,
            });
            return cmd::run::run(
                &client()?,
                program,
                tool,
                arguments.as_deref(),
                caller.as_ref(),
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}
