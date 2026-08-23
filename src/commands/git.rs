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
    /// Stash uncommitted changes
    Stash {
        #[command(subcommand)]
        cmd: StashCommand,
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
        cmd: RemoteCommand,
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

#[derive(clap::Subcommand)]
pub enum StashCommand {
    /// Save uncommitted changes and revert the working tree
    Save {
        message: Option<String>,
    },
    /// List saved stashes
    List,
    /// Apply and remove a stash (defaults to the most recent)
    Pop {
        #[arg(default_value_t = 0)]
        index: usize,
    },
    /// Remove a stash without applying it (defaults to the most recent)
    Drop {
        #[arg(default_value_t = 0)]
        index: usize,
    },
}

#[derive(clap::Subcommand)]
pub enum RemoteCommand {
    /// List configured remotes
    List,
    /// Add a remote
    Add { name: String, url: String },
    /// Remove a remote
    Remove { name: String },
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
        GitCommand::Stash { cmd } => stash(cmd),
        GitCommand::Tag {
            create,
            message,
            delete,
        } => tag(create, message, delete),
        GitCommand::Remote { cmd } => remote(cmd),
        GitCommand::Reset { mode, target } => {
            git::reset(&mode, &target)?;
            println!("{} Reset ({}) to {}", "✓".green().bold(), mode, target.cyan());
            Ok(())
        }
    }
}

fn stash(cmd: StashCommand) -> Result<()> {
    match cmd {
        StashCommand::Save { message } => {
            git::stash_save(message.as_deref())?;
            println!("{} Stashed changes", "✓".green().bold());
            Ok(())
        }
        StashCommand::List => {
            let entries = git::stash_list()?;
            if entries.is_empty() {
                println!("{}", "no stashes".dimmed());
                return Ok(());
            }
            for e in entries {
                println!("{} {}", format!("stash@{{{}}}", e.index).yellow(), e.message);
            }
            Ok(())
        }
        StashCommand::Pop { index } => {
            git::stash_pop(index)?;
            println!("{} Popped stash@{{{index}}}", "✓".green().bold());
            Ok(())
        }
        StashCommand::Drop { index } => {
            git::stash_drop(index)?;
            println!("{} Dropped stash@{{{index}}}", "✓".green().bold());
            Ok(())
        }
    }
}

fn tag(create: Option<String>, message: Option<String>, delete: Option<String>) -> Result<()> {
    if let Some(name) = create {
        git::tag_create(&name, message.as_deref())?;
        println!("{} Created tag {}", "✓".green().bold(), name.cyan());
        return Ok(());
    }
    if let Some(name) = delete {
        git::tag_delete(&name)?;
        println!("{} Deleted tag {}", "✓".green().bold(), name.cyan());
        return Ok(());
    }
    for name in git::tag_list()? {
        println!("{name}");
    }
    Ok(())
}

fn remote(cmd: RemoteCommand) -> Result<()> {
    match cmd {
        RemoteCommand::List => {
            for (name, url) in git::remote_list()? {
                println!("{:<15} {}", name.cyan(), url);
            }
            Ok(())
        }
        RemoteCommand::Add { name, url } => {
            git::remote_add(&name, &url)?;
            println!("{} Added remote {}", "✓".green().bold(), name.cyan());
            Ok(())
        }
        RemoteCommand::Remove { name } => {
            git::remote_remove(&name)?;
            println!("{} Removed remote {}", "✓".green().bold(), name.cyan());
            Ok(())
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
