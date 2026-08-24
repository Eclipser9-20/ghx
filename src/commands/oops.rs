//! `ghx oops` — walk the HEAD reflog and hard-reset back to where HEAD was
//! before a recent operation.

use crate::git;
use crate::palette::{Paint, COMMENT, CYAN, GREEN, ORANGE, RED};
use anyhow::{bail, Result};
use colored::Colorize;

const DEPTH: usize = 5;

struct Step {
    /// What HEAD would be reset to.
    target: git2::Oid,
    /// The operation that moved HEAD away from `target`.
    undoes: String,
    summary: String,
}

fn steps(repo: &git2::Repository) -> Result<Vec<Step>> {
    let reflog = repo.reflog("HEAD")?;
    let mut out = Vec::new();
    for i in 1..=DEPTH {
        let Some(entry) = reflog.get(i) else { break };
        let target = entry.id_new();
        let undoes = reflog
            .get(i - 1)
            .and_then(|e| e.message().map(|m| m.to_string()))
            .unwrap_or_else(|| "(unknown)".into());
        let summary = repo
            .find_commit(target)
            .ok()
            .and_then(|c| c.summary().map(|s| s.to_string()))
            .unwrap_or_default();
        out.push(Step {
            target,
            undoes,
            summary,
        });
    }
    Ok(out)
}

pub fn run(index: Option<usize>) -> Result<()> {
    let repo = git::open_current()?;
    let steps = steps(&repo)?;
    if steps.is_empty() {
        println!("{}", "nothing in the HEAD reflog to undo".tc(COMMENT));
        return Ok(());
    }

    let Some(index) = index else {
        println!("{}", "Recent HEAD moves you can undo:".tc(CYAN));
        for (i, step) in steps.iter().enumerate() {
            println!(
                "  {} {} {}  {}",
                format!("{}", i + 1).tc(ORANGE).bold(),
                step.target.to_string()[..7].tc(GREEN),
                step.summary.tc(COMMENT),
                format!("undoes: {}", step.undoes).tc(COMMENT)
            );
        }
        println!();
        println!(
            "{}",
            "Run `ghx oops <N>` (default 1) to hard-reset the current branch to that commit.".tc(COMMENT)
        );
        return Ok(());
    };

    let Some(step) = index.checked_sub(1).and_then(|i| steps.get(i)) else {
        bail!("no reflog entry {index} — run `ghx oops` to see what's available");
    };

    println!(
        "{}",
        "THIS IS A HARD RESET. Uncommitted changes in the working tree will be discarded."
            .tc(RED)
            .bold()
    );
    println!(
        "{} {} {}",
        "Resetting the current branch to".tc(COMMENT),
        step.target.to_string()[..7].tc(ORANGE),
        step.summary.tc(COMMENT)
    );
    println!("{} {}", "Undoing:".tc(COMMENT), step.undoes.tc(COMMENT));
    println!();

    let obj = repo.find_object(step.target, None)?;
    repo.reset(&obj, git2::ResetType::Hard, None)?;
    println!(
        "{} Now at {}",
        "✓".green().bold(),
        step.target.to_string()[..7].tc(ORANGE)
    );
    Ok(())
}
