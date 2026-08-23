mod api;
mod commands;
mod config;
mod gitutil;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{auth, issue, pr, repo};

#[derive(Parser)]
#[command(name = "ghx", version, about = "A GitHub CLI, in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authenticate with GitHub
    Auth {
        #[command(subcommand)]
        cmd: auth::AuthCommand,
    },
    /// Work with repositories
    Repo {
        #[command(subcommand)]
        cmd: repo::RepoCommand,
    },
    /// Work with pull requests
    Pr {
        #[command(subcommand)]
        cmd: pr::PrCommand,
    },
    /// Work with issues
    Issue {
        #[command(subcommand)]
        cmd: issue::IssueCommand,
    },
    /// Run a raw git command (passthrough convenience)
    Git {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", "error:".to_string());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Auth { cmd } => auth::run(cmd),
        Command::Git { args } => {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            gitutil::run_inherit(&arg_refs)
        }
        other => {
            // Every other command group needs a GitHub API client.
            let token = config::Config::resolve_token()?;
            let client = api::Client::new(token)?;
            match other {
                Command::Repo { cmd } => repo::run(&client, cmd),
                Command::Pr { cmd } => pr::run(&client, cmd),
                Command::Issue { cmd } => issue::run(&client, cmd),
                Command::Auth { .. } | Command::Git { .. } => unreachable!(),
            }
        }
    }
}
