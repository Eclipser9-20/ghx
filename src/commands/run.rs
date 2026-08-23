use crate::api::Client;
use crate::git::GhRepo;
use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::Value;

#[derive(clap::Subcommand)]
pub enum RunCommand {
    /// List recent workflow runs
    List {
        #[arg(short = 'R', long)]
        repo: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// View a run's status and jobs
    View {
        run_id: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
    },
    /// Print the log for a job (or every job in a run, if --job is omitted)
    Logs {
        run_id: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
        /// A specific job id (defaults to all jobs in the run)
        #[arg(long)]
        job: Option<u64>,
    },
    /// Re-run a workflow run
    Rerun {
        run_id: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
        /// Only re-run failed jobs
        #[arg(long)]
        failed_only: bool,
    },
    /// Cancel a workflow run
    Cancel {
        run_id: u64,
        #[arg(short = 'R', long)]
        repo: Option<String>,
    },
}

fn resolve(repo: Option<String>) -> Result<(String, String)> {
    match repo {
        Some(slug) => crate::git::parse_slug(&slug),
        None => {
            let r = GhRepo::detect()?;
            Ok((r.owner, r.name))
        }
    }
}

pub fn run(client: &Client, cmd: RunCommand) -> Result<()> {
    match cmd {
        RunCommand::List { repo, limit } => list(client, repo, limit),
        RunCommand::View { run_id, repo } => view(client, repo, run_id),
        RunCommand::Logs { run_id, repo, job } => logs(client, repo, run_id, job),
        RunCommand::Rerun {
            run_id,
            repo,
            failed_only,
        } => rerun(client, repo, run_id, failed_only),
        RunCommand::Cancel { run_id, repo } => cancel(client, repo, run_id),
    }
}

fn status_tag(status: &str, conclusion: Option<&str>) -> colored::ColoredString {
    match conclusion {
        Some("success") => "success".green(),
        Some("failure") => "failure".red(),
        Some("cancelled") => "cancelled".yellow(),
        Some(other) => other.normal(),
        None => status.cyan(),
    }
}

fn list(client: &Client, repo: Option<String>, limit: u32) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let data: Value = client.get(&format!(
        "/repos/{owner}/{name}/actions/runs?per_page={}",
        limit.min(100)
    ))?;
    let runs = data["workflow_runs"].as_array().cloned().unwrap_or_default();
    for r in runs.iter().take(limit as usize) {
        let id = r["id"].as_u64().unwrap_or(0);
        let name = r["name"].as_str().unwrap_or("?");
        let status = r["status"].as_str().unwrap_or("?");
        let conclusion = r["conclusion"].as_str();
        let branch = r["head_branch"].as_str().unwrap_or("?");
        println!(
            "{} {:<12} {}  {}",
            format!("#{id}").dimmed(),
            status_tag(status, conclusion),
            name,
            branch.cyan()
        );
    }
    Ok(())
}

fn view(client: &Client, repo: Option<String>, run_id: u64) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let run: Value = client.get(&format!("/repos/{owner}/{name}/actions/runs/{run_id}"))?;
    let title = run["name"].as_str().unwrap_or("?");
    let status = run["status"].as_str().unwrap_or("?");
    let conclusion = run["conclusion"].as_str();
    let url = run["html_url"].as_str().unwrap_or("");

    println!("{}  {}", title.bold(), status_tag(status, conclusion));
    println!("{}", url.underline());
    println!();

    let jobs: Value = client.get(&format!(
        "/repos/{owner}/{name}/actions/runs/{run_id}/jobs"
    ))?;
    for j in jobs["jobs"].as_array().cloned().unwrap_or_default() {
        let jid = j["id"].as_u64().unwrap_or(0);
        let jname = j["name"].as_str().unwrap_or("?");
        let jstatus = j["status"].as_str().unwrap_or("?");
        let jconclusion = j["conclusion"].as_str();
        println!(
            "{} {}  {}",
            format!("#{jid}").dimmed(),
            status_tag(jstatus, jconclusion),
            jname
        );
    }
    Ok(())
}

fn logs(client: &Client, repo: Option<String>, run_id: u64, job: Option<u64>) -> Result<()> {
    let (owner, name) = resolve(repo)?;

    let job_ids: Vec<u64> = match job {
        Some(j) => vec![j],
        None => {
            let jobs: Value = client.get(&format!(
                "/repos/{owner}/{name}/actions/runs/{run_id}/jobs"
            ))?;
            jobs["jobs"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|j| j["id"].as_u64())
                .collect()
        }
    };

    for jid in job_ids {
        let text = client
            .get_raw(
                &format!("/repos/{owner}/{name}/actions/jobs/{jid}/logs"),
                "application/vnd.github+json",
            )
            .with_context(|| format!("fetching logs for job {jid}"))?;
        println!("{}", format!("── job {jid} ──").dimmed());
        println!("{text}");
    }
    Ok(())
}

fn rerun(client: &Client, repo: Option<String>, run_id: u64, failed_only: bool) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let path = if failed_only {
        format!("/repos/{owner}/{name}/actions/runs/{run_id}/rerun-failed-jobs")
    } else {
        format!("/repos/{owner}/{name}/actions/runs/{run_id}/rerun")
    };
    client.post_empty(&path)?;
    println!("{} Re-running run #{run_id}", "✓".green().bold());
    Ok(())
}

fn cancel(client: &Client, repo: Option<String>, run_id: u64) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    client.post_empty(&format!(
        "/repos/{owner}/{name}/actions/runs/{run_id}/cancel"
    ))?;
    println!("{} Cancelled run #{run_id}", "✓".green().bold());
    Ok(())
}
