use crate::api::Client;
use crate::git::{self, GhRepo};
use anyhow::Result;
use colored::Colorize;
use serde_json::{json, Value};

#[derive(clap::Subcommand)]
pub enum IssueCommand {
    /// List issues
    List {
        #[arg(short = 'R', long)]
        repo: Option<String>,
        #[arg(long, default_value = "open")]
        state: String,
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },
    /// View an issue
    View {
        number: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
    },
    /// Create an issue
    Create {
        #[arg(short = 'R', long)]
        repo: Option<String>,
        #[arg(short = 't', long)]
        title: String,
        #[arg(short = 'b', long, default_value = "")]
        body: String,
    },
    /// Add a comment to an issue
    Comment {
        number: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
        #[arg(short = 'b', long)]
        body: String,
    },
    /// Close an issue
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

pub fn run(client: &Client, cmd: IssueCommand) -> Result<()> {
    match cmd {
        IssueCommand::List { repo, state, limit } => list(client, repo, &state, limit),
        IssueCommand::View { number, repo } => view(client, repo, number),
        IssueCommand::Create { repo, title, body } => create(client, repo, title, body),
        IssueCommand::Comment { number, repo, body } => comment(client, repo, number, body),
        IssueCommand::Close { number, repo } => close(client, repo, number),
    }
}

/// The GitHub REST API represents PRs as issues too, so listing endpoints
/// return both — filter those out to behave like a real issue list.
fn is_pr(v: &Value) -> bool {
    v.get("pull_request").is_some()
}

fn list(client: &Client, repo: Option<String>, state: &str, limit: u32) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let issues: Vec<Value> = client.get(&format!(
        "/repos/{owner}/{name}/issues?state={state}&per_page=100"
    ))?;
    for issue in issues.iter().filter(|v| !is_pr(v)).take(limit as usize) {
        let number = issue["number"].as_u64().unwrap_or(0);
        let title = issue["title"].as_str().unwrap_or("?");
        let author = issue["user"]["login"].as_str().unwrap_or("?");
        println!(
            "{} {}  {}",
            format!("#{number}").green().bold(),
            title,
            format!("by {author}").dimmed()
        );
    }
    Ok(())
}

fn view(client: &Client, repo: Option<String>, number: u64) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let issue: Value = client.get(&format!("/repos/{owner}/{name}/issues/{number}"))?;
    let title = issue["title"].as_str().unwrap_or("?");
    let state = issue["state"].as_str().unwrap_or("?");
    let author = issue["user"]["login"].as_str().unwrap_or("?");
    let body = issue["body"].as_str().unwrap_or("");
    let url = issue["html_url"].as_str().unwrap_or("");

    println!("{} {}", format!("#{number}").green().bold(), title.bold());
    println!("{}  opened by {}", state.to_uppercase().yellow(), author);
    println!();
    if !body.is_empty() {
        println!("{body}");
        println!();
    }
    println!("{}", url.underline());
    Ok(())
}

fn create(client: &Client, repo: Option<String>, title: String, body: String) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let payload = json!({ "title": title, "body": body });
    let issue: Value = client.post(&format!("/repos/{owner}/{name}/issues"), &payload)?;
    let number = issue["number"].as_u64().unwrap_or(0);
    let url = issue["html_url"].as_str().unwrap_or("");
    println!(
        "{} Created issue {}",
        "✓".green().bold(),
        format!("#{number}").bold()
    );
    println!("{}", url.underline());
    Ok(())
}

fn comment(client: &Client, repo: Option<String>, number: u64, body: String) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let payload = json!({ "body": body });
    let _: Value = client.post(
        &format!("/repos/{owner}/{name}/issues/{number}/comments"),
        &payload,
    )?;
    println!("{} Commented on issue #{number}", "✓".green().bold());
    Ok(())
}

fn close(client: &Client, repo: Option<String>, number: u64) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let payload = json!({ "state": "closed" });
    let _: Value = client.patch(&format!("/repos/{owner}/{name}/issues/{number}"), &payload)?;
    println!("{} Closed issue #{number}", "✓".green().bold());
    Ok(())
}
