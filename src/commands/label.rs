use crate::api::Client;
use crate::git;
use anyhow::Result;
use colored::Colorize;
use serde_json::{json, Value};

#[derive(clap::Subcommand)]
pub enum LabelCommand {
    /// List labels for a repository
    List { repo: String },
    /// Create a label
    Create {
        repo: String,
        name: String,
        /// Hex color, without the leading '#'
        #[arg(long, default_value = "ededed")]
        color: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Delete a label
    Delete { repo: String, name: String },
}

pub fn run(client: &Client, cmd: LabelCommand) -> Result<()> {
    match cmd {
        LabelCommand::List { repo } => list(client, &repo),
        LabelCommand::Create {
            repo,
            name,
            color,
            description,
        } => create(client, &repo, &name, &color, &description),
        LabelCommand::Delete { repo, name } => delete(client, &repo, &name),
    }
}

fn list(client: &Client, repo: &str) -> Result<()> {
    let (owner, name) = git::parse_slug(repo)?;
    let labels: Vec<Value> = client.get(&format!("/repos/{owner}/{name}/labels"))?;
    for l in &labels {
        let label_name = l["name"].as_str().unwrap_or("?");
        let color = l["color"].as_str().unwrap_or("");
        let description = l["description"].as_str().unwrap_or("");
        println!(
            "{:<25} {:<8} {}",
            label_name.bold(),
            format!("#{color}").dimmed(),
            description.dimmed()
        );
    }
    Ok(())
}

fn create(client: &Client, repo: &str, name: &str, color: &str, description: &str) -> Result<()> {
    let (owner, repo_name) = git::parse_slug(repo)?;
    let body = json!({ "name": name, "color": color, "description": description });
    let _: Value = client.post(&format!("/repos/{owner}/{repo_name}/labels"), &body)?;
    println!("{} Created label {}", "✓".green().bold(), name.cyan());
    Ok(())
}

fn delete(client: &Client, repo: &str, name: &str) -> Result<()> {
    let (owner, repo_name) = git::parse_slug(repo)?;
    client.delete(&format!("/repos/{owner}/{repo_name}/labels/{name}"))?;
    println!("{} Deleted label {}", "✓".green().bold(), name.cyan());
    Ok(())
}
