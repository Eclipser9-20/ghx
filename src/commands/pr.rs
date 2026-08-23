use crate::api::Client;
use crate::git::{self, GhRepo};
use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::{json, Value};

#[derive(clap::Subcommand)]
pub enum PrCommand {
    /// List pull requests
    List {
        #[arg(short = 'R', long)]
        repo: Option<String>,
        #[arg(long, default_value = "open")]
        state: String,
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },
    /// View a pull request
    View {
        number: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
        /// Show the diff instead of the summary
        #[arg(long)]
        diff: bool,
    },
    /// Create a pull request from the current branch
    Create {
        #[arg(short = 'R', long)]
        repo: Option<String>,
        #[arg(short = 't', long)]
        title: String,
        #[arg(short = 'b', long, default_value = "")]
        body: String,
        /// Target branch to merge into (defaults to the repo's default branch)
        #[arg(long)]
        base: Option<String>,
        /// Source branch (defaults to the current git branch)
        #[arg(long)]
        head: Option<String>,
        /// Create as a draft PR
        #[arg(long)]
        draft: bool,
    },
    /// Check out a pull request's branch locally
    Checkout {
        number: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
    },
    /// Merge a pull request
    Merge {
        number: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
        #[arg(long, value_parser = ["merge", "squash", "rebase"], default_value = "merge")]
        method: String,
    },
    /// Close a pull request without merging
    Close {
        number: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
    },
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

pub fn run(client: &Client, cmd: PrCommand) -> Result<()> {
    match cmd {
        PrCommand::List { repo, state, limit } => list(client, repo, &state, limit),
        PrCommand::View { number, repo, diff } => view(client, repo, number, diff),
        PrCommand::Create {
            repo,
            title,
            body,
            base,
            head,
            draft,
        } => create(client, repo, title, body, base, head, draft),
        PrCommand::Checkout { number, repo } => checkout(client, repo, number),
        PrCommand::Merge {
            number,
            repo,
            method,
        } => merge(client, repo, number, &method),
        PrCommand::Close { number, repo } => close(client, repo, number),
    }
}

fn list(client: &Client, repo: Option<String>, state: &str, limit: u32) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let prs: Vec<Value> = client.get(&format!(
        "/repos/{owner}/{name}/pulls?state={state}&per_page={}",
        limit.min(100)
    ))?;
    for pr in prs.iter().take(limit as usize) {
        let number = pr["number"].as_u64().unwrap_or(0);
        let title = pr["title"].as_str().unwrap_or("?");
        let author = pr["user"]["login"].as_str().unwrap_or("?");
        let branch = pr["head"]["ref"].as_str().unwrap_or("?");
        println!(
            "{} {}  {}  {}",
            format!("#{number}").green().bold(),
            title,
            branch.cyan(),
            format!("by {author}").dimmed()
        );
    }
    Ok(())
}

fn view(client: &Client, repo: Option<String>, number: u64, diff: bool) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    if diff {
        let path = format!("/repos/{owner}/{name}/pulls/{number}");
        let text = client.get_raw(&path, "application/vnd.github.diff")?;
        println!("{text}");
        return Ok(());
    }

    let pr: Value = client.get(&format!("/repos/{owner}/{name}/pulls/{number}"))?;
    let title = pr["title"].as_str().unwrap_or("?");
    let state = pr["state"].as_str().unwrap_or("?");
    let author = pr["user"]["login"].as_str().unwrap_or("?");
    let base = pr["base"]["ref"].as_str().unwrap_or("?");
    let head = pr["head"]["ref"].as_str().unwrap_or("?");
    let body = pr["body"].as_str().unwrap_or("");
    let url = pr["html_url"].as_str().unwrap_or("");

    println!(
        "{} {}",
        format!("#{number}").green().bold(),
        title.bold()
    );
    println!(
        "{}  {} -> {}  by {}",
        state.to_uppercase().yellow(),
        head.cyan(),
        base.cyan(),
        author
    );
    println!();
    if !body.is_empty() {
        println!("{body}");
        println!();
    }
    println!("{}", url.underline());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn create(
    client: &Client,
    repo: Option<String>,
    title: String,
    body: String,
    base: Option<String>,
    head: Option<String>,
    draft: bool,
) -> Result<()> {
    let (owner, name) = resolve(repo)?;

    let base = match base {
        Some(b) => b,
        None => {
            let r: Value = client.get(&format!("/repos/{owner}/{name}"))?;
            r["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_string()
        }
    };
    let head = match head {
        Some(h) => h,
        None => git::current_branch().context("could not determine current branch")?,
    };

    let payload = json!({
        "title": title,
        "body": body,
        "base": base,
        "head": head,
        "draft": draft,
    });
    let pr: Value = client.post(&format!("/repos/{owner}/{name}/pulls"), &payload)?;
    let number = pr["number"].as_u64().unwrap_or(0);
    let url = pr["html_url"].as_str().unwrap_or("");
    println!(
        "{} Created pull request {}",
        "✓".green().bold(),
        format!("#{number}").bold()
    );
    println!("{}", url.underline());
    Ok(())
}

fn checkout(client: &Client, repo: Option<String>, number: u64) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let pr: Value = client.get(&format!("/repos/{owner}/{name}/pulls/{number}"))?;
    let head_ref = pr["head"]["ref"].as_str().unwrap_or("").to_string();
    let head_repo_full = pr["head"]["repo"]["full_name"].as_str().unwrap_or("");

    let local_branch = format!("pr-{number}");
    let remote_ref = if head_repo_full == format!("{owner}/{name}") {
        // Same-repo branch: fetch it directly.
        format!("refs/heads/{head_ref}")
    } else {
        // Fork PR: fetch via the special pull/<n>/head ref.
        format!("refs/pull/{number}/head")
    };
    git::fetch_ref_as_branch("origin", &remote_ref, &local_branch)
}

fn merge(client: &Client, repo: Option<String>, number: u64, method: &str) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let payload = json!({ "merge_method": method });
    let _: Value = client.put_json(&format!("/repos/{owner}/{name}/pulls/{number}/merge"), &payload)?;
    println!(
        "{} Merged pull request #{number} ({method})",
        "✓".green().bold()
    );
    Ok(())
}

fn close(client: &Client, repo: Option<String>, number: u64) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let payload = json!({ "state": "closed" });
    let _: Value = client.patch(&format!("/repos/{owner}/{name}/pulls/{number}"), &payload)?;
    println!("{} Closed pull request #{number}", "✓".green().bold());
    Ok(())
}
