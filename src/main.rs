mod api;
mod commands;
mod config;
mod git;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{auth, git as gitcmd, issue, pr, repo, run as runcmd};

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
    // ---- GitHub -----------------------------------------------------
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
    /// Work with GitHub Actions workflow runs
    Run {
        #[command(subcommand)]
        cmd: runcmd::RunCommand,
    },
    /// Print a file's contents from a repo (owner/repo/path/to/file)
    Raw {
        spec: String,
        #[arg(long = "ref")]
        git_ref: Option<String>,
    },

    // ---- native git operations (backed by libgit2) ----
    /// Show working tree status
    Status,
    /// Show commit history
    Log {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Show changes (working tree by default, or staged with --staged)
    Diff {
        #[arg(long)]
        staged: bool,
    },
    /// List, create, or delete branches
    Branch {
        /// Create a branch with this name
        #[arg(long)]
        create: Option<String>,
        /// Delete a branch with this name
        #[arg(long)]
        delete: Option<String>,
    },
    /// Switch to a branch or revision
    Checkout { refname: String },
    /// Stage files (defaults to all changes)
    Add { paths: Vec<String> },
    /// Record staged changes
    Commit {
        #[arg(short = 'm', long)]
        message: String,
    },
    /// Download objects/refs from a remote
    Fetch {
        #[arg(default_value = "origin")]
        remote: String,
    },
    /// Fetch and fast-forward the current branch
    Pull {
        #[arg(default_value = "origin")]
        remote: String,
    },
    /// Upload the current (or given) branch to a remote
    Push {
        #[arg(default_value = "origin")]
        remote: String,
        branch: Option<String>,
    },
    /// Clone a repository by URL
    Clone { url: String, dir: Option<String> },
    /// Stash uncommitted changes
    Stash {
        #[command(subcommand)]
        cmd: gitcmd::StashCommand,
    },
    /// List, create, or delete tags
    Tag {
        /// Create a tag with this name
        #[arg(long)]
        create: Option<String>,
        /// Annotate the created tag with this message (implies an annotated, not lightweight, tag)
        #[arg(short = 'm', long)]
        message: Option<String>,
        /// Delete a tag with this name
        #[arg(long)]
        delete: Option<String>,
    },
    /// List, add, or remove remotes
    Remote {
        #[command(subcommand)]
        cmd: gitcmd::RemoteCommand,
    },
    /// Reset the current branch to a revision
    Reset {
        /// soft (keep changes staged), mixed (keep changes unstaged), or hard (discard changes)
        #[arg(long, default_value = "mixed", value_parser = ["soft", "mixed", "hard"])]
        mode: String,
        #[arg(default_value = "HEAD")]
        target: String,
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
        Command::Auth { cmd } => return auth::run(cmd),
        Command::Status => return gitcmd::status(),
        Command::Log { limit } => return gitcmd::log(limit),
        Command::Diff { staged } => return gitcmd::diff(staged),
        Command::Branch { create, delete } => return gitcmd::branch(create, delete),
        Command::Checkout { refname } => return gitcmd::checkout(&refname),
        Command::Add { paths } => return gitcmd::add(paths),
        Command::Commit { message } => return gitcmd::commit(&message),
        Command::Fetch { remote } => return gitcmd::fetch(&remote),
        Command::Pull { remote } => return gitcmd::pull(&remote),
        Command::Push { remote, branch } => return gitcmd::push(&remote, branch.as_deref()),
        Command::Clone { url, dir } => return gitcmd::clone(&url, dir.as_deref()),
        Command::Stash { cmd } => return gitcmd::stash(cmd),
        Command::Tag {
            create,
            message,
            delete,
        } => return gitcmd::tag(create, message, delete),
        Command::Remote { cmd } => return gitcmd::remote(cmd),
        Command::Reset { mode, target } => return gitcmd::reset(&mode, &target),
        _ => {}
    }

    // Everything else needs a GitHub API client.
    let token = config::Config::resolve_token()?;
    let client = api::Client::new(token)?;
    match command {
        Command::Repo { cmd } => repo::run(&client, cmd),
        Command::Pr { cmd } => pr::run(&client, cmd),
        Command::Issue { cmd } => issue::run(&client, cmd),
        Command::Run { cmd } => runcmd::run(&client, cmd),
        Command::Raw { spec, git_ref } => repo::raw(&client, &spec, git_ref),
        _ => unreachable!(),
    }
}

fn print_tree() {
    use clap::CommandFactory;
    use colored::Colorize;

    // Tokyo Night Storm palette (no purple/magenta by request) — same
    // family as the LazyVim/VS Code/JetBrains "storm" neon-dark themes.
    let comment = (86, 95, 137); // #565f89 muted/dim
    let cyan = (125, 207, 255); // #7dcfff
    let teal = (115, 218, 202); // #73daca
    let green = (158, 206, 106); // #9ece6a
    let orange = (224, 175, 104); // #e0af68

    println!(
        "{} {}",
        "ghx".truecolor(teal.0, teal.1, teal.2).bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).truecolor(comment.0, comment.1, comment.2)
    );
    println!(
        "{}",
        "A fast, native GitHub and git CLI."
            .truecolor(comment.0, comment.1, comment.2)
    );
    println!();

    let cmd = Cli::command();
    let subcommands: Vec<_> = cmd.get_subcommands().collect();
    let count = subcommands.len();

    for (i, sub) in subcommands.iter().enumerate() {
        let is_last_top = i + 1 == count;
        let branch = if is_last_top { "└─" } else { "├─" };
        let children: Vec<_> = sub.get_subcommands().collect();

        if children.is_empty() {
            // Leaf top-level command (most native git ops): show its own args.
            let args = format_args(sub);
            println!(
                "{} {} {}  {}",
                branch.truecolor(comment.0, comment.1, comment.2),
                sub.get_name().truecolor(green.0, green.1, green.2).bold(),
                args.truecolor(orange.0, orange.1, orange.2),
                sub.get_about()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    .truecolor(comment.0, comment.1, comment.2)
            );
            continue;
        }

        println!(
            "{} {}  {}",
            branch.truecolor(comment.0, comment.1, comment.2),
            sub.get_name().truecolor(cyan.0, cyan.1, cyan.2).bold(),
            sub.get_about()
                .map(|s| s.to_string())
                .unwrap_or_default()
                .truecolor(comment.0, comment.1, comment.2)
        );

        let prefix = if is_last_top { "   " } else { "│  " };
        let child_count = children.len();

        for (j, child) in children.iter().enumerate() {
            let is_last_child = j + 1 == child_count;
            let child_branch = if is_last_child { "└─" } else { "├─" };

            let args = format_args(child);
            println!(
                "{prefix}{} {} {}  {}",
                child_branch.truecolor(comment.0, comment.1, comment.2),
                child.get_name().truecolor(green.0, green.1, green.2),
                args.truecolor(orange.0, orange.1, orange.2),
                child
                    .get_about()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    .truecolor(comment.0, comment.1, comment.2)
            );
        }
    }

    println!();
    println!(
        "{}",
        "Run `ghx <command> [subcommand] --help` for full details on any command."
            .truecolor(comment.0, comment.1, comment.2)
            .italic()
    );
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
