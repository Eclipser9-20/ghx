//! Native, simplified replacement for `git-filter-repo`: rewrite history to
//! keep/drop a path, or scrub text from blob contents, across every commit
//! on the current branch.
//!
//! Limitations (documented rather than hidden):
//! - Only the current branch's history is rewritten; other branches/tags
//!   are left pointing at old history.
//! - No merge-commit-aware pruning of now-empty merges — a merge commit is
//!   kept even if both its rewritten parents become identical.
//! - `--replace-text` does plain substring replacement, not filter-repo's
//!   regex/glob callback system.
//! - Binary blobs are left untouched by `--replace-text` (content is only
//!   rewritten when it's valid UTF-8).

use anyhow::{bail, Context, Result};
use git2::{Oid, Repository, Sort};
use std::collections::HashMap;

pub struct FilterOptions {
    pub path: Option<String>,
    pub invert_paths: bool,
    pub replace_text: Vec<(String, String)>,
    pub yes: bool,
}

pub struct FilterPlan {
    pub total_commits: usize,
    pub affected_commits: usize,
    pub emptied_commits: usize,
}

fn open_current() -> Result<Repository> {
    Repository::discover(".").context("not a git repository (or any parent up to the root)")
}

/// Does `entry_path` fall under the kept/removed path filter? `path` is
/// matched as an exact file or a directory prefix, mirroring filter-repo's
/// simple `--path` semantics.
fn matches_path(entry_path: &str, filter: &str) -> bool {
    entry_path == filter || entry_path.starts_with(&format!("{filter}/"))
}

/// Rebuild a tree, dropping or keeping entries under `path_filter` per
/// `invert`, and rewriting blob text per `replace_text`. Recurses into
/// subtrees; a subtree that becomes empty is dropped from its parent.
fn rewrite_tree(
    repo: &Repository,
    tree: &git2::Tree,
    prefix: &str,
    opts: &FilterOptions,
) -> Result<Oid> {
    let mut builder = repo.treebuilder(None)?;

    for entry in tree.iter() {
        let name = entry.name().unwrap_or("").to_string();
        let full_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };

        // Path filtering only makes a keep/drop decision at blobs — a
        // directory is always recursed into, and then dropped naturally if
        // recursion leaves it empty. This keeps the tree-walk simple: no
        // separate "does this directory contain the filter path" case to
        // get wrong.
        if let Some(filter) = &opts.path {
            if entry.kind() != Some(git2::ObjectType::Tree) {
                let under_filter = matches_path(&full_path, filter);
                let keep = under_filter != opts.invert_paths;
                if !keep {
                    continue;
                }
            }
        }

        match entry.kind() {
            Some(git2::ObjectType::Tree) => {
                let subtree = repo.find_tree(entry.id())?;
                let new_id = rewrite_tree(repo, &subtree, &full_path, opts)?;
                let new_tree = repo.find_tree(new_id)?;
                if new_tree.iter().next().is_some() {
                    builder.insert(&name, new_id, entry.filemode())?;
                }
            }
            Some(git2::ObjectType::Blob) => {
                if opts.replace_text.is_empty() {
                    builder.insert(&name, entry.id(), entry.filemode())?;
                } else {
                    let blob = repo.find_blob(entry.id())?;
                    match std::str::from_utf8(blob.content()) {
                        Ok(text) => {
                            let mut rewritten = text.to_string();
                            for (from, to) in &opts.replace_text {
                                rewritten = rewritten.replace(from.as_str(), to.as_str());
                            }
                            if rewritten == text {
                                builder.insert(&name, entry.id(), entry.filemode())?;
                            } else {
                                let new_blob = repo.blob(rewritten.as_bytes())?;
                                builder.insert(&name, new_blob, entry.filemode())?;
                            }
                        }
                        Err(_) => {
                            builder.insert(&name, entry.id(), entry.filemode())?;
                        }
                    }
                }
            }
            _ => {
                builder.insert(&name, entry.id(), entry.filemode())?;
            }
        }
    }

    Ok(builder.write()?)
}

fn walk_commits(repo: &Repository) -> Result<Vec<Oid>> {
    let mut walk = repo.revwalk()?;
    walk.push_head()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)?;
    Ok(walk.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn plan(opts: &FilterOptions) -> Result<FilterPlan> {
    let repo = open_current()?;
    let oids = walk_commits(&repo)?;
    let mut affected = 0;
    let mut emptied = 0;

    for oid in &oids {
        let commit = repo.find_commit(*oid)?;
        let tree = commit.tree()?;
        let new_tree_id = rewrite_tree(&repo, &tree, "", opts)?;
        if new_tree_id != tree.id() {
            affected += 1;
            let new_tree = repo.find_tree(new_tree_id)?;
            if new_tree.iter().next().is_none() && tree.iter().next().is_some() {
                emptied += 1;
            }
        }
    }

    Ok(FilterPlan {
        total_commits: oids.len(),
        affected_commits: affected,
        emptied_commits: emptied,
    })
}

/// Rewrite the current branch's history in place, per `opts`. Returns the
/// new tip OID.
pub fn run(opts: &FilterOptions) -> Result<Oid> {
    if !opts.yes {
        bail!("run with --yes to actually rewrite history (this was a dry run)");
    }

    let repo = open_current()?;
    let branch_name = crate::git::current_branch()?;
    let oids = walk_commits(&repo)?;

    let mut remap: HashMap<Oid, Option<Oid>> = HashMap::new();
    let mut last_new_tip: Option<Oid> = None;

    for oid in oids {
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let new_tree_id = rewrite_tree(&repo, &tree, "", opts)?;
        let new_tree = repo.find_tree(new_tree_id)?;

        let mut new_parents = Vec::new();
        for parent in commit.parents() {
            if let Some(Some(new_parent_oid)) = remap.get(&parent.id()) {
                new_parents.push(repo.find_commit(*new_parent_oid)?);
            }
            // A remapped-to-None parent (fully dropped commit) is elided —
            // its own parents were already folded in when it was processed.
        }
        let parent_refs: Vec<&git2::Commit> = new_parents.iter().collect();

        // A non-merge commit whose tree exactly matches its lone parent's
        // becomes a no-op after filtering — drop it rather than keep a dead
        // commit, unless it has no parent (would drop the repo's root).
        if parent_refs.len() == 1 && parent_refs[0].tree()?.id() == new_tree_id {
            remap.insert(oid, Some(parent_refs[0].id()));
            last_new_tip = Some(parent_refs[0].id());
            continue;
        }

        let new_oid = repo.commit_create_buffer(
            &commit.author(),
            &commit.committer(),
            commit.message().unwrap_or(""),
            &new_tree,
            &parent_refs,
        )?;
        let new_oid = repo.odb()?.write(git2::ObjectType::Commit, &new_oid)?;

        remap.insert(oid, Some(new_oid));
        last_new_tip = Some(new_oid);
    }

    let Some(new_tip) = last_new_tip else {
        bail!("no commits to rewrite");
    };

    let refname = format!("refs/heads/{branch_name}");
    repo.reference(
        &refname,
        new_tip,
        true,
        "ghx filter: history rewritten",
    )?;
    repo.set_head(&refname)?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;

    Ok(new_tip)
}

pub fn parse_replace_text(spec: &str) -> Result<(String, String)> {
    match spec.split_once('=') {
        Some((from, to)) => Ok((from.to_string(), to.to_string())),
        None => bail!("--replace-text expects PATTERN=REPLACEMENT, got \"{spec}\""),
    }
}
