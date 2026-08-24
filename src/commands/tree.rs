//! `ghx tree` — commit history as a lane graph, or the HEAD file tree.

use crate::git;
use crate::palette::{Paint, COMMENT, CYAN, GREEN, ORANGE, TEAL};
use anyhow::Result;
use colored::Colorize;
use git2::{Oid, Repository};
use std::collections::HashMap;

fn ref_labels(repo: &Repository) -> Result<HashMap<Oid, Vec<String>>> {
    let mut labels: HashMap<Oid, Vec<String>> = HashMap::new();
    for reference in repo.references()? {
        let reference = reference?;
        let Some(name) = reference.shorthand().map(|s| s.to_string()) else {
            continue;
        };
        if name == "HEAD" {
            continue;
        }
        if let Ok(commit) = reference.peel_to_commit() {
            labels.entry(commit.id()).or_default().push(name);
        }
    }
    for names in labels.values_mut() {
        names.sort();
        names.dedup();
    }
    Ok(labels)
}

/// One row of the graph: lanes are open branches of history, and the lane
/// holding this commit is the one that gets a node drawn on it.
fn draw_lanes(lanes: &[Option<Oid>], active: usize) -> String {
    let mut row = String::new();
    for (i, lane) in lanes.iter().enumerate() {
        if i == active {
            row.push_str(&format!("{}", "●".tc(TEAL).bold()));
        } else if lane.is_some() {
            row.push_str(&format!("{}", "│".tc(COMMENT)));
        } else {
            row.push(' ');
        }
        row.push(' ');
    }
    row
}

pub fn run(limit: usize, files: bool) -> Result<()> {
    let repo = git::open_current()?;
    if files {
        return print_files(&repo);
    }

    let labels = ref_labels(&repo)?;
    let mut walk = repo.revwalk()?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;
    walk.push_glob("refs/heads/*")?;
    let _ = walk.push_glob("refs/tags/*");

    let mut lanes: Vec<Option<Oid>> = Vec::new();

    for (n, oid) in walk.enumerate() {
        if n >= limit {
            break;
        }
        let oid = oid?;
        let commit = repo.find_commit(oid)?;

        let active = match lanes.iter().position(|l| *l == Some(oid)) {
            Some(i) => i,
            None => {
                let free = lanes.iter().position(|l| l.is_none());
                match free {
                    Some(i) => {
                        lanes[i] = Some(oid);
                        i
                    }
                    None => {
                        lanes.push(Some(oid));
                        lanes.len() - 1
                    }
                }
            }
        };

        let graph = draw_lanes(&lanes, active);

        let refs = labels
            .get(&oid)
            .map(|names| format!("({}) ", names.join(", ")))
            .unwrap_or_default();

        println!(
            "{graph} {} {}{}  {}",
            oid.to_string()[..7].tc(ORANGE),
            refs.tc(CYAN),
            commit.summary().unwrap_or("").normal(),
            commit
                .author()
                .name()
                .unwrap_or("")
                .to_string()
                .tc(COMMENT)
        );

        let parents: Vec<Oid> = commit.parent_ids().collect();
        lanes[active] = parents.first().copied();
        for extra in parents.iter().skip(1) {
            if lanes.iter().any(|l| l == &Some(*extra)) {
                continue;
            }
            match lanes.iter().position(|l| l.is_none()) {
                Some(i) => lanes[i] = Some(*extra),
                None => lanes.push(Some(*extra)),
            }
        }
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
    }
    Ok(())
}

fn print_files(repo: &Repository) -> Result<()> {
    let tree = repo.head()?.peel_to_tree()?;
    walk_files(repo, &tree, "")?;
    Ok(())
}

fn walk_files(repo: &Repository, tree: &git2::Tree, prefix: &str) -> Result<()> {
    let count = tree.len();
    for (i, entry) in tree.iter().enumerate() {
        let last = i + 1 == count;
        let branch = if last { "└─ " } else { "├─ " };
        let name = entry.name().unwrap_or("").to_string();
        let is_dir = entry.kind() == Some(git2::ObjectType::Tree);
        println!(
            "{}{}{}",
            prefix.tc(COMMENT),
            branch.tc(COMMENT),
            if is_dir {
                name.tc(CYAN).bold()
            } else {
                name.tc(GREEN)
            }
        );
        if is_dir {
            let sub = entry.to_object(repo)?.peel_to_tree()?;
            let child_prefix = format!("{prefix}{}", if last { "   " } else { "│  " });
            walk_files(repo, &sub, &child_prefix)?;
        }
    }
    Ok(())
}
