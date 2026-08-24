use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Non-secret settings. The auth token normally lives in the OS credential
/// store (Windows Credential Manager / macOS Keychain / Linux Secret
/// Service), keyed by username — `token_fallback` is only populated when
/// that store isn't reachable (e.g. a headless Linux box with no D-Bus
/// session), so login still works rather than hard-failing.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_fallback: Option<String>,
}

const KEYRING_SERVICE: &str = "ghx";

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("could not determine config directory")?
        .join("ghx");
    Ok(dir.join("config.json"))
}

fn keyring_entry(username: &str) -> Result<Entry> {
    Entry::new(KEYRING_SERVICE, username).context("opening OS credential store")
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(&path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let cfg: Config = serde_json::from_str(&data).context("parsing config")?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating config dir {}", parent.display()))?;
        }
        let data = serde_json::to_string_pretty(self)?;
        fs::write(&path, data).with_context(|| format!("writing config at {}", path.display()))?;
        Ok(())
    }

    /// Save a successful login: the token goes into the OS credential
    /// store when one is reachable, otherwise it's kept in the plaintext
    /// config as a fallback (with a warning) so login still succeeds on
    /// systems with no secret-service backend.
    pub fn store_login(username: &str, token: &str) -> Result<()> {
        match keyring_entry(username)?.set_password(token) {
            Ok(()) => Config {
                username: Some(username.to_string()),
                token_fallback: None,
            }
            .save(),
            Err(e) => {
                eprintln!(
                    "warning: could not use the OS credential store ({e}); \
                     storing the token in plaintext at the ghx config file instead"
                );
                Config {
                    username: Some(username.to_string()),
                    token_fallback: Some(token.to_string()),
                }
                .save()
            }
        }
    }

    /// Forget the current login: removes the cached credential and clears
    /// the stored username.
    pub fn clear_login() -> Result<()> {
        if let Some(username) = Self::load()?.username {
            match keyring_entry(&username)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e).context("removing token from the OS credential store"),
            }
        }
        Config::default().save()
    }

    /// Env vars and the plaintext config fallback only — no OS credential
    /// store access. Some macOS Security framework calls print a native
    /// diagnostic straight to stderr as a side effect of the Keychain
    /// lookup itself, before this crate's Result even comes back, so
    /// callers that can do without a cached login (anything that treats
    /// the token as a nice-to-have, not a requirement) should use this
    /// instead of `resolve_token` to avoid touching the Keychain at all.
    pub fn resolve_token_no_keychain() -> Result<Option<String>> {
        for var in ["GHX_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
            if let Ok(t) = std::env::var(var) {
                if !t.is_empty() {
                    return Ok(Some(t));
                }
            }
        }
        Ok(Self::load()?.token_fallback)
    }

    /// Resolve the token to use: env vars take precedence over the cached
    /// login, matching gh's own GH_TOKEN/GITHUB_TOKEN precedence
    /// convention. Falls back to the OS credential store entry for the
    /// last logged-in username, or the plaintext fallback if that's what
    /// login had to use.
    pub fn resolve_token() -> Result<Option<String>> {
        for var in ["GHX_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
            if let Ok(t) = std::env::var(var) {
                if !t.is_empty() {
                    return Ok(Some(t));
                }
            }
        }

        let cfg = Self::load()?;
        if let Some(token) = cfg.token_fallback {
            return Ok(Some(token));
        }
        let Some(username) = cfg.username else {
            return Ok(None);
        };
        match keyring_entry(&username)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("reading token from the OS credential store"),
        }
    }
}
