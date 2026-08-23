use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub token: Option<String>,
    pub username: Option<String>,
}

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("could not determine config directory")?
        .join("ghx");
    Ok(dir.join("config.json"))
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

    /// Resolve the token to use: env var takes precedence over stored config,
    /// matching gh's own GH_TOKEN/GITHUB_TOKEN precedence convention.
    pub fn resolve_token() -> Result<Option<String>> {
        if let Ok(t) = std::env::var("GHX_TOKEN") {
            if !t.is_empty() {
                return Ok(Some(t));
            }
        }
        if let Ok(t) = std::env::var("GH_TOKEN") {
            if !t.is_empty() {
                return Ok(Some(t));
            }
        }
        if let Ok(t) = std::env::var("GITHUB_TOKEN") {
            if !t.is_empty() {
                return Ok(Some(t));
            }
        }
        Ok(Self::load()?.token)
    }
}
