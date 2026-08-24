use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;

const API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("ghx/", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
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
        self.request_accept(method, path, "application/vnd.github+json")
    }

    /// Like `request`, but with a caller-chosen Accept header instead of
    /// the default `application/vnd.github+json` (reqwest's `.header()`
    /// appends rather than replaces, so setting Accept twice would send
    /// two values — this builds it fresh instead).
    fn request_accept(
        &self,
        method: reqwest::Method,
        path: &str,
        accept: &str,
    ) -> reqwest::blocking::RequestBuilder {
        let url = if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{API_BASE}{path}")
        };
        let mut req = self
            .http
            .request(method, url)
            .header("Accept", accept)
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        req
    }

    /// GET with a custom Accept header, returning the raw response body as
    /// bytes. Used for binary downloads (release assets).
    pub fn get_bytes(&self, path: &str, accept: &str) -> Result<Vec<u8>> {
        let resp = self
            .request_accept(reqwest::Method::GET, path, accept)
            .send()
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            bail!("GitHub API error ({status}) fetching {path}");
        }
        Ok(resp.bytes().context("reading response body")?.to_vec())
    }

    /// GET with a custom Accept header, returning the raw response body as
    /// text. Used for endpoints that don't return JSON (diffs, Action
    /// job logs).
    pub fn get_raw(&self, path: &str, accept: &str) -> Result<String> {
        let resp = self
            .request_accept(reqwest::Method::GET, path, accept)
            .send()
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        let text = resp.text().context("reading response body")?;
        if !status.is_success() {
            bail!("GitHub API error ({status}): {text}");
        }
        Ok(text)
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .request(reqwest::Method::GET, path)
            .send()
            .with_context(|| format!("GET {path}"))?;
        Self::handle(resp)
    }

    /// GET returning the raw status code alongside the parsed JSON body (or
    /// `Value::Null` when the body is empty). Used where a non-2xx status
    /// (e.g. 404) is a meaningful result rather than an error, such as
    /// checking branch protection on an unprotected branch.
    pub fn get_status(&self, path: &str) -> Result<(u16, Value)> {
        let resp = self
            .request(reqwest::Method::GET, path)
            .send()
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status().as_u16();
        let text = resp.text().context("reading response body")?;
        let value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("parsing GitHub API response: {text}"))?
        };
        Ok((status, value))
    }

    /// GET returning the parsed JSON body along with response headers, so
    /// callers can inspect things like `X-Poll-Interval`.
    pub fn get_with_headers<T: DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<(T, reqwest::header::HeaderMap)> {
        let resp = self
            .request(reqwest::Method::GET, path)
            .send()
            .with_context(|| format!("GET {path}"))?;
        let headers = resp.headers().clone();
        let value = Self::handle(resp)?;
        Ok((value, headers))
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

    /// POST with no request body, discarding the response body — for
    /// action-trigger endpoints (rerun, cancel, etc.) that return 201/204.
    pub fn post_empty(&self, path: &str) -> Result<()> {
        let resp = self
            .request(reqwest::Method::POST, path)
            .send()
            .with_context(|| format!("POST {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("POST {path} failed: {status}: {text}");
        }
        Ok(())
    }

    /// POST with a JSON request body, discarding the response body — for
    /// trigger endpoints (workflow dispatch, etc.) that return 204.
    pub fn post_json_empty(&self, path: &str, body: &Value) -> Result<()> {
        let resp = self
            .request(reqwest::Method::POST, path)
            .json(body)
            .send()
            .with_context(|| format!("POST {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("POST {path} failed: {status}: {text}");
        }
        Ok(())
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

    /// DELETE with a JSON request body, returning the parsed response body.
    /// Used by the Contents API's file-delete endpoint, which requires the
    /// blob sha and commit message in the request body.
    pub fn delete_json<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let resp = self
            .request(reqwest::Method::DELETE, path)
            .json(body)
            .send()
            .with_context(|| format!("DELETE {path}"))?;
        Self::handle(resp)
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

    /// Arbitrary request against the API, for `ghx api`. Returns the
    /// status code and raw response body regardless of success/failure —
    /// callers decide what to do with an error status themselves.
    pub fn raw_request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, String)> {
        let mut req = self.request(method.clone(), path);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().with_context(|| format!("{method} {path}"))?;
        let status = resp.status().as_u16();
        let text = resp.text().context("reading response body")?;
        Ok((status, text))
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
