use crate::api::Client;
use crate::git;
use anyhow::Result;
use colored::Colorize;
use serde_json::{json, Value};

#[derive(clap::Subcommand)]
pub enum WebhookCommand {
    /// List webhooks for a repository
    List { repo: String },
    /// Create a webhook for a repository
    Create {
        repo: String,
        url: String,
        /// Comma-separated list of events (defaults to "push")
        #[arg(long)]
        events: Option<String>,
        #[arg(long)]
        secret: Option<String>,
    },
    /// Delete a webhook from a repository
    Delete { repo: String, hook_id: u64 },
}

pub fn run(client: &Client, cmd: WebhookCommand) -> Result<()> {
    match cmd {
        WebhookCommand::List { repo } => list(client, &repo),
        WebhookCommand::Create {
            repo,
            url,
            events,
            secret,
        } => create(client, &repo, &url, events, secret),
        WebhookCommand::Delete { repo, hook_id } => delete(client, &repo, hook_id),
    }
}

fn list(client: &Client, repo: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let hooks: Vec<Value> = client.get(&format!("/repos/{owner}/{name}/hooks"))?;
    for hook in &hooks {
        let id = hook["id"].as_u64().unwrap_or(0);
        let hook_url = hook["config"]["url"].as_str().unwrap_or("?");
        let active = hook["active"].as_bool().unwrap_or(false);
        let events: Vec<String> = hook["events"]
            .as_array()
            .map(|a| a.iter().filter_map(|e| e.as_str().map(String::from)).collect())
            .unwrap_or_default();
        println!(
            "{}  {}  {}  {}",
            id.to_string().bold(),
            hook_url,
            if active { "active".green() } else { "inactive".yellow() },
            events.join(",").dimmed()
        );
    }
    Ok(())
}

fn create(
    client: &Client,
    repo: &str,
    url: &str,
    events: Option<String>,
    secret: Option<String>,
) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let events: Vec<String> = events
        .map(|e| e.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|| vec!["push".to_string()]);

    let mut config = json!({
        "url": url,
        "content_type": "json",
    });
    if let Some(secret) = secret {
        config["secret"] = json!(secret);
    }

    let body = json!({
        "name": "web",
        "active": true,
        "events": events,
        "config": config,
    });

    let hook: Value = client.post(&format!("/repos/{owner}/{name}/hooks"), &body)?;
    let id = hook["id"].as_u64().unwrap_or(0);
    println!("{} Created webhook {}", "✓".green().bold(), id.to_string().bold());
    Ok(())
}

fn delete(client: &Client, repo: &str, hook_id: u64) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    client.delete(&format!("/repos/{owner}/{name}/hooks/{hook_id}"))?;
    println!("{} Deleted webhook {hook_id}", "✓".green().bold());
    Ok(())
}
