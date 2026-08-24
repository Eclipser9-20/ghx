use crate::git;
use crate::highlight::Highlighter;
use anyhow::{Context, Result};
use colored::Colorize;

#[derive(clap::Subcommand)]
pub enum StashCommand {
    /// Save uncommitted changes and revert the working tree
    Save { message: Option<String> },
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

pub fn stash(cmd: StashCommand) -> Result<()> {
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

pub fn tag(
    create: Option<String>,
    message: Option<String>,
    delete: Option<String>,
    push: Option<String>,
) -> Result<()> {
    if let Some(name) = create {
        git::tag_create(&name, message.as_deref())?;
        println!("{} Created tag {}", "✓".green().bold(), name.cyan());
        if let Some(remote) = push {
            git::push_tag(&remote, &name)?;
            println!("{} Pushed tag {} to {}", "✓".green().bold(), name.cyan(), remote);
        }
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

pub fn remote(cmd: RemoteCommand) -> Result<()> {
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

pub fn status() -> Result<()> {
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

pub fn log(limit: usize) -> Result<()> {
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

pub fn diff(staged: bool) -> Result<()> {
    let text = git::diff(staged)?;
    Highlighter::new().print_diff(&text);
    Ok(())
}

pub fn branch(create: Option<String>, delete: Option<String>) -> Result<()> {
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

pub fn checkout(refname: &str) -> Result<()> {
    git::checkout(refname)?;
    println!("{} Switched to {}", "✓".green().bold(), refname.cyan());
    Ok(())
}

pub fn add(paths: Vec<String>) -> Result<()> {
    if paths.is_empty() {
        git::add_all()?;
    } else {
        git::add(&paths)?;
    }
    println!("{} Staged changes", "✓".green().bold());
    Ok(())
}

pub fn commit(message: Option<String>, generate: bool) -> Result<()> {
    let message = if generate {
        let diff = crate::git::diff(true)?;
        crate::ai::generate_commit_message(&diff)?
    } else {
        message.context("a commit message is required (-m) or use --generate")?
    };
    let id = git::commit(&message)?;
    let summary = message.lines().next().unwrap_or("");
    println!("{} Committed {} - {}", "✓".green().bold(), id.yellow(), summary);
    Ok(())
}

pub fn fetch(remote: &str) -> Result<()> {
    git::fetch(remote)?;
    println!("{} Fetched {}", "✓".green().bold(), remote);
    Ok(())
}

pub fn pull(remote: &str) -> Result<()> {
    git::pull(remote)?;
    println!("{} Pulled {}", "✓".green().bold(), remote);
    Ok(())
}

pub fn push(
    remote: &str,
    branch: Option<&str>,
    force: bool,
    force_with_lease: bool,
) -> Result<()> {
    git::push_opts(remote, branch, force, force_with_lease)?;
    if force || force_with_lease {
        println!("{} Force-pushed to {}", "✓".green().bold(), remote);
    } else {
        println!("{} Pushed to {}", "✓".green().bold(), remote);
    }
    Ok(())
}

pub fn clone(url: &str, dir: Option<&str>) -> Result<()> {
    git::clone(url, dir.map(std::path::Path::new))
}

pub fn reset(mode: &str, target: &str) -> Result<()> {
    git::reset(mode, target)?;
    println!("{} Reset ({}) to {}", "✓".green().bold(), mode, target.cyan());
    Ok(())
}

pub fn merge(branch: &str) -> Result<()> {
    let result = git::merge(branch)?;
    println!("{} {}", "✓".green().bold(), result);
    Ok(())
}

pub fn rebase(onto: &str) -> Result<()> {
    let result = git::rebase(onto)?;
    println!("{} {}", "✓".green().bold(), result);
    Ok(())
}

pub fn cherry_pick(commit_ref: &str) -> Result<()> {
    let id = git::cherry_pick(commit_ref)?;
    println!("{} Cherry-picked as {}", "✓".green().bold(), id.yellow());
    Ok(())
}

pub fn blame(path: &str) -> Result<()> {
    let hl = Highlighter::new();
    for line in git::blame(path)? {
        print!(
            "{} {:<16} {:>4}  ",
            line.commit.yellow(),
            line.author.cyan(),
            line.line_no
        );
        hl.print_line(path, &line.content);
    }
    Ok(())
}

#[derive(clap::Subcommand)]
pub enum WorktreeCommand {
    /// List worktrees
    List,
    /// Add a new worktree
    Add {
        /// Name of the worktree
        name: String,
        /// Path to create the worktree at
        path: String,
        /// Create a new branch for the worktree (defaults to `name`)
        #[arg(long)]
        branch: Option<String>,
    },
    /// Remove a worktree
    Remove { name: String },
}

pub fn worktree(cmd: WorktreeCommand) -> Result<()> {
    match cmd {
        WorktreeCommand::List => {
            for wt in git::worktree_list()? {
                println!("{:<20} {}", wt.name.cyan(), wt.path);
            }
            Ok(())
        }
        WorktreeCommand::Add { name, path, branch } => {
            let branch = branch.unwrap_or_else(|| name.clone());
            git::worktree_add(&name, &path, Some(&branch))?;
            println!(
                "{} Added worktree {} at {} (branch {})",
                "✓".green().bold(),
                name.cyan(),
                path,
                branch.cyan()
            );
            Ok(())
        }
        WorktreeCommand::Remove { name } => {
            git::worktree_remove(&name)?;
            println!("{} Removed worktree {}", "✓".green().bold(), name.cyan());
            Ok(())
        }
    }
}
