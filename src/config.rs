use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Non-secret settings only. The auth token itself never touches disk in
/// plaintext — it lives in the OS credential store (Windows Credential
/// Manager / macOS Keychain / Linux Secret Service), keyed by username.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub username: Option<String>,
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
    /// store, only the (non-secret) username is written to disk.
    pub fn store_login(username: &str, token: &str) -> Result<()> {
        keyring_entry(username)?
            .set_password(token)
            .context("saving token to the OS credential store")?;
        Config {
            username: Some(username.to_string()),
        }
        .save()
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

    /// Resolve the token to use: env vars take precedence over the cached
    /// login, matching gh's own GH_TOKEN/GITHUB_TOKEN precedence
    /// convention. Falls back to the OS credential store entry for the
    /// last logged-in username.
    pub fn resolve_token() -> Result<Option<String>> {
        for var in ["GHX_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
            if let Ok(t) = std::env::var(var) {
                if !t.is_empty() {
                    return Ok(Some(t));
                }
            }
        }

        let Some(username) = Self::load()?.username else {
            return Ok(None);
        };
        match keyring_entry(&username)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("reading token from the OS credential store"),
        }
    }
}
