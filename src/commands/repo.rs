use crate::api::Client;
use crate::git::{self, GhRepo};
use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::{json, Value};
use std::path::PathBuf;

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
    /// Create a new repository on GitHub and initialize it locally
    Create {
        /// Repository name, or owner/name to create under an org
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        private: bool,
        /// Initialize the current directory as the repo instead of creating a
        /// new subdirectory (publishes files already here)
        #[arg(long)]
        here: bool,
        /// Create and commit locally but don't push to GitHub yet
        #[arg(long)]
        no_push: bool,
    },
    /// Initialize a new local git repository (no GitHub repo created)
    Init {
        /// Directory to initialize (defaults to the current directory)
        dir: Option<String>,
    },
    /// Delete a repository (requires typing the full owner/repo to confirm)
    Delete {
        /// owner/repo — must be given in full, no default-to-current-dir, as a safety measure
        repo: String,
    },
    /// Change a repository's visibility
    Visibility {
        repo: String,
        #[arg(value_parser = ["public", "private"])]
        visibility: String,
    },
    /// List a repository's collaborators
    Collaborators { repo: String },
    /// Add a collaborator to a repository
    AddCollaborator {
        repo: String,
        username: String,
        #[arg(long, value_parser = ["pull", "push", "admin", "maintain", "triage"], default_value = "push")]
        permission: String,
    },
    /// Remove a collaborator from a repository
    RemoveCollaborator { repo: String, username: String },
    /// Show branch protection settings for a branch
    BranchProtection { repo: String, branch: String },
    /// Protect a branch
    Protect {
        repo: String,
        branch: String,
        #[arg(long)]
        require_reviews: Option<u64>,
        /// Comma-separated list of required status checks
        #[arg(long)]
        require_status_checks: Option<String>,
        #[arg(long)]
        enforce_admins: bool,
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
            here,
            no_push,
        } => create(client, &name, description, private, here, no_push),
        RepoCommand::Init { dir } => init(dir.as_deref()),
        RepoCommand::Delete { repo } => delete(client, &repo),
        RepoCommand::Visibility {
            repo,
            visibility: vis,
        } => visibility(client, &repo, &vis),
        RepoCommand::Collaborators { repo } => collaborators(client, &repo),
        RepoCommand::AddCollaborator {
            repo,
            username,
            permission,
        } => add_collaborator(client, &repo, &username, &permission),
        RepoCommand::RemoveCollaborator { repo, username } => {
            remove_collaborator(client, &repo, &username)
        }
        RepoCommand::BranchProtection { repo, branch } => branch_protection(client, &repo, &branch),
        RepoCommand::Protect {
            repo,
            branch,
            require_reviews,
            require_status_checks,
            enforce_admins,
        } => protect(
            client,
            &repo,
            &branch,
            require_reviews,
            require_status_checks,
            enforce_admins,
        ),
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

fn create(
    client: &Client,
    name: &str,
    description: Option<String>,
    private: bool,
    here: bool,
    no_push: bool,
) -> Result<()> {
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
    let html_url = data["html_url"].as_str().unwrap_or("");
    // The exact remote URL and the server-side repo name, straight from the
    // create response, so the local remote matches regardless of casing or
    // org routing.
    let clone_url = data["clone_url"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://github.com/{full_name}.git"));
    let repo_name = data["name"].as_str().unwrap_or(name);

    println!("{} Created {}", "✓".green().bold(), full_name.bold());
    println!("{}", html_url.underline());

    // Initialize locally so the new repo is immediately usable — this is the
    // step plain `gh repo create` leaves to the user, and skipping it is why
    // a freshly-created repo used to be "not a git repository".
    let path: PathBuf = if here {
        PathBuf::from(".")
    } else {
        PathBuf::from(repo_name)
    };

    let repo = git::init_repo(&path, Some(&clone_url))?;

    // A README gives the initial commit real content; don't overwrite one the
    // user already has (relevant with --here).
    let readme = path.join("README.md");
    if !readme.exists() {
        std::fs::write(&readme, format!("# {repo_name}\n"))
            .with_context(|| format!("writing {}", readme.display()))?;
    }

    let short = git::commit_all_in(&repo, "Initial commit")?;
    println!("{} Initialized local repo ({short})", "✓".green().bold());

    if no_push {
        println!("Skipped push (--no-push). Push later with `ghx push`.");
    } else {
        git::push_branch(&repo, "origin", "main")?;
        println!("{} Pushed main to origin", "✓".green().bold());
    }

    if !here {
        println!("\n  cd {repo_name}");
    }
    Ok(())
}

fn init(dir: Option<&str>) -> Result<()> {
    let path = PathBuf::from(dir.unwrap_or("."));
    git::init_repo(&path, None)?;
    let display = path.join(".git");
    println!(
        "{} Initialized empty git repository in {}",
        "✓".green().bold(),
        display.display()
    );
    Ok(())
}

fn delete(client: &Client, repo: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    client.delete(&format!("/repos/{owner}/{name}"))?;
    println!("{} Deleted {}", "✓".green().bold(), repo);
    Ok(())
}

fn visibility(client: &Client, repo: &str, visibility: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let private = visibility == "private";
    let body = json!({ "private": private });
    let _: Value = client.patch(&format!("/repos/{owner}/{name}"), &body)?;
    println!(
        "{} Set {repo} to {visibility}",
        "✓".green().bold()
    );
    Ok(())
}

fn collaborators(client: &Client, repo: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let collaborators: Vec<Value> = client.get(&format!("/repos/{owner}/{name}/collaborators"))?;
    for c in &collaborators {
        let login = c["login"].as_str().unwrap_or("?");
        let permissions = &c["permissions"];
        println!("{:<25} {}", login.bold(), permissions);
    }
    Ok(())
}

fn add_collaborator(client: &Client, repo: &str, username: &str, permission: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let body = json!({ "permission": permission });
    client.put(
        &format!("/repos/{owner}/{name}/collaborators/{username}"),
        &body,
    )?;
    println!(
        "{} Added {username} to {repo} with {permission} permission",
        "✓".green().bold()
    );
    Ok(())
}

fn remove_collaborator(client: &Client, repo: &str, username: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    client.delete(&format!(
        "/repos/{owner}/{name}/collaborators/{username}"
    ))?;
    println!("{} Removed {username} from {repo}", "✓".green().bold());
    Ok(())
}

fn branch_protection(client: &Client, repo: &str, branch: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let (status, data) = client.get_status(&format!(
        "/repos/{owner}/{name}/branches/{branch}/protection"
    ))?;
    if status == 404 {
        println!("{} is not protected", branch.bold());
        return Ok(());
    }
    if status >= 400 {
        anyhow::bail!("GitHub API error ({status}): {data}");
    }
    println!("{data:#}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn protect(
    client: &Client,
    repo: &str,
    branch: &str,
    require_reviews: Option<u64>,
    require_status_checks: Option<String>,
    enforce_admins: bool,
) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;

    let required_status_checks = match require_status_checks {
        Some(checks) => {
            let contexts: Vec<String> = checks.split(',').map(|s| s.trim().to_string()).collect();
            json!({ "strict": true, "contexts": contexts })
        }
        None => Value::Null,
    };

    let required_pull_request_reviews = match require_reviews {
        Some(n) => json!({ "required_approving_review_count": n }),
        None => Value::Null,
    };

    let body = json!({
        "required_status_checks": required_status_checks,
        "enforce_admins": enforce_admins,
        "required_pull_request_reviews": required_pull_request_reviews,
        "restrictions": Value::Null,
    });

    let _: Value = client.put_json(
        &format!("/repos/{owner}/{name}/branches/{branch}/protection"),
        &body,
    )?;
    println!(
        "{} Protected branch {} on {repo}",
        "✓".green().bold(),
        branch.bold()
    );
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
