use crate::api::Client;
use anyhow::{Context, Result};
use serde_json::Value;

/// Raw authenticated request against the GitHub API — an escape hatch for
/// endpoints that don't have a dedicated command yet.
pub fn run(client: &Client, method: String, path: String, body: Option<String>) -> Result<()> {
    let method = method
        .parse::<reqwest::Method>()
        .with_context(|| format!("invalid HTTP method: {method}"))?;

    let body = body
        .map(|b| serde_json::from_str::<Value>(&b))
        .transpose()
        .context("--body must be valid JSON")?;

    let (status, text) = client.raw_request(method, &path, body.as_ref())?;

    match serde_json::from_str::<Value>(&text) {
        Ok(v) => println!("{v:#}"),
        Err(_) => println!("{text}"),
    }

    if status >= 400 {
        anyhow::bail!("GitHub API error ({status})");
    }
    Ok(())
}
