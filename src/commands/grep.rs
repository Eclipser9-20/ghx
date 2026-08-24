//! `ghx grep` — regex search across tracked files.
//!
//! The tree walk is the expensive part in a large repo, so the path/blob
//! listing is cached under `.git/ghx-grep-index.json` and keyed by the HEAD
//! tree OID: unchanged HEAD means the listing is reused verbatim, and a
//! changed HEAD only costs one walk. Blob contents themselves come straight
//! out of the object database (or the working tree, when a file has
//! uncommitted edits, so matches reflect what's actually on disk).

use crate::git;
use crate::palette::{Paint, COMMENT, CYAN, GREEN, ORANGE};
use anyhow::{Context, Result};
use colored::Colorize;
use git2::{Oid, Repository};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Serialize, Deserialize)]
struct Index {
    tree: String,
    entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    path: String,
    blob: String,
}

const INDEX_FILE: &str = "ghx-grep-index.json";
const MAX_BLOB: usize = 2 * 1024 * 1024;

fn head_tree_oid(repo: &Repository) -> Result<Option<Oid>> {
    match repo.head() {
        Ok(head) => Ok(Some(head.peel_to_tree()?.id())),
        Err(_) => Ok(None),
    }
}

fn walk_tree(repo: &Repository, tree_oid: Oid) -> Result<Vec<Entry>> {
    let tree = repo.find_tree(tree_oid)?;
    let mut entries = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, item| {
        if item.kind() == Some(git2::ObjectType::Blob) {
            if let Some(name) = item.name() {
                entries.push(Entry {
                    path: format!("{dir}{name}"),
                    blob: item.id().to_string(),
                });
            }
        }
        git2::TreeWalkResult::Ok
    })?;
    Ok(entries)
}

fn load_or_build_index(repo: &Repository) -> Result<Vec<Entry>> {
    let Some(tree_oid) = head_tree_oid(repo)? else {
        return Ok(Vec::new());
    };
    let cache_path = repo.path().join(INDEX_FILE);
    let tree = tree_oid.to_string();

    if let Ok(bytes) = std::fs::read(&cache_path) {
        if let Ok(index) = serde_json::from_slice::<Index>(&bytes) {
            if index.tree == tree {
                return Ok(index.entries);
            }
        }
    }

    let entries = walk_tree(repo, tree_oid)?;
    let index = Index { tree, entries };
    if let Ok(json) = serde_json::to_vec(&index) {
        let _ = std::fs::write(&cache_path, json);
    }
    Ok(index.entries)
}

fn content_for(repo: &Repository, entry: &Entry) -> Option<String> {
    if let Some(workdir) = repo.workdir() {
        let on_disk = workdir.join(&entry.path);
        if on_disk.is_file() {
            let bytes = std::fs::read(&on_disk).ok()?;
            if bytes.len() > MAX_BLOB {
                return None;
            }
            return String::from_utf8(bytes).ok();
        }
    }
    let oid = Oid::from_str(&entry.blob).ok()?;
    let blob = repo.find_blob(oid).ok()?;
    if blob.size() > MAX_BLOB {
        return None;
    }
    String::from_utf8(blob.content().to_vec()).ok()
}

pub fn run(pattern: &str, path: Option<String>) -> Result<()> {
    let re = Regex::new(pattern).with_context(|| format!("invalid pattern: {pattern}"))?;
    let repo = git::open_current()?;
    let entries = load_or_build_index(&repo)?;

    let scope = path.map(|p| p.replace('\\', "/"));
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    let mut matches = 0usize;

    for entry in &entries {
        if let Some(scope) = &scope {
            if !entry.path.starts_with(scope.trim_end_matches('/')) {
                continue;
            }
        }
        let Some(text) = content_for(&repo, entry) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if !re.is_match(line) {
                continue;
            }
            matches += 1;
            writeln!(
                out,
                "{}{}{}{}{}",
                entry.path.tc(CYAN),
                ":".tc(COMMENT),
                (i + 1).to_string().tc(ORANGE),
                ":".tc(COMMENT),
                highlight(&re, line)
            )?;
        }
    }

    out.flush()?;
    if matches == 0 {
        println!("{}", format!("no matches for {pattern}").tc(COMMENT));
    }
    Ok(())
}

fn highlight(re: &Regex, line: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for m in re.find_iter(line) {
        if m.start() < last {
            continue;
        }
        out.push_str(&line[last..m.start()]);
        out.push_str(&format!("{}", m.as_str().tc(GREEN).bold()));
        last = m.end();
    }
    out.push_str(&line[last..]);
    out
}
