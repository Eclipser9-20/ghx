//! `ghx prune` — list (and, with --yes, delete) local branches whose remote
//! counterpart is gone and whose commits are already in the default branch.

use crate::git;
use crate::palette::{Paint, COMMENT, CYAN, GREEN, ORANGE};
use anyhow::{Context, Result};
use colored::Colorize;
use git2::{BranchType, Repository};

fn default_branch(repo: &Repository) -> Result<String> {
    if let Ok(head) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Some(target) = head.symbolic_target() {
            if let Some(name) = target.strip_prefix("refs/remotes/origin/") {
                return Ok(name.to_string());
            }
        }
    }
    for candidate in ["main", "master"] {
        if repo.find_branch(candidate, BranchType::Local).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    anyhow::bail!("could not determine the default branch (no origin/HEAD, no main or master)")
}

pub fn run(yes: bool) -> Result<()> {
    git::fetch_pruned("origin")?;

    let repo = git::open_current()?;
    let default = default_branch(&repo)?;
    let current = git::current_branch().ok();

    let default_tip = repo
        .find_branch(&default, BranchType::Local)
        .or_else(|_| repo.find_branch(&format!("origin/{default}"), BranchType::Remote))
        .context("default branch has no local or remote-tracking ref")?
        .get()
        .peel_to_commit()?
        .id();

    let mut candidates = Vec::new();
    for branch in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = branch?;
        let Some(name) = branch.name()?.map(|s| s.to_string()) else {
            continue;
        };
        if name == default || current.as_deref() == Some(name.as_str()) {
            continue;
        }
        let remote_gone = repo
            .find_reference(&format!("refs/remotes/origin/{name}"))
            .is_err();
        if !remote_gone {
            continue;
        }
        let tip = branch.get().peel_to_commit()?.id();
        let merged = tip == default_tip || repo.graph_descendant_of(default_tip, tip)?;
        if merged {
            candidates.push((name, tip));
        }
    }

    if candidates.is_empty() {
        println!("{}", "no dead local branches to prune".tc(COMMENT));
        return Ok(());
    }

    println!(
        "{}",
        format!("Gone from origin and already merged into {default}:").tc(CYAN)
    );
    for (name, tip) in &candidates {
        println!(
            "  {}  {}",
            name.tc(GREEN),
            tip.to_string()[..7].tc(COMMENT)
        );
    }

    if !yes {
        println!();
        println!(
            "{}",
            "Dry run only — pass --yes to actually delete these branches.".tc(COMMENT)
        );
        return Ok(());
    }

    println!();
    for (name, _) in &candidates {
        let mut branch = repo.find_branch(name, BranchType::Local)?;
        branch.delete()?;
        println!("{} Deleted {}", "✓".green().bold(), name.tc(ORANGE));
    }
    Ok(())
}
