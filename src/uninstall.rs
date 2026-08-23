//! `ghx --uninstall`: removes stored credentials/config and the install
//! tree created by install.sh/install.ps1 (or just the binary itself, if
//! it wasn't installed via those layouts — e.g. a plain `cargo build`).

use anyhow::{Context, Result};
use std::path::Path;

pub fn run() -> Result<()> {
    if let Err(e) = crate::config::Config::clear_login() {
        eprintln!("warning: could not clear stored credentials: {e}");
    }

    let exe = std::env::current_exe().context("locating the running executable")?;

    // Standard install layout: <LOCAL>/ghx/bin/ghx[.exe], with a PATH
    // symlink at <LOCAL>/bin/ghx (Unix only — Windows adds the real bin
    // dir straight to PATH, no symlink).
    let bin_dir = exe.parent();
    let ghx_dir = bin_dir.and_then(Path::parent);
    let is_standard_layout = bin_dir.is_some_and(|d| d.file_name().is_some_and(|n| n == "bin"))
        && ghx_dir.is_some_and(|d| d.file_name().is_some_and(|n| n == "ghx"));

    if is_standard_layout {
        let ghx_dir = ghx_dir.unwrap();
        remove_running_exe(&exe)?;
        for entry in std::fs::read_dir(ghx_dir).with_context(|| format!("reading {}", ghx_dir.display()))? {
            let path = entry?.path();
            if path == exe {
                continue;
            }
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
        let _ = std::fs::remove_dir(exe.parent().unwrap());
        let _ = std::fs::remove_dir(ghx_dir);

        #[cfg(unix)]
        if let Some(local) = ghx_dir.parent() {
            let _ = std::fs::remove_file(local.join("bin").join("ghx"));
        }

        println!("Removed {}", ghx_dir.display());
    } else {
        remove_running_exe(&exe)?;
        println!("Removed {}", exe.display());
    }

    println!("ghx has been uninstalled.");
    Ok(())
}

#[cfg(unix)]
fn remove_running_exe(exe: &Path) -> Result<()> {
    std::fs::remove_file(exe).with_context(|| format!("removing {}", exe.display()))
}

/// Windows won't let a running process delete its own executable image
/// directly (sharing violation) — rename it aside first (allowed while
/// running), then delete the renamed copy, which usually succeeds
/// immediately; if the OS still has it locked, it's a single small
/// leftover file that gets cleaned up whenever it's next writable.
#[cfg(windows)]
fn remove_running_exe(exe: &Path) -> Result<()> {
    let renamed = exe.with_extension("exe.removing");
    let _ = std::fs::remove_file(&renamed);
    std::fs::rename(exe, &renamed).context("renaming the running binary aside")?;
    let _ = std::fs::remove_file(&renamed);
    Ok(())
}
