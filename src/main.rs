mod api;
mod commands;
mod config;
mod git;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{auth, git as git_cmd, issue, pr, repo};

#[derive(Parser)]
#[command(name = "ghx", version, about = "A GitHub CLI, in Rust", disable_help_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short = 'h', long = "help", action = clap::ArgAction::SetTrue)]
    help: bool,
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
    /// Native git operations (no shelling out — backed by libgit2 directly)
    Git {
        #[command(subcommand)]
        cmd: git_cmd::GitCommand,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // A bare `ghx`, `ghx --help`/`-h`, or `ghx help` prints the tree
    // overview instead of clap's default flat help — everything else goes
    // through clap normally so per-subcommand `--help` stays detailed.
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let wants_tree = raw.is_empty()
        || raw.iter().any(|a| a == "--help" || a == "-h" || a == "help") && raw.len() == 1;
    if wants_tree {
        print_tree();
        return Ok(());
    }

    let cli = Cli::parse();
    let Some(command) = cli.command else {
        print_tree();
        return Ok(());
    };

    match command {
        Command::Auth { cmd } => auth::run(cmd),
        Command::Git { cmd } => git_cmd::run(cmd),
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

fn print_tree() {
    use clap::CommandFactory;
    use colored::Colorize;

    println!("{} {}", "ghx".bold().green(), format!("v{}", env!("CARGO_PKG_VERSION")).dimmed());
    println!("{}", "A GitHub CLI, in Rust — no shelling out, ever.".dimmed());
    println!();

    let cmd = Cli::command();
    let subcommands: Vec<_> = cmd.get_subcommands().collect();
    let count = subcommands.len();

    for (i, sub) in subcommands.iter().enumerate() {
        let is_last_top = i + 1 == count;
        let branch = if is_last_top { "└─" } else { "├─" };
        println!(
            "{} {}  {}",
            branch.dimmed(),
            sub.get_name().bold().cyan(),
            sub.get_about().map(|s| s.to_string()).unwrap_or_default().dimmed()
        );

        let prefix = if is_last_top { "   " } else { "│  " };
        let children: Vec<_> = sub.get_subcommands().collect();
        let child_count = children.len();

        for (j, child) in children.iter().enumerate() {
            let is_last_child = j + 1 == child_count;
            let child_branch = if is_last_child { "└─" } else { "├─" };

            let args = format_args(child);
            println!(
                "{prefix}{} {} {}  {}",
                child_branch.dimmed(),
                child.get_name().yellow(),
                args.dimmed(),
                child.get_about().map(|s| s.to_string()).unwrap_or_default().dimmed()
            );
        }
    }

    println!();
    println!("{}", "Run `ghx <command> <subcommand> --help` for full details on any subcommand.".dimmed());
}

fn format_args(cmd: &clap::Command) -> String {
    let mut parts = Vec::new();

    for arg in cmd.get_positionals() {
        let name = arg.get_id().as_str();
        if arg.is_required_set() {
            parts.push(format!("<{name}>"));
        } else {
            parts.push(format!("[{name}]"));
        }
    }

    for arg in cmd.get_arguments() {
        if arg.is_positional() || arg.get_id() == "help" {
            continue;
        }
        let flag = match arg.get_long() {
            Some(long) => format!("--{long}"),
            None => match arg.get_short() {
                Some(short) => format!("-{short}"),
                None => continue,
            },
        };
        parts.push(format!("[{flag}]"));
    }

    parts.join(" ")
}
