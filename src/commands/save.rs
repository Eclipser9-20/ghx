//! `ghx save` / `ghx load` — named workspace snapshots.
//!
//! A slot is a stash (untracked files included) plus the branch it was taken
//! on, recorded in `.git/ghx-saves.json`. Stash indices shift as stashes come
//! and go, so slots are keyed by the stash commit OID and the index is looked
//! up again at load time.

use crate::git;
use crate::palette::{Paint, COMMENT, CYAN, GREEN, ORANGE};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SLOTS_FILE: &str = "ghx-saves.json";

#[derive(Serialize, Deserialize, Default)]
struct Slots {
    slots: BTreeMap<String, Slot>,
}

#[derive(Serialize, Deserialize)]
struct Slot {
    branch: String,
    stash: String,
}

fn slots_path(repo: &Repository) -> std::path::PathBuf {
    repo.path().join(SLOTS_FILE)
}

fn read_slots(repo: &Repository) -> Slots {
    std::fs::read(slots_path(repo))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_slots(repo: &Repository, slots: &Slots) -> Result<()> {
    let json = serde_json::to_vec_pretty(slots)?;
    std::fs::write(slots_path(repo), json).context("writing the saved-slot list")
}

fn stash_index_of(repo: &mut Repository, oid: git2::Oid) -> Option<usize> {
    let mut found = None;
    let _ = repo.stash_foreach(|index, _message, stash_oid| {
        if *stash_oid == oid {
            found = Some(index);
            return false;
        }
        true
    });
    found
}

pub fn save(name: Option<String>, list: bool) -> Result<()> {
    let mut repo = git::open_current()?;

    if list {
        let slots = read_slots(&repo);
        if slots.slots.is_empty() {
            println!("{}", "no saved slots".tc(COMMENT));
            return Ok(());
        }
        for (name, slot) in &slots.slots {
            println!(
                "{:<20} {}  {}",
                name.tc(GREEN),
                slot.branch.tc(CYAN),
                slot.stash[..7].tc(COMMENT)
            );
        }
        return Ok(());
    }

    let name = name.context("a slot name is required (or pass --list)")?;
    let branch = git::current_branch()?;
    let signature = repo.signature()?;
    let oid = repo
        .stash_save(
            &signature,
            &format!("ghx save: {name}"),
            Some(git2::StashFlags::INCLUDE_UNTRACKED),
        )
        .context("nothing to save, or the snapshot could not be taken")?;

    let mut slots = read_slots(&repo);
    slots.slots.insert(
        name.clone(),
        Slot {
            branch: branch.clone(),
            stash: oid.to_string(),
        },
    );
    write_slots(&repo, &slots)?;

    println!(
        "{} Saved {} on {} ({})",
        "✓".green().bold(),
        name.tc(GREEN),
        branch.tc(CYAN),
        oid.to_string()[..7].tc(COMMENT)
    );
    Ok(())
}

pub fn load(name: &str) -> Result<()> {
    let mut repo = git::open_current()?;
    let mut slots = read_slots(&repo);
    let Some(slot) = slots.slots.get(name) else {
        bail!("no saved slot named '{name}' — run `ghx save --list` to see them");
    };
    let oid = git2::Oid::from_str(&slot.stash)?;
    let branch = slot.branch.clone();

    if git::current_branch().ok().as_deref() != Some(branch.as_str()) {
        git::checkout(&branch)?;
        println!("{} Switched to {}", "✓".green().bold(), branch.tc(CYAN));
    }

    let Some(index) = stash_index_of(&mut repo, oid) else {
        bail!("the stash for '{name}' is gone — it was dropped or popped outside ghx");
    };
    repo.stash_pop(index, None)
        .context("re-applying the saved snapshot")?;

    slots.slots.remove(name);
    write_slots(&repo, &slots)?;

    println!(
        "{} Restored {} onto {}",
        "✓".green().bold(),
        name.tc(ORANGE),
        branch.tc(CYAN)
    );
    Ok(())
}
