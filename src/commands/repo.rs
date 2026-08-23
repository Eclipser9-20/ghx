use crate::api::Client;
use crate::gitutil::{self, Repo};
use anyhow::Result;
use colored::Colorize;
use serde_json::Value;

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
}

fn resolve(repo: Option<String>) -> Result<(String, String)> {
    match repo {
        Some(slug) => gitutil::parse_slug(&slug),
        None => {
            let r = Repo::detect()?;
            Ok((r.owner, r.name))
        }
    }
}

pub fn run(client: &Client, cmd: RepoCommand) -> Result<()> {
    match cmd {
        RepoCommand::View { repo } => view(client, repo),
        RepoCommand::Clone { repo, dir } => clone(&repo, dir),
        RepoCommand::List { limit } => list(client, limit),
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
    let (owner, name) = gitutil::parse_slug(repo)?;
    let url = format!("https://github.com/{owner}/{name}.git");
    let mut args = vec!["clone", url.as_str()];
    if let Some(d) = &dir {
        args.push(d.as_str());
    }
    gitutil::run_inherit(&args)
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
