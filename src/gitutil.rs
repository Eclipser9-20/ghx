use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

/// Run `git` with the given args in the current directory, inheriting
/// stdio so interactive/paged output (log, diff, etc.) works normally.
pub fn run_inherit(args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .status()
        .context("failed to run git — is it installed and on PATH?")?;
    if !status.success() {
        bail!("git {} exited with {}", args.join(" "), status);
    }
    Ok(())
}

/// Run `git` and capture stdout as a trimmed string.
pub fn run_capture(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run git — is it installed and on PATH?")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The owner/repo of the current directory's `origin` remote, parsed from
/// either an HTTPS or SSH GitHub remote URL.
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl Repo {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Detect the current repo from `git remote get-url origin`. Errors
    /// with a helpful message if not in a git repo or origin isn't GitHub.
    pub fn detect() -> Result<Repo> {
        let url = run_capture(&["remote", "get-url", "origin"])
            .context("could not read git remote 'origin' — are you in a git repository?")?;
        Self::parse(&url)
            .with_context(|| format!("could not parse GitHub owner/repo from remote url: {url}"))
    }

    fn parse(url: &str) -> Option<Repo> {
        let url = url.trim().trim_end_matches(".git");

        // SSH form: git@github.com:owner/repo
        if let Some(rest) = url.strip_prefix("git@github.com:") {
            let mut parts = rest.splitn(2, '/');
            let owner = parts.next()?.to_string();
            let name = parts.next()?.to_string();
            return Some(Repo { owner, name });
        }

        // HTTPS form: https://github.com/owner/repo
        if let Some(rest) = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"))
        {
            let mut parts = rest.splitn(2, '/');
            let owner = parts.next()?.to_string();
            let name = parts.next()?.to_string();
            return Some(Repo { owner, name });
        }

        None
    }
}

/// Explicit owner/repo argument accepted by most subcommands, e.g. "owner/repo".
pub fn parse_slug(slug: &str) -> Result<(String, String)> {
    let mut parts = slug.splitn(2, '/');
    let owner = parts.next().filter(|s| !s.is_empty());
    let name = parts.next().filter(|s| !s.is_empty());
    match (owner, name) {
        (Some(o), Some(n)) => Ok((o.to_string(), n.to_string())),
        _ => bail!("expected \"owner/repo\", got \"{slug}\""),
    }
}
