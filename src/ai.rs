//! Commit message generation from a diff, via whichever AI backend is
//! configured: Anthropic, OpenAI-compatible, or OpenRouter (free models).

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

const SYSTEM_PROMPT: &str = "You write concise, conventional git commit messages. Given a diff, output ONLY the commit message: a short imperative summary line under 72 characters, optionally followed by a blank line and a brief body explaining why. No markdown, no quotes, no preamble, no trailing explanation.";

const MAX_DIFF_LEN: usize = 12_000;

pub fn generate_commit_message(diff: &str) -> Result<String> {
    if diff.trim().is_empty() {
        bail!("no staged changes to describe");
    }
    let diff = if diff.len() > MAX_DIFF_LEN {
        &diff[..MAX_DIFF_LEN]
    } else {
        diff
    };

    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            return call_anthropic(&key, diff);
        }
    }
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            return call_openai(&key, diff);
        }
    }
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() {
            return call_openrouter(&key, diff);
        }
    }
    bail!(
        "no AI backend configured — set one of ANTHROPIC_API_KEY, OPENAI_API_KEY, \
         or OPENROUTER_API_KEY (openrouter.ai has free models and a free API key, \
         no local install needed)"
    )
}

fn call_anthropic(key: &str, diff: &str) -> Result<String> {
    let model =
        std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-haiku-latest".into());
    let http = reqwest::blocking::Client::new();
    let resp: Value = http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": model,
            "max_tokens": 300,
            "system": SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": format!("Diff:\n{diff}")}]
        }))
        .send()
        .context("calling the Anthropic API")?
        .json()
        .context("parsing the Anthropic API response")?;

    resp["content"][0]["text"]
        .as_str()
        .map(|s| s.trim().to_string())
        .with_context(|| format!("unexpected response from Anthropic: {resp}"))
}

fn call_openai(key: &str, diff: &str) -> Result<String> {
    let base =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    let http = reqwest::blocking::Client::new();
    let resp: Value = http
        .post(format!("{base}/chat/completions"))
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": format!("Diff:\n{diff}")}
            ]
        }))
        .send()
        .context("calling the OpenAI-compatible API")?
        .json()
        .context("parsing the OpenAI-compatible API response")?;

    resp["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .with_context(|| format!("unexpected response from OpenAI-compatible API: {resp}"))
}

/// OpenRouter is OpenAI-compatible and hosts several genuinely free
/// models — no local install, just a free API key.
fn call_openrouter(key: &str, diff: &str) -> Result<String> {
    let model =
        std::env::var("OPENROUTER_MODEL").unwrap_or_else(|_| "meta-llama/llama-3.1-8b-instruct:free".into());
    let http = reqwest::blocking::Client::new();
    let resp: Value = http
        .post("https://openrouter.ai/api/v1/chat/completions")
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": format!("Diff:\n{diff}")}
            ]
        }))
        .send()
        .context("calling OpenRouter")?
        .json()
        .context("parsing the OpenRouter response")?;

    resp["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .with_context(|| format!("unexpected response from OpenRouter: {resp}"))
}
