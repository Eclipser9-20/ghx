use crate::git;
use anyhow::Result;
use colored::Colorize;

#[derive(clap::Subcommand)]
pub enum GitCommand {
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
    Add {
        #[arg(default_value = None)]
        paths: Vec<String>,
    },
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
    Clone {
        url: String,
        dir: Option<String>,
    },
}

pub fn run(cmd: GitCommand) -> Result<()> {
    match cmd {
        GitCommand::Status => status(),
        GitCommand::Log { limit } => log(limit),
        GitCommand::Diff { staged } => diff(staged),
        GitCommand::Branch { create, delete } => branch(create, delete),
        GitCommand::Checkout { refname } => {
            git::checkout(&refname)?;
            println!("{} Switched to {}", "✓".green().bold(), refname.cyan());
            Ok(())
        }
        GitCommand::Add { paths } => add(paths),
        GitCommand::Commit { message } => commit(&message),
        GitCommand::Fetch { remote } => {
            git::fetch(&remote)?;
            println!("{} Fetched {}", "✓".green().bold(), remote);
            Ok(())
        }
        GitCommand::Pull { remote } => {
            git::pull(&remote)?;
            println!("{} Pulled {}", "✓".green().bold(), remote);
            Ok(())
        }
        GitCommand::Push { remote, branch } => {
            git::push(&remote, branch.as_deref())?;
            println!("{} Pushed to {}", "✓".green().bold(), remote);
            Ok(())
        }
        GitCommand::Clone { url, dir } => {
            git::clone(&url, dir.as_ref().map(std::path::Path::new))
        }
    }
}

fn status() -> Result<()> {
    let entries = git::status()?;
    if entries.is_empty() {
        println!("{}", "working tree clean".green());
        return Ok(());
    }
    for (code, path) in entries {
        let tag = match code.as_str() {
            "new" => code.green(),
            "modified" => code.yellow(),
            "deleted" => code.red(),
            "conflict" => code.red().bold(),
            _ => code.normal(),
        };
        println!("{tag:<10} {path}");
    }
    Ok(())
}

fn log(limit: usize) -> Result<()> {
    for entry in git::log(limit)? {
        println!(
            "{} {}  {}  {}",
            entry.id.yellow(),
            entry.summary,
            entry.author.cyan(),
            entry.time.dimmed()
        );
    }
    Ok(())
}

fn diff(staged: bool) -> Result<()> {
    let text = git::diff(staged)?;
    if text.is_empty() {
        println!("{}", "no changes".dimmed());
        return Ok(());
    }
    for line in text.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            println!("{}", line.green());
        } else if line.starts_with('-') && !line.starts_with("---") {
            println!("{}", line.red());
        } else {
            println!("{line}");
        }
    }
    Ok(())
}

fn branch(create: Option<String>, delete: Option<String>) -> Result<()> {
    if let Some(name) = create {
        git::branch_create(&name)?;
        println!("{} Created branch {}", "✓".green().bold(), name.cyan());
        return Ok(());
    }
    if let Some(name) = delete {
        git::branch_delete(&name)?;
        println!("{} Deleted branch {}", "✓".green().bold(), name.cyan());
        return Ok(());
    }
    for (name, is_current) in git::branch_list()? {
        if is_current {
            println!("* {}", name.green().bold());
        } else {
            println!("  {name}");
        }
    }
    Ok(())
}

fn add(paths: Vec<String>) -> Result<()> {
    if paths.is_empty() {
        git::add_all()?;
    } else {
        git::add(&paths)?;
    }
    println!("{} Staged changes", "✓".green().bold());
    Ok(())
}

fn commit(message: &str) -> Result<()> {
    let id = git::commit(message)?;
    println!("{} Committed {}", "✓".green().bold(), id.yellow());
    Ok(())
}
