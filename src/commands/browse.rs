use crate::api::Client;
use anyhow::Result;
use colored::Colorize;
use serde_json::Value;

/// Splits "owner/repo[/path/to/thing]" into (owner, repo, path). `path` is
/// "" when omitted, meaning the repo root.
fn parse_spec(spec: &str) -> Result<(String, String, String)> {
    let mut parts = spec.splitn(3, '/');
    let owner = parts.next().filter(|s| !s.is_empty());
    let name = parts.next().filter(|s| !s.is_empty());
    let path = parts.next().unwrap_or("").to_string();
    let (Some(owner), Some(name)) = (owner, name) else {
        anyhow::bail!("expected \"owner/repo[/path]\", got \"{spec}\"");
    };
    Ok((owner.to_string(), name.to_string(), path))
}

/// `ghx ls owner/repo[/path]` — browse a repo's file tree remotely via the
/// GitHub Contents API, styled like `ls -la`.
pub fn ls(client: &Client, spec: &str, all: bool) -> Result<()> {
    let (owner, name, path) = parse_spec(spec)?;
    let url = format!("/repos/{owner}/{name}/contents/{path}");
    let data: Value = client.get(&url)?;

    // Contents API returns an object for a file, an array for a directory.
    let entries: Vec<Value> = match data {
        Value::Array(a) => a,
        other => vec![other],
    };

    let mut entries: Vec<&Value> = entries.iter().collect();
    entries.sort_by_key(|e| {
        (
            e["type"].as_str() != Some("dir"),
            e["name"].as_str().unwrap_or("").to_lowercase(),
        )
    });

    for e in entries {
        let entry_name = e["name"].as_str().unwrap_or("?");
        if !all && entry_name.starts_with('.') {
            continue;
        }
        let is_dir = e["type"].as_str() == Some("dir");
        let type_ch = if is_dir { "d" } else { "-" };
        let size = e["size"].as_u64().unwrap_or(0);
        let entry_path = e["path"].as_str().unwrap_or(entry_name);

        let (author, date) = last_touched(client, &owner, &name, entry_path).unwrap_or_default();

        let colored_name = if is_dir {
            format!("{entry_name}/").blue().bold()
        } else {
            entry_name.normal()
        };

        println!(
            "{} {:>10}  {:<20} {:<16} {}",
            type_ch.truecolor(115, 218, 202),
            size,
            author.truecolor(224, 175, 104),
            date.truecolor(86, 95, 137),
            colored_name,
        );
    }
    Ok(())
}

/// Fetches the last commit that touched `path`, returning (author, short date).
fn last_touched(client: &Client, owner: &str, repo: &str, path: &str) -> Option<(String, String)> {
    let url = format!("/repos/{owner}/{repo}/commits?path={path}&per_page=1");
    let commits: Vec<Value> = client.get(&url).ok()?;
    let commit = commits.first()?;
    let author = commit["commit"]["author"]["name"]
        .as_str()
        .or_else(|| commit["author"]["login"].as_str())
        .unwrap_or("?")
        .to_string();
    let date = commit["commit"]["author"]["date"]
        .as_str()
        .unwrap_or("")
        .split('T')
        .next()
        .unwrap_or("")
        .to_string();
    Some((author, date))
}

/// `ghx cp owner/repo/path local-path` — download a single file from a repo
/// to local disk via the Contents API.
pub fn cp(client: &Client, spec: &str, local_path: &str) -> Result<()> {
    let (owner, name, path) = parse_spec(spec)?;
    if path.is_empty() {
        anyhow::bail!("expected \"owner/repo/path/to/file\", got \"{spec}\"");
    }
    let data: Value = client.get(&format!("/repos/{owner}/{name}/contents/{path}"))?;
    if data["type"].as_str() != Some("file") {
        anyhow::bail!("{spec} is not a file");
    }

    let bytes = if let Some(download_url) = data["download_url"].as_str() {
        client.get_bytes(download_url, "*/*")?
    } else {
        let content = data["content"].as_str().unwrap_or("");
        let cleaned: String = content.chars().filter(|c| !c.is_whitespace()).collect();
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(cleaned)?
    };

    std::fs::write(local_path, &bytes)?;
    println!(
        "{} Wrote {} ({} bytes)",
        "✓".green().bold(),
        local_path.bold(),
        bytes.len()
    );
    Ok(())
}

/// `ghx rm owner/repo/path -m "message" --yes` — delete a file from a repo
/// via the Contents API's DELETE endpoint, creating a real commit.
pub fn rm(client: &Client, spec: &str, message: &str, yes: bool) -> Result<()> {
    let (owner, name, path) = parse_spec(spec)?;
    if path.is_empty() {
        anyhow::bail!("expected \"owner/repo/path/to/file\", got \"{spec}\"");
    }

    let data: Value = client.get(&format!("/repos/{owner}/{name}/contents/{path}"))?;
    let sha = data["sha"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("could not resolve blob sha for {spec}"))?;

    if !yes {
        println!(
            "Would delete {} from {}/{} (sha {sha}). Re-run with --yes to actually delete it.",
            path.bold(),
            owner,
            name
        );
        return Ok(());
    }

    let body = serde_json::json!({
        "message": message,
        "sha": sha,
    });
    let _: Value = client.delete_json(&format!("/repos/{owner}/{name}/contents/{path}"), &body)?;
    println!("{} Deleted {} from {}/{}", "✓".green().bold(), path.bold(), owner, name);
    Ok(())
}
