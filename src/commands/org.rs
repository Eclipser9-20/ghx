use crate::api::Client;
use anyhow::Result;
use colored::Colorize;
use serde_json::Value;

#[derive(clap::Subcommand)]
pub enum OrgCommand {
    /// List organizations for the authenticated user
    List,
    /// Show details about an organization
    View { org: String },
    /// List members of an organization
    Members { org: String },
    /// List teams in an organization
    Teams { org: String },
}

pub fn run(client: &Client, cmd: OrgCommand) -> Result<()> {
    match cmd {
        OrgCommand::List => list(client),
        OrgCommand::View { org } => view(client, &org),
        OrgCommand::Members { org } => members(client, &org),
        OrgCommand::Teams { org } => teams(client, &org),
    }
}

fn list(client: &Client) -> Result<()> {
    let orgs: Vec<Value> = client.get("/user/orgs")?;
    for org in &orgs {
        let login = org["login"].as_str().unwrap_or("?");
        let description = org["description"].as_str().unwrap_or("");
        println!("{:<25} {}", login.bold(), description.dimmed());
    }
    Ok(())
}

fn view(client: &Client, org: &str) -> Result<()> {
    let data: Value = client.get(&format!("/orgs/{org}"))?;
    let login = data["login"].as_str().unwrap_or("?");
    let name = data["name"].as_str().unwrap_or("");
    let description = data["description"].as_str().unwrap_or("");
    let public_repos = data["public_repos"].as_u64().unwrap_or(0);
    let followers = data["followers"].as_u64().unwrap_or(0);
    let url = data["html_url"].as_str().unwrap_or("");

    println!("{}", login.bold());
    if !name.is_empty() {
        println!("{name}");
    }
    if !description.is_empty() {
        println!("{description}");
    }
    println!();
    println!(
        "{} public repos   {} followers",
        public_repos,
        followers,
    );
    println!("{}", url.underline());
    Ok(())
}

fn members(client: &Client, org: &str) -> Result<()> {
    let members: Vec<Value> = client.get(&format!("/orgs/{org}/members"))?;
    for m in &members {
        let login = m["login"].as_str().unwrap_or("?");
        println!("{login}");
    }
    Ok(())
}

fn teams(client: &Client, org: &str) -> Result<()> {
    let teams: Vec<Value> = client.get(&format!("/orgs/{org}/teams"))?;
    for t in &teams {
        let name = t["name"].as_str().unwrap_or("?");
        let slug = t["slug"].as_str().unwrap_or("?");
        let privacy = t["privacy"].as_str().unwrap_or("?");
        println!("{:<25} {:<25} {}", name.bold(), slug.dimmed(), privacy.cyan());
    }
    Ok(())
}
