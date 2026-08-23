use crate::api::Client;
use crate::git::{self, GhRepo};
use anyhow::Result;
use colored::Colorize;
use serde_json::{json, Value};

#[derive(clap::Subcommand)]
pub enum RepoCommand {
    /// Show details about a repository
    View {
        /// owner/repo (defaults to the current directory's repo)
        repo: Option<String>,
    },
    /// Clone a repository
    Clone {
        /// owner/repo to clone
        repo: String,
        /// Directory to clone into (defaults to the repo name)
        dir: Option<String>,
    },
    /// List repositories for the authenticated user
    List {
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },
    /// Create a new repository
    Create {
        /// Repository name, or owner/name to create under an org
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        private: bool,
    },
    /// Delete a repository (requires typing the full owner/repo to confirm)
    Delete {
        /// owner/repo — must be given in full, no default-to-current-dir, as a safety measure
        repo: String,
    },
}

/// Print a file's contents from a repo, given "owner/repo/path/to/file"
/// instead of a full raw.githubusercontent.com URL.
pub fn raw(client: &Client, spec: &str, git_ref: Option<String>) -> Result<()> {
    let mut parts = spec.splitn(3, '/');
    let owner = parts.next().filter(|s| !s.is_empty());
    let name = parts.next().filter(|s| !s.is_empty());
    let path = parts.next().filter(|s| !s.is_empty());
    let (Some(owner), Some(name), Some(path)) = (owner, name, path) else {
        anyhow::bail!("expected \"owner/repo/path/to/file\", got \"{spec}\"");
    };

    let git_ref = match git_ref {
        Some(r) => r,
        None => {
            let data: Value = client.get(&format!("/repos/{owner}/{name}"))?;
            data["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_string()
        }
    };

    let url = format!("https://raw.githubusercontent.com/{owner}/{name}/{git_ref}/{path}");
    let text = client.get_raw(&url, "*/*")?;
    print!("{text}");
    Ok(())
}

fn resolve(repo: Option<String>) -> Result<(String, String)> {
    match repo {
        Some(slug) => git::parse_slug(&slug),
        None => {
            let r = GhRepo::detect()?;
            Ok((r.owner, r.name))
        }
    }
}

pub fn run(client: &Client, cmd: RepoCommand) -> Result<()> {
    match cmd {
        RepoCommand::View { repo } => view(client, repo),
        RepoCommand::Clone { repo, dir } => clone(&repo, dir),
        RepoCommand::List { limit } => list(client, limit),
        RepoCommand::Create {
            name,
            description,
            private,
        } => create(client, &name, description, private),
        RepoCommand::Delete { repo } => delete(client, &repo),
    }
}

fn view(client: &Client, repo: Option<String>) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let data: Value = client.get(&format!("/repos/{owner}/{name}"))?;

    let full_name = data["full_name"].as_str().unwrap_or("?");
    let description = data["description"].as_str().unwrap_or("");
    let stars = data["stargazers_count"].as_u64().unwrap_or(0);
    let forks = data["forks_count"].as_u64().unwrap_or(0);
    let open_issues = data["open_issues_count"].as_u64().unwrap_or(0);
    let default_branch = data["default_branch"].as_str().unwrap_or("?");
    let url = data["html_url"].as_str().unwrap_or("");
    let private = data["private"].as_bool().unwrap_or(false);

    println!("{}", full_name.bold());
    if !description.is_empty() {
        println!("{description}");
    }
    println!();
    println!(
        "{}  {} stars   {} forks   {} open issues   default branch: {}",
        if private { "private".yellow() } else { "public".green() },
        stars,
        forks,
        open_issues,
        default_branch.cyan(),
    );
    println!("{}", url.underline());
    Ok(())
}

fn clone(repo: &str, dir: Option<String>) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let url = format!("https://github.com/{owner}/{name}.git");
    git::clone(&url, dir.as_ref().map(std::path::Path::new))
}

fn create(client: &Client, name: &str, description: Option<String>, private: bool) -> Result<()> {
    let body = json!({
        "name": name,
        "description": description.unwrap_or_default(),
        "private": private,
    });

    let data: Value = if let Some((org, repo_name)) = name.split_once('/') {
        client.post(
            &format!("/orgs/{org}/repos"),
            &json!({
                "name": repo_name,
                "description": body["description"],
                "private": private,
            }),
        )?
    } else {
        client.post("/user/repos", &body)?
    };

    let full_name = data["full_name"].as_str().unwrap_or(name);
    let url = data["html_url"].as_str().unwrap_or("");
    println!("{} Created {}", "✓".green().bold(), full_name.bold());
    println!("{}", url.underline());
    Ok(())
}

fn delete(client: &Client, repo: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    client.delete(&format!("/repos/{owner}/{name}"))?;
    println!("{} Deleted {}", "✓".green().bold(), repo);
    Ok(())
}

fn list(client: &Client, limit: u32) -> Result<()> {
    let repos: Vec<Value> = client.get(&format!(
        "/user/repos?per_page={}&sort=updated",
        limit.min(100)
    ))?;
    for r in repos.iter().take(limit as usize) {
        let name = r["full_name"].as_str().unwrap_or("?");
        let private = r["private"].as_bool().unwrap_or(false);
        let tag = if private { "private".yellow() } else { "public".green() };
        println!("{name:<45} {tag}");
    }
    Ok(())
}
