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

fn parse_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&chrono::Utc))
}

fn format_duration(start: &str, end: Option<&str>) -> String {
    let Some(start) = parse_time(start) else {
        return "?".to_string();
    };
    let end = end.and_then(parse_time).unwrap_or_else(chrono::Utc::now);
    let secs = (end - start).num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m{}s", secs / 60, secs % 60)
    }
}

fn view(client: &Client, repo: Option<String>, run_id: u64) -> Result<()> {
    let (owner, name) = resolve(repo)?;
    let run: Value = client.get(&format!("/repos/{owner}/{name}/actions/runs/{run_id}"))?;

    let title = run["name"].as_str().unwrap_or("?");
    let status = run["status"].as_str().unwrap_or("?");
    let conclusion = run["conclusion"].as_str();
    let url = run["html_url"].as_str().unwrap_or("");
    let event = run["event"].as_str().unwrap_or("?");
    let branch = run["head_branch"].as_str().unwrap_or("?");
    let actor = run["triggering_actor"]["login"]
        .as_str()
        .or_else(|| run["actor"]["login"].as_str())
        .unwrap_or("?");
    let sha = run["head_sha"].as_str().unwrap_or("");
    let short_sha = &sha[..sha.len().min(7)];
    let commit_msg = run["head_commit"]["message"]
        .as_str()
        .and_then(|m| m.lines().next())
        .unwrap_or("");
    let run_number = run["run_number"].as_u64().unwrap_or(0);
    let run_attempt = run["run_attempt"].as_u64().unwrap_or(1);
    let created_at = run["created_at"].as_str().unwrap_or("");
    let updated_at = run["updated_at"].as_str();

    println!(
        "{}  {}  {}",
        title.bold(),
        status_tag(status, conclusion),
        format!("run #{run_number} (attempt {run_attempt})").dimmed()
    );
    println!(
        "{} on {}  by {}  {}",
        event.cyan(),
        branch.cyan(),
        actor,
        format!("{short_sha} {commit_msg}").dimmed()
    );
    if !created_at.is_empty() {
        println!("duration: {}", format_duration(created_at, updated_at));
    }
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
        let jstarted = j["started_at"].as_str();
        let jduration = jstarted
            .map(|s| format_duration(s, j["completed_at"].as_str()))
            .unwrap_or_else(|| "?".to_string());

        println!(
            "{} {}  {}  {}",
            format!("#{jid}").dimmed(),
            status_tag(jstatus, jconclusion),
            jname.bold(),
            format!("({jduration})").dimmed()
        );

        for step in j["steps"].as_array().cloned().unwrap_or_default() {
            let step_name = step["name"].as_str().unwrap_or("?");
            let step_status = step["status"].as_str().unwrap_or("?");
            let step_conclusion = step["conclusion"].as_str();
            let step_number = step["number"].as_u64().unwrap_or(0);
            println!(
                "    {} {} {}",
                format!("{step_number}.").dimmed(),
                status_tag(step_status, step_conclusion),
                step_name
            );
        }
    }
    Ok(())
}

fn logs(client: &Client, repo: Option<String>, run_id: u64, job: Option<u64>) -> Result<()> {
    let (owner, name) = resolve(repo)?;

    // Fetch job status alongside id/name so we can skip logs for jobs that
    // haven't finished yet — GitHub's logs endpoint 404s with a raw
    // "BlobNotFound" XML error for a job whose log archive doesn't exist
    // yet, which reads as a crash rather than "come back once it's done".
    let all_jobs: Value = client.get(&format!(
        "/repos/{owner}/{name}/actions/runs/{run_id}/jobs"
    ))?;
    let jobs_arr = all_jobs["jobs"].as_array().cloned().unwrap_or_default();

    let targets: Vec<Value> = match job {
        Some(j) => jobs_arr.into_iter().filter(|v| v["id"].as_u64() == Some(j)).collect(),
        None => jobs_arr,
    };

    if targets.is_empty() {
        println!("{} no matching job found for run #{run_id}", "!".yellow());
        return Ok(());
    }

    for j in targets {
        let jid = j["id"].as_u64().unwrap_or(0);
        let jname = j["name"].as_str().unwrap_or("?");
        let jstatus = j["status"].as_str().unwrap_or("?");

        if jstatus != "completed" {
            println!(
                "{} job {jid} ({jname}) is still {} — logs aren't available until it finishes",
                "…".yellow(),
                jstatus.cyan()
            );
            continue;
        }

        let text = client
            .get_raw(
                &format!("/repos/{owner}/{name}/actions/jobs/{jid}/logs"),
                "application/vnd.github+json",
            )
            .with_context(|| format!("fetching logs for job {jid}"))?;
        println!("{}", format!("── job {jid} ({jname}) ──").dimmed());
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
