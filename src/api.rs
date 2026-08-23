use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;

const API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("ghx/", env!("CARGO_PKG_VERSION"));

pub struct Client {
    http: reqwest::blocking::Client,
    token: Option<String>,
}

impl Client {
    pub fn new(token: Option<String>) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("building HTTP client")?;
        Ok(Self { http, token })
    }

    pub fn require_token(&self) -> Result<&str> {
        self.token
            .as_deref()
            .context("not authenticated — run `ghx auth login` first")
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::blocking::RequestBuilder {
        let url = if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{API_BASE}{path}")
        };
        let mut req = self
            .http
            .request(method, url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        req
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .request(reqwest::Method::GET, path)
            .send()
            .with_context(|| format!("GET {path}"))?;
        Self::handle(resp)
    }

    pub fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let resp = self
            .request(reqwest::Method::POST, path)
            .json(body)
            .send()
            .with_context(|| format!("POST {path}"))?;
        Self::handle(resp)
    }

    pub fn patch<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let resp = self
            .request(reqwest::Method::PATCH, path)
            .json(body)
            .send()
            .with_context(|| format!("PATCH {path}"))?;
        Self::handle(resp)
    }

    pub fn put_json<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let resp = self
            .request(reqwest::Method::PUT, path)
            .json(body)
            .send()
            .with_context(|| format!("PUT {path}"))?;
        Self::handle(resp)
    }

    pub fn put(&self, path: &str, body: &Value) -> Result<()> {
        let resp = self
            .request(reqwest::Method::PUT, path)
            .json(body)
            .send()
            .with_context(|| format!("PUT {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("PUT {path} failed: {status}: {text}");
        }
        Ok(())
    }

    pub fn delete(&self, path: &str) -> Result<()> {
        let resp = self
            .request(reqwest::Method::DELETE, path)
            .send()
            .with_context(|| format!("DELETE {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("DELETE {path} failed: {status}: {text}");
        }
        Ok(())
    }

    fn handle<T: DeserializeOwned>(resp: reqwest::blocking::Response) -> Result<T> {
        let status = resp.status();
        let text = resp.text().context("reading response body")?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                .unwrap_or_else(|| text.clone());
            bail!("GitHub API error ({status}): {message}");
        }
        serde_json::from_str(&text)
            .with_context(|| format!("parsing GitHub API response: {text}"))
    }
}
