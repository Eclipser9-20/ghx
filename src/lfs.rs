//! Native Git LFS support: pointer-file encoding, the local object cache,
//! and the GitHub LFS batch API client. No shelling out to `git-lfs` — the
//! pointer format and the batch API are implemented directly here,
//! consistent with how `git.rs` talks to libgit2 directly rather than
//! shelling out to `git`.
//!
//! Design: rather than wiring libgit2 smudge/clean filters (which are a
//! process-based mechanism meant for the real `git-lfs` binary), `ghx`
//! handles the pointer <-> real-content swap itself at the two points that
//! matter: `ghx add` (working tree -> pointer in the index, real content
//! into the local cache) and `ghx checkout`/`ghx pull` (pointer in the
//! working tree -> real content, fetched from the cache or the LFS server).

use crate::config::Config;
use crate::git::GhRepo;
use anyhow::{bail, Context, Result};
use git2::{IndexEntry, IndexTime, Oid, Repository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const SPEC_LINE: &str = "version https://git-lfs.github.com/spec/v1";

pub struct Pointer {
    pub oid: String,
    pub size: u64,
}

impl Pointer {
    fn to_bytes(&self) -> Vec<u8> {
        format!("{SPEC_LINE}\noid sha256:{}\nsize {}\n", self.oid, self.size).into_bytes()
    }
}

/// Parse a file's content as an LFS pointer, if it looks like one. Detection
/// is the spec magic line, per the git-lfs pointer format.
pub fn parse_pointer(content: &[u8]) -> Option<Pointer> {
    let text = std::str::from_utf8(content).ok()?;
    let mut lines = text.lines();
    if lines.next()? != SPEC_LINE {
        return None;
    }
    let mut oid = None;
    let mut size = None;
    for line in lines {
        if let Some(rest) = line.strip_prefix("oid sha256:") {
            oid = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("size ") {
            size = rest.trim().parse::<u64>().ok();
        }
    }
    Some(Pointer {
        oid: oid?,
        size: size?,
    })
}

fn open_current() -> Result<Repository> {
    Repository::discover(".").context("not a git repository (or any parent up to the root)")
}

fn workdir(repo: &Repository) -> Result<PathBuf> {
    repo.workdir()
        .context("repository has no working directory (bare repo?)")
        .map(Path::to_path_buf)
}

fn gitattributes_path(repo: &Repository) -> Result<PathBuf> {
    Ok(workdir(repo)?.join(".gitattributes"))
}

/// `.git/lfs/objects/<oid[0:2]>/<oid[2:4]>/<oid>` — mirrors real Git LFS's
/// on-disk cache layout.
fn cache_object_path(repo: &Repository, oid: &str) -> PathBuf {
    repo.path()
        .join("lfs")
        .join("objects")
        .join(&oid[0..2])
        .join(&oid[2..4])
        .join(oid)
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

// A tiny hex encoder so we don't pull in another dependency just for this.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let mut s = String::with_capacity(bytes.as_ref().len() * 2);
        for b in bytes.as_ref() {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

// ---------------------------------------------------------------------
// install / track / untrack
// ---------------------------------------------------------------------

/// Enable LFS for the current repo: create `.gitattributes` if missing, and
/// mark the repo as LFS-managed in its local (non-shared) git config, so
/// `ghx` knows to intercept `add`/`checkout` here.
pub fn install() -> Result<()> {
    let repo = open_current()?;
    let attrs = gitattributes_path(&repo)?;
    if !attrs.exists() {
        fs::write(&attrs, "").with_context(|| format!("creating {}", attrs.display()))?;
    }
    let mut cfg = repo.config().context("opening repo config")?;
    cfg.set_bool("lfs.ghx", true)
        .context("marking repo as LFS-managed")?;
    let cache_dir = repo.path().join("lfs").join("objects");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("creating {}", cache_dir.display()))?;
    Ok(())
}

/// Whether `ghx lfs install` has been run in the current repo.
pub fn is_installed() -> Result<bool> {
    let repo = open_current()?;
    let cfg = repo.config().context("opening repo config")?;
    Ok(cfg.get_bool("lfs.ghx").unwrap_or(false))
}

fn read_gitattributes(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text.lines().map(str::to_string).collect())
}

fn write_gitattributes(path: &Path, lines: &[String]) -> Result<()> {
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn lfs_attrs_line(pattern: &str) -> String {
    format!("{pattern} filter=lfs diff=lfs merge=lfs -text")
}

/// The tracked pattern from a `.gitattributes` line, if it's an LFS filter
/// line (`<pattern> filter=lfs ...`).
fn pattern_of(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace();
    let pattern = parts.next()?;
    if parts.any(|p| p == "filter=lfs") {
        Some(pattern)
    } else {
        None
    }
}

pub fn track(pattern: &str) -> Result<()> {
    let repo = open_current()?;
    let path = gitattributes_path(&repo)?;
    let mut lines = read_gitattributes(&path)?;
    if lines.iter().any(|l| pattern_of(l) == Some(pattern)) {
        return Ok(()); // already tracked
    }
    lines.push(lfs_attrs_line(pattern));
    write_gitattributes(&path, &lines)
}

pub fn untrack(pattern: &str) -> Result<()> {
    let repo = open_current()?;
    let path = gitattributes_path(&repo)?;
    let mut lines = read_gitattributes(&path)?;
    lines.retain(|l| pattern_of(l) != Some(pattern));
    write_gitattributes(&path, &lines)
}

pub fn tracked_patterns() -> Result<Vec<String>> {
    let repo = open_current()?;
    let path = gitattributes_path(&repo)?;
    Ok(read_gitattributes(&path)?
        .iter()
        .filter_map(|l| pattern_of(l))
        .map(str::to_string)
        .collect())
}

/// Minimal glob match supporting `*` (any run of characters) and `?` (any
/// single character) — enough for the patterns git-lfs `track` typically
/// deals with (`*.psd`, `assets/*.mp4`, `*.bin`, etc.). Patterns with no `/`
/// match against the file's basename, like `.gitattributes` semantics;
/// patterns with a `/` match the full relative path.
pub fn glob_match(pattern: &str, rel_path: &str) -> bool {
    let candidate = if pattern.contains('/') {
        rel_path
    } else {
        rel_path.rsplit('/').next().unwrap_or(rel_path)
    };
    glob_match_raw(pattern.as_bytes(), candidate.as_bytes())
}

fn glob_match_raw(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            glob_match_raw(&pattern[1..], text)
                || (!text.is_empty() && glob_match_raw(pattern, &text[1..]))
        }
        (Some(b'?'), Some(_)) => glob_match_raw(&pattern[1..], &text[1..]),
        (Some(pc), Some(tc)) if pc == tc => glob_match_raw(&pattern[1..], &text[1..]),
        _ => false,
    }
}

pub fn is_tracked(rel_path: &str) -> Result<bool> {
    let patterns = tracked_patterns()?;
    Ok(patterns.iter().any(|p| glob_match(p, rel_path)))
}

// ---------------------------------------------------------------------
// staging: working-tree file -> pointer in the index + real content cached
// ---------------------------------------------------------------------

/// Stage `rel_path` LFS-aware: if it matches a tracked pattern, write a
/// pointer file into the index (decoupled from the real working-tree
/// content via `add_frombuffer`) and cache the real content locally.
/// Returns `true` if handled as LFS; `false` means the caller should fall
/// back to a normal `index.add_path`.
pub fn stage_path(repo: &Repository, rel_path: &Path) -> Result<bool> {
    let rel_str = rel_path.to_string_lossy().replace('\\', "/");
    if !is_tracked(&rel_str)? {
        return Ok(false);
    }

    let abs_path = workdir(repo)?.join(rel_path);
    let content = fs::read(&abs_path).with_context(|| format!("reading {}", abs_path.display()))?;

    // Content already a pointer (e.g. re-adding after checkout) — leave it
    // as-is rather than pointer-wrapping a pointer.
    if parse_pointer(&content).is_some() {
        return Ok(false);
    }

    let oid = sha256_hex(&content);
    let size = content.len() as u64;
    let pointer = Pointer {
        oid: oid.clone(),
        size,
    };
    let pointer_bytes = pointer.to_bytes();

    let cache_path = cache_object_path(repo, &oid);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    if !cache_path.exists() {
        fs::write(&cache_path, &content)
            .with_context(|| format!("writing LFS cache object {}", cache_path.display()))?;
    }

    let mut index = repo.index().context("opening index")?;
    let entry = IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: 0o100644,
        uid: 0,
        gid: 0,
        file_size: 0,
        id: Oid::zero(),
        flags: 0,
        flags_extended: 0,
        path: rel_str.into_bytes(),
    };
    index
        .add_frombuffer(&entry, &pointer_bytes)
        .context("staging LFS pointer")?;
    index.write().context("writing index")?;

    Ok(true)
}

// ---------------------------------------------------------------------
// batch API
// ---------------------------------------------------------------------

#[derive(Serialize)]
struct BatchRequest<'a> {
    operation: &'a str,
    transfers: Vec<&'a str>,
    objects: Vec<BatchObjectReq>,
}

#[derive(Serialize)]
struct BatchObjectReq {
    oid: String,
    size: u64,
}

#[derive(Deserialize)]
struct BatchResponse {
    objects: Vec<BatchObjectResp>,
}

#[derive(Deserialize)]
struct BatchObjectResp {
    oid: String,
    #[allow(dead_code)]
    size: u64,
    #[serde(default)]
    actions: Option<BatchActions>,
    #[serde(default)]
    error: Option<BatchError>,
}

#[derive(Deserialize)]
struct BatchActions {
    #[serde(default)]
    upload: Option<BatchAction>,
    #[serde(default)]
    download: Option<BatchAction>,
}

#[derive(Deserialize)]
struct BatchAction {
    href: String,
    #[serde(default)]
    header: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct BatchError {
    #[allow(dead_code)]
    code: u32,
    message: String,
}

fn lfs_url(repo: &GhRepo) -> String {
    format!(
        "https://github.com/{}/{}.git/info/lfs",
        repo.owner, repo.name
    )
}

/// `Authorization: Basic base64(user:token)` — the documented convention
/// for GitHub's LFS batch endpoint (distinct from the Bearer auth used
/// against the regular GitHub REST API in `api.rs`).
fn basic_auth_header(token: &str) -> String {
    use base64::Engine;
    let raw = format!("x-access-token:{token}");
    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(raw))
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("ghx/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("building HTTP client")
}

fn batch(
    repo: &GhRepo,
    operation: &str,
    objects: Vec<BatchObjectReq>,
) -> Result<Vec<BatchObjectResp>> {
    if objects.is_empty() {
        return Ok(Vec::new());
    }
    let token = Config::resolve_token()?
        .context("not authenticated — run `ghx auth login` first")?;
    let client = http_client()?;
    let body = BatchRequest {
        operation,
        transfers: vec!["basic"],
        objects,
    };
    let url = format!("{}/objects/batch", lfs_url(repo));
    let resp = client
        .post(&url)
        .header("Authorization", basic_auth_header(&token))
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/vnd.git-lfs+json")
        .json(&body)
        .send()
        .with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    let text = resp.text().context("reading LFS batch response")?;
    if !status.is_success() {
        bail!("LFS batch API error ({status}): {text}");
    }
    let parsed: BatchResponse =
        serde_json::from_str(&text).with_context(|| format!("parsing LFS batch response: {text}"))?;
    Ok(parsed.objects)
}

// ---------------------------------------------------------------------
// push: upload cached objects for pointer files reachable from HEAD
// ---------------------------------------------------------------------

/// Scan the tree at HEAD for LFS pointer files and upload any whose object
/// isn't yet on the server. Meant to run right after a successful `ghx
/// push`.
pub fn push_objects(ghrepo: &GhRepo) -> Result<usize> {
    let repo = open_current()?;
    let pointers = pointers_in_head(&repo)?;
    if pointers.is_empty() {
        return Ok(0);
    }

    let objects: Vec<BatchObjectReq> = pointers
        .iter()
        .map(|p| BatchObjectReq {
            oid: p.oid.clone(),
            size: p.size,
        })
        .collect();
    let resp = batch(ghrepo, "upload", objects)?;

    let client = http_client()?;
    let mut uploaded = 0;
    for obj in resp {
        let Some(actions) = obj.actions else {
            continue; // no actions means the server already has it
        };
        let Some(upload) = actions.upload else {
            continue;
        };
        if let Some(err) = obj.error {
            bail!("LFS server error for {}: {}", obj.oid, err.message);
        }
        let cache_path = cache_object_path(&repo, &obj.oid);
        let content = fs::read(&cache_path).with_context(|| {
            format!(
                "LFS object {} not found in local cache ({}) — was it added with `ghx add`?",
                obj.oid,
                cache_path.display()
            )
        })?;

        let mut req = client.put(&upload.href).body(content);
        for (k, v) in &upload.header {
            req = req.header(k, v);
        }
        let resp = req.send().with_context(|| format!("PUT {}", upload.href))?;
        if !resp.status().is_success() {
            bail!(
                "uploading LFS object {} failed: {}",
                obj.oid,
                resp.status()
            );
        }
        uploaded += 1;
    }
    Ok(uploaded)
}

fn pointers_in_head(repo: &Repository) -> Result<Vec<Pointer>> {
    let mut out = Vec::new();
    let Ok(head) = repo.head() else {
        return Ok(out);
    };
    let Ok(tree) = head.peel_to_tree() else {
        return Ok(out);
    };
    tree.walk(git2::TreeWalkMode::PreOrder, |_root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            if let Some(obj) = entry.to_object(repo).ok() {
                if let Some(blob) = obj.as_blob() {
                    if let Some(p) = parse_pointer(blob.content()) {
                        out.push(p);
                    }
                }
            }
        }
        git2::TreeWalkResult::Ok
    })?;
    Ok(out)
}

// ---------------------------------------------------------------------
// pull/checkout: hydrate pointer files in the working tree
// ---------------------------------------------------------------------

struct WorkingPointer {
    rel_path: PathBuf,
    pointer: Pointer,
}

fn pointers_in_workdir(repo: &Repository) -> Result<Vec<WorkingPointer>> {
    let dir = workdir(repo)?;
    let mut out = Vec::new();
    // Walk the working tree directly rather than via git2::Statuses, since
    // we need to inspect arbitrary file content, not just status flags.
    walk_dir(&dir, &dir, &mut out)?;
    Ok(out)
}

fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<WorkingPointer>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        if file_name == ".git" {
            continue;
        }
        if path.is_dir() {
            walk_dir(root, &path, out)?;
            continue;
        }
        // Cheap size check before reading content: pointer files are tiny.
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() > 1024 {
            continue;
        }
        let Ok(content) = fs::read(&path) else { continue };
        if let Some(pointer) = parse_pointer(&content) {
            let rel_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.push(WorkingPointer { rel_path, pointer });
        }
    }
    Ok(())
}

/// Scan the working tree for LFS pointer files and hydrate any whose real
/// content isn't already cached locally, downloading from the server as
/// needed. Meant to run right after `ghx checkout`/`ghx pull`.
pub fn pull_objects(ghrepo: &GhRepo) -> Result<usize> {
    let repo = open_current()?;
    let dir = workdir(&repo)?;
    let pointers = pointers_in_workdir(&repo)?;

    let mut to_fetch = Vec::new();
    for wp in &pointers {
        let cache_path = cache_object_path(&repo, &wp.pointer.oid);
        if !cache_path.exists() {
            to_fetch.push(BatchObjectReq {
                oid: wp.pointer.oid.clone(),
                size: wp.pointer.size,
            });
        }
    }

    if !to_fetch.is_empty() {
        let resp = batch(ghrepo, "download", to_fetch)?;
        let client = http_client()?;
        for obj in resp {
            if let Some(err) = obj.error {
                bail!("LFS server error for {}: {}", obj.oid, err.message);
            }
            let Some(actions) = obj.actions else { continue };
            let Some(download) = actions.download else {
                continue;
            };
            let mut req = client.get(&download.href);
            for (k, v) in &download.header {
                req = req.header(k, v);
            }
            let resp = req
                .send()
                .with_context(|| format!("GET {}", download.href))?;
            if !resp.status().is_success() {
                bail!(
                    "downloading LFS object {} failed: {}",
                    obj.oid,
                    resp.status()
                );
            }
            let bytes = resp.bytes().context("reading LFS object body")?;
            let cache_path = cache_object_path(&repo, &obj.oid);
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            fs::write(&cache_path, &bytes)
                .with_context(|| format!("writing {}", cache_path.display()))?;
        }
    }

    let mut hydrated = 0;
    for wp in &pointers {
        let cache_path = cache_object_path(&repo, &wp.pointer.oid);
        if !cache_path.exists() {
            continue; // download failed or wasn't available — leave the pointer in place
        }
        let content = fs::read(&cache_path)
            .with_context(|| format!("reading LFS cache object {}", cache_path.display()))?;
        let abs_path = dir.join(&wp.rel_path);
        fs::write(&abs_path, &content)
            .with_context(|| format!("writing {}", abs_path.display()))?;
        hydrated += 1;
    }
    Ok(hydrated)
}

// ---------------------------------------------------------------------
// status
// ---------------------------------------------------------------------

pub enum LfsFileState {
    /// Pointer file in the working tree, real content cached locally.
    PointerCached,
    /// Pointer file in the working tree, real content not cached (needs a pull).
    PointerMissing,
}

pub struct LfsStatusEntry {
    pub path: PathBuf,
    pub state: LfsFileState,
    pub oid: String,
    pub size: u64,
}

pub fn status() -> Result<Vec<LfsStatusEntry>> {
    let repo = open_current()?;
    let pointers = pointers_in_workdir(&repo)?;
    let mut out = Vec::new();
    for wp in pointers {
        let cached = cache_object_path(&repo, &wp.pointer.oid).exists();
        out.push(LfsStatusEntry {
            path: wp.rel_path,
            state: if cached {
                LfsFileState::PointerCached
            } else {
                LfsFileState::PointerMissing
            },
            oid: wp.pointer.oid,
            size: wp.pointer.size,
        });
    }
    Ok(out)
}
