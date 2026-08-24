use crate::api::Client;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::Write;

const REPO_OWNER: &str = "Eclipser9-20";
const REPO_NAME: &str = "ghx";

/// The release asset name for the platform this binary was built for.
fn asset_name() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "ghx-windows-x86_64.exe",
        ("linux", "x86_64") => "ghx-linux-x86_64",
        ("macos", "aarch64") => "ghx-macos-aarch64",
        ("macos", "x86_64") => "ghx-macos-x86_64",
        (os, arch) => bail!("no release asset published for {os}/{arch}"),
    })
}

fn find_release(client: &Client, channel: &str) -> Result<Value> {
    match channel {
        "stable" => client.get(&format!("/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest")),
        "dev" => client.get(&format!(
            "/repos/{REPO_OWNER}/{REPO_NAME}/releases/tags/dev"
        )),
        "beta" => {
            let releases: Vec<Value> =
                client.get(&format!("/repos/{REPO_OWNER}/{REPO_NAME}/releases"))?;
            releases
                .into_iter()
                .find(|r| {
                    r["tag_name"]
                        .as_str()
                        .is_some_and(|t| t.contains("-beta."))
                })
                .context("no beta release found")
        }
        other => bail!("unknown channel '{other}' (expected stable, beta, or dev)"),
    }
}

/// Best-effort, idempotent: creates the shared maintenance group if it
/// doesn't already exist, so a system-wide install stays updatable by
/// anyone added to that group even if the installer script was never run
/// again after the group convention was introduced. Silently does nothing
/// when the caller lacks permission to create groups (e.g. a per-user
/// install, or an unprivileged account) — that's expected, not an error.
fn ensure_maintenance_group() {
    const GROUP_NAME: &str = "_GHXmaintenance";

    #[cfg(unix)]
    {
        let exists = std::process::Command::new("getent")
            .args(["group", GROUP_NAME])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !exists {
            if cfg!(target_os = "macos") {
                let _ = std::process::Command::new("dseditgroup")
                    .args(["-o", "create", GROUP_NAME])
                    .status();
            } else {
                let _ = std::process::Command::new("groupadd")
                    .arg(GROUP_NAME)
                    .status();
            }
        }
    }

    #[cfg(windows)]
    {
        let exists = std::process::Command::new("net")
            .args(["localgroup", GROUP_NAME])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !exists {
            let _ = std::process::Command::new("net")
                .args(["localgroup", GROUP_NAME, "/add"])
                .status();
        }
    }
}

pub fn run(client: &Client, channel: &str) -> Result<()> {
    if std::env::var("GHX_UPDATE_TRACE").is_ok() {
        eprintln!("[trace] before ensure_maintenance_group");
    }
    ensure_maintenance_group();
    if std::env::var("GHX_UPDATE_TRACE").is_ok() {
        eprintln!("[trace] after ensure_maintenance_group, before find_release");
    }
    let channel = channel.to_ascii_lowercase();
    let release = find_release(client, &channel)?;
    if std::env::var("GHX_UPDATE_TRACE").is_ok() {
        eprintln!("[trace] after find_release");
    }
    let tag = release["tag_name"].as_str().unwrap_or("?");

    if tag == format!("v{}", env!("CARGO_PKG_VERSION")) {
        println!("Already up to date (v{}).", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let asset_name = asset_name()?;
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(asset_name))
        .with_context(|| format!("release {tag} has no asset named {asset_name}"))?;

    let download_url = asset["url"]
        .as_str()
        .context("release asset has no API url")?;

    println!("Downloading {tag} ({asset_name})...");
    if std::env::var("GHX_UPDATE_TRACE").is_ok() {
        eprintln!("[trace] before get_bytes");
    }
    let bytes = client.get_bytes(download_url, "application/octet-stream")?;
    if std::env::var("GHX_UPDATE_TRACE").is_ok() {
        eprintln!("[trace] after get_bytes, before file write");
    }

    let current_exe = std::env::current_exe().context("locating the running executable")?;
    let dir = current_exe
        .parent()
        .context("executable has no parent directory")?;
    let staged = dir.join(format!(
        "{}.new",
        current_exe.file_name().unwrap().to_string_lossy()
    ));
    let backup = dir.join(format!(
        "{}.old",
        current_exe.file_name().unwrap().to_string_lossy()
    ));

    {
        let mut f = std::fs::File::create(&staged).context("writing downloaded binary")?;
        f.write_all(&bytes)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    }

    // Replace in place: rename the running binary aside (allowed even while
    // it's executing), then move the new one into its place. Best-effort
    // cleanup of the old one — some platforms won't allow deleting a file
    // that's still mapped by a running process, which is fine; it's an
    // orphaned few-MB file, not a correctness issue.
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(&current_exe, &backup).context("moving the current binary aside")?;
    std::fs::rename(&staged, &current_exe).context("installing the new binary")?;
    let _ = std::fs::remove_file(&backup);

    println!(
        "Updated to {tag} ({channel} channel). The new version will be used next time you run ghx."
    );
    Ok(())
}
