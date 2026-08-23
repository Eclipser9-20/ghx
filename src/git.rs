//! Native git operations, implemented on top of libgit2 (via the `git2`
//! crate).

use crate::config::Config;
use anyhow::{bail, Context, Result};
use git2::{
    BranchType, Cred, CredentialType, FetchOptions, PushOptions, RemoteCallbacks, Repository,
    ResetType, StatusOptions,
};
use std::path::Path;

/// The owner/repo of a GitHub remote, plus the open repository handle.
pub struct GhRepo {
    pub owner: String,
    pub name: String,
}

impl GhRepo {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Detect the current repo from the `origin` remote of the repository
    /// containing the current directory.
    pub fn detect() -> Result<GhRepo> {
        let repo = open_current()?;
        let remote = repo
            .find_remote("origin")
            .context("no 'origin' remote configured — are you in a git repository?")?;
        let url = remote
            .url()
            .context("'origin' remote has no URL")?
            .to_string();
        Self::parse(&url)
            .with_context(|| format!("could not parse GitHub owner/repo from remote url: {url}"))
    }

    fn parse(url: &str) -> Option<GhRepo> {
        let url = url.trim().trim_end_matches(".git");

        if let Some(rest) = url.strip_prefix("git@github.com:") {
            let mut parts = rest.splitn(2, '/');
            return Some(GhRepo {
                owner: parts.next()?.to_string(),
                name: parts.next()?.to_string(),
            });
        }

        if let Some(rest) = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"))
            .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        {
            let mut parts = rest.splitn(2, '/');
            return Some(GhRepo {
                owner: parts.next()?.to_string(),
                name: parts.next()?.to_string(),
            });
        }

        None
    }
}

/// Explicit "owner/repo" argument accepted by most subcommands.
pub fn parse_slug(slug: &str) -> Result<(String, String)> {
    let mut parts = slug.splitn(2, '/');
    let owner = parts.next().filter(|s| !s.is_empty());
    let name = parts.next().filter(|s| !s.is_empty());
    match (owner, name) {
        (Some(o), Some(n)) => Ok((o.to_string(), n.to_string())),
        _ => bail!("expected \"owner/repo\", got \"{slug}\""),
    }
}

fn open_current() -> Result<Repository> {
    Repository::discover(".").context("not a git repository (or any parent up to the root)")
}

/// Credential callback shared by fetch/push/clone: tries the stored GHX
/// GitHub token over HTTPS, then falls back to the SSH agent and default
/// key locations for SSH remotes.
fn credentials_callback<'a>() -> impl FnMut(
    &str,
    Option<&str>,
    CredentialType,
) -> std::result::Result<Cred, git2::Error>
       + 'a {
    let token = Config::resolve_token().ok().flatten();
    move |_url, username_from_url, allowed| {
        if allowed.contains(CredentialType::SSH_KEY) {
            let user = username_from_url.unwrap_or("git");
            if let Ok(cred) = Cred::ssh_key_from_agent(user) {
                return Ok(cred);
            }
        }
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            match &token {
                // Fine-grained PATs (and GitHub Apps tokens) reject the
                // classic "token-as-username" convention with a 403 —
                // the token must be the password, with any non-empty
                // username.
                Some(t) => return Cred::userpass_plaintext("x-access-token", t),
                // Surface our own message instead of falling through to
                // Cred::default(), which libgit2 rejects with a generic
                // "Username and password must be provided" — accurate,
                // but it gives no hint that `ghx auth login` is the fix.
                None => {
                    return Err(git2::Error::from_str(
                        "not logged in — run `ghx auth login`",
                    ))
                }
            }
        }
        Cred::default()
    }
}

fn remote_callbacks() -> RemoteCallbacks<'static> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(credentials_callback());
    cb
}

pub fn clone(url: &str, dir: Option<&Path>) -> Result<()> {
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(remote_callbacks());
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fo);

    let target = match dir {
        Some(d) => d.to_path_buf(),
        None => {
            let name = url
                .trim_end_matches(".git")
                .rsplit(['/', ':'])
                .next()
                .context("could not infer directory name from URL")?;
            Path::new(name).to_path_buf()
        }
    };

    builder
        .clone(url, &target)
        .with_context(|| format!("cloning {url} into {}", target.display()))?;
    Ok(())
}

pub fn current_branch() -> Result<String> {
    let repo = open_current()?;
    let head = repo.head().context("HEAD is unborn (no commits yet?)")?;
    if head.is_branch() {
        Ok(head
            .shorthand()
            .context("branch name is not valid UTF-8")?
            .to_string())
    } else {
        bail!("HEAD is detached, not on a branch")
    }
}

pub fn status() -> Result<Vec<(String, String)>> {
    let repo = open_current()?;
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    let statuses = repo.statuses(Some(&mut opts))?;

    let mut out = Vec::new();
    for entry in statuses.iter() {
        let path = entry.path().unwrap_or("?").to_string();
        let s = entry.status();
        let code = if s.is_wt_new() || s.is_index_new() {
            "new"
        } else if s.is_wt_modified() || s.is_index_modified() {
            "modified"
        } else if s.is_wt_deleted() || s.is_index_deleted() {
            "deleted"
        } else if s.is_wt_renamed() || s.is_index_renamed() {
            "renamed"
        } else if s.is_conflicted() {
            "conflict"
        } else {
            "changed"
        };
        out.push((code.to_string(), path));
    }
    Ok(out)
}

pub struct LogEntry {
    pub id: String,
    pub summary: String,
    pub author: String,
    pub time: String,
}

pub fn log(limit: usize) -> Result<Vec<LogEntry>> {
    let repo = open_current()?;
    let mut walk = repo.revwalk()?;
    walk.push_head()?;

    let mut out = Vec::new();
    for oid in walk.take(limit) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let time = commit.time();
        let dt = chrono::DateTime::from_timestamp(time.seconds(), 0)
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        out.push(LogEntry {
            id: oid.to_string()[..7].to_string(),
            summary: commit.summary().unwrap_or("").to_string(),
            author: commit.author().name().unwrap_or("?").to_string(),
            time: dt,
        });
    }
    Ok(out)
}

pub fn diff(staged: bool) -> Result<String> {
    let repo = open_current()?;
    let diff = if staged {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, None)?
    } else {
        repo.diff_index_to_workdir(None, None)?
    };

    let mut buf = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if origin == '+' || origin == '-' || origin == ' ' {
            buf.push(origin as u8);
        }
        buf.extend_from_slice(line.content());
        true
    })?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

/// Files whose whole-file replacement is handled elsewhere (LFS pointer
/// swap) rather than via a plain working-tree read. Checked before falling
/// back to a normal `index.add_path`/`add_all`.
fn stage_lfs_aware(repo: &Repository, rel_path: &Path) -> Result<bool> {
    crate::lfs::stage_path(repo, rel_path)
}

pub fn add_all() -> Result<()> {
    let repo = open_current()?;

    // LFS-tracked paths get pointer-swapped individually; everything else
    // goes through the normal bulk add.
    let has_lfs_patterns = !crate::lfs::tracked_patterns()?.is_empty();
    if has_lfs_patterns {
        for (code, path) in status()? {
            let rel = Path::new(&path);
            if code == "deleted" {
                let mut index = repo.index()?;
                index.remove_path(rel).ok();
                index.write()?;
                continue;
            }
            if stage_lfs_aware(&repo, rel)? {
                continue;
            }
            let mut index = repo.index()?;
            index.add_path(rel).with_context(|| format!("adding {path}"))?;
            index.write()?;
        }
        return Ok(());
    }

    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    Ok(())
}

pub fn add(paths: &[String]) -> Result<()> {
    let repo = open_current()?;
    for p in paths {
        let rel = Path::new(p);
        if stage_lfs_aware(&repo, rel)? {
            continue;
        }
        let mut index = repo.index()?;
        index
            .add_path(rel)
            .with_context(|| format!("adding {p}"))?;
        index.write()?;
    }
    Ok(())
}

pub fn commit(message: &str) -> Result<String> {
    let repo = open_current()?;
    let sig = repo
        .signature()
        .context("could not determine author identity — set user.name/user.email")?;
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();

    let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
    Ok(oid.to_string()[..7].to_string())
}

pub fn branch_list() -> Result<Vec<(String, bool)>> {
    let repo = open_current()?;
    let current = current_branch().ok();
    let mut out = Vec::new();
    for b in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = b?;
        if let Some(name) = branch.name()? {
            let is_current = current.as_deref() == Some(name);
            out.push((name.to_string(), is_current));
        }
    }
    Ok(out)
}

pub fn branch_create(name: &str) -> Result<()> {
    let repo = open_current()?;
    let head = repo.head()?.peel_to_commit()?;
    repo.branch(name, &head, false)?;
    Ok(())
}

pub fn branch_delete(name: &str) -> Result<()> {
    let repo = open_current()?;
    let mut branch = repo.find_branch(name, BranchType::Local)?;
    branch.delete()?;
    Ok(())
}

pub fn checkout(refname: &str) -> Result<()> {
    let repo = open_current()?;

    // Try as a local branch first.
    if let Ok(branch) = repo.find_branch(refname, BranchType::Local) {
        let obj = branch.get().peel(git2::ObjectType::Commit)?;
        repo.checkout_tree(&obj, None)?;
        repo.set_head(&format!("refs/heads/{refname}"))?;
        return Ok(());
    }

    // Fall back to any revision (tag, commit, remote ref) — detached HEAD.
    let obj = repo
        .revparse_single(refname)
        .with_context(|| format!("no such branch or revision: {refname}"))?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head_detached(obj.id())?;
    Ok(())
}

/// Fetch a specific remote ref down to a local branch — used for `pr
/// checkout` (including forked-repo PRs via `pull/<n>/head`).
pub fn fetch_ref_as_branch(remote_name: &str, remote_ref: &str, local_branch: &str) -> Result<()> {
    let repo = open_current()?;
    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("no remote named '{remote_name}'"))?;

    let refspec = format!("+{remote_ref}:refs/remotes/{remote_name}/{local_branch}");
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(remote_callbacks());
    remote
        .fetch(&[refspec.as_str()], Some(&mut fo), None)
        .with_context(|| format!("fetching {remote_ref} from {remote_name}"))?;

    let remote_branch_ref = format!("refs/remotes/{remote_name}/{local_branch}");
    let target = repo
        .find_reference(&remote_branch_ref)
        .context("fetched ref not found after fetch")?
        .peel_to_commit()?;

    if repo.find_branch(local_branch, BranchType::Local).is_err() {
        repo.branch(local_branch, &target, false)?;
    }
    checkout(local_branch)
}

pub fn fetch(remote_name: &str) -> Result<()> {
    let repo = open_current()?;
    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("no remote named '{remote_name}'"))?;
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(remote_callbacks());
    remote
        .fetch(&[] as &[&str], Some(&mut fo), None)
        .with_context(|| format!("fetching from {remote_name}"))?;
    Ok(())
}

/// Fetch then fast-forward the current branch. Non-fast-forward states
/// (diverged history) are reported rather than silently merged, since a
/// real merge/rebase is a deliberate future feature, not a quiet default.
pub fn pull(remote_name: &str) -> Result<()> {
    fetch(remote_name)?;
    let repo = open_current()?;
    let branch = current_branch()?;
    let remote_ref = format!("refs/remotes/{remote_name}/{branch}");
    let remote_commit = repo
        .find_reference(&remote_ref)
        .with_context(|| format!("no remote-tracking ref {remote_ref} (try `ghx git fetch` first, or check the branch exists on the remote)"))?
        .peel_to_commit()?;

    let head_ref = repo.head()?;
    let head_commit = head_ref.peel_to_commit()?;

    if head_commit.id() == remote_commit.id() {
        return Ok(()); // already up to date
    }

    let (analysis, _) = repo.merge_analysis(&[&repo.reference_to_annotated_commit(
        &repo.find_reference(&remote_ref)?,
    )?])?;

    if analysis.is_fast_forward() {
        let mut reference = repo.find_reference(&format!("refs/heads/{branch}"))?;
        reference.set_target(remote_commit.id(), "ghx pull: fast-forward")?;
        repo.set_head(&format!("refs/heads/{branch}"))?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        Ok(())
    } else if analysis.is_up_to_date() {
        Ok(())
    } else {
        bail!(
            "cannot fast-forward — local and remote history have diverged. \
             A merge/rebase command isn't implemented yet; resolve manually."
        )
    }
}

pub fn push(remote_name: &str, branch: Option<&str>) -> Result<()> {
    let repo = open_current()?;
    let branch = match branch {
        Some(b) => b.to_string(),
        None => current_branch()?,
    };
    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("no remote named '{remote_name}'"))?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let mut po = PushOptions::new();
    po.remote_callbacks(remote_callbacks());
    remote
        .push(&[refspec.as_str()], Some(&mut po))
        .with_context(|| format!("pushing {branch} to {remote_name}"))?;
    Ok(())
}

pub fn push_tag(remote_name: &str, tag: &str) -> Result<()> {
    let repo = open_current()?;
    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("no remote named '{remote_name}'"))?;
    let refspec = format!("refs/tags/{tag}:refs/tags/{tag}");
    let mut po = PushOptions::new();
    po.remote_callbacks(remote_callbacks());
    remote
        .push(&[refspec.as_str()], Some(&mut po))
        .with_context(|| format!("pushing tag {tag} to {remote_name}"))?;
    Ok(())
}

// ---------------------------------------------------------------------
// stash — one of the most notoriously fiddly parts of git's own CLI
// ---------------------------------------------------------------------

pub fn stash_save(message: Option<&str>) -> Result<()> {
    let mut repo = open_current()?;
    let sig = repo
        .signature()
        .context("could not determine author identity — set user.name/user.email")?;
    repo.stash_save2(&sig, message, None)
        .context("nothing to stash (working tree clean?)")?;
    Ok(())
}

pub struct StashEntry {
    pub index: usize,
    pub message: String,
}

pub fn stash_list() -> Result<Vec<StashEntry>> {
    let mut repo = open_current()?;
    let mut out = Vec::new();
    repo.stash_foreach(|index, message, _oid| {
        out.push(StashEntry {
            index,
            message: message.to_string(),
        });
        true
    })?;
    Ok(out)
}

pub fn stash_pop(index: usize) -> Result<()> {
    let mut repo = open_current()?;
    repo.stash_pop(index, None)
        .with_context(|| format!("popping stash@{{{index}}}"))
}

pub fn stash_drop(index: usize) -> Result<()> {
    let mut repo = open_current()?;
    repo.stash_drop(index)
        .with_context(|| format!("dropping stash@{{{index}}}"))
}

// ---------------------------------------------------------------------
// tags
// ---------------------------------------------------------------------

pub fn tag_list() -> Result<Vec<String>> {
    let repo = open_current()?;
    let names = repo.tag_names(None)?;
    Ok(names.iter().flatten().map(str::to_string).collect())
}

pub fn tag_create(name: &str, message: Option<&str>) -> Result<()> {
    let repo = open_current()?;
    let head = repo.head()?.peel_to_commit()?;
    match message {
        Some(msg) => {
            let sig = repo
                .signature()
                .context("could not determine author identity — set user.name/user.email")?;
            repo.tag(name, head.as_object(), &sig, msg, false)?;
        }
        None => {
            repo.tag_lightweight(name, head.as_object(), false)?;
        }
    }
    Ok(())
}

pub fn tag_delete(name: &str) -> Result<()> {
    let repo = open_current()?;
    repo.tag_delete(name)
        .with_context(|| format!("deleting tag {name}"))
}

// ---------------------------------------------------------------------
// remotes
// ---------------------------------------------------------------------

pub fn remote_list() -> Result<Vec<(String, String)>> {
    let repo = open_current()?;
    let names = repo.remotes()?;
    let mut out = Vec::new();
    for name in names.iter().flatten() {
        if let Ok(remote) = repo.find_remote(name) {
            out.push((name.to_string(), remote.url().unwrap_or("").to_string()));
        }
    }
    Ok(out)
}

pub fn remote_add(name: &str, url: &str) -> Result<()> {
    let repo = open_current()?;
    repo.remote(name, url)
        .with_context(|| format!("adding remote {name}"))?;
    Ok(())
}

pub fn remote_remove(name: &str) -> Result<()> {
    let repo = open_current()?;
    repo.remote_delete(name)
        .with_context(|| format!("removing remote {name}"))
}

// ---------------------------------------------------------------------
// reset
// ---------------------------------------------------------------------

pub fn reset(mode: &str, target: &str) -> Result<()> {
    let repo = open_current()?;
    let obj = repo
        .revparse_single(target)
        .with_context(|| format!("no such revision: {target}"))?;
    let reset_type = match mode {
        "soft" => ResetType::Soft,
        "mixed" => ResetType::Mixed,
        "hard" => ResetType::Hard,
        other => bail!("unknown reset mode '{other}' (expected soft, mixed, or hard)"),
    };
    repo.reset(&obj, reset_type, None)
        .with_context(|| format!("resetting ({mode}) to {target}"))
}

pub fn merge(branch: &str) -> Result<String> {
    let repo = open_current()?;
    let their_branch = repo
        .find_branch(branch, BranchType::Local)
        .or_else(|_| repo.find_branch(&format!("origin/{branch}"), BranchType::Remote))
        .with_context(|| format!("no such branch: {branch}"))?;
    let their_commit = their_branch.get().peel_to_commit()?;
    let their_annotated = repo.find_annotated_commit(their_commit.id())?;

    let (analysis, _) = repo.merge_analysis(&[&their_annotated])?;

    if analysis.is_up_to_date() {
        return Ok("already up to date".to_string());
    }

    if analysis.is_fast_forward() {
        let refname = format!("refs/heads/{}", current_branch()?);
        let mut r = repo.find_reference(&refname)?;
        r.set_target(their_commit.id(), "ghx merge: fast-forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        return Ok(format!(
            "fast-forwarded to {}",
            &their_commit.id().to_string()[..7]
        ));
    }

    repo.merge(&[&their_annotated], None, None)?;
    let mut index = repo.index()?;
    if index.has_conflicts() {
        bail!("merge has conflicts — resolve them, then `ghx add` the fixed files and `ghx commit`");
    }

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = repo
        .signature()
        .context("could not determine author identity — set user.name/user.email")?;
    let head_commit = repo.head()?.peel_to_commit()?;
    let oid = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("Merge branch '{branch}'"),
        &tree,
        &[&head_commit, &their_commit],
    )?;
    repo.cleanup_state()?;
    Ok(format!("merged as {}", &oid.to_string()[..7]))
}
