use crate::git::GhRepo;
use crate::lfs;
use anyhow::Result;
use colored::Colorize;

#[derive(clap::Subcommand)]
pub enum LfsCommand {
    /// Configure the current repo to use LFS
    Install,
    /// Track a pattern (e.g. `*.psd`) as an LFS file in .gitattributes
    Track { pattern: String },
    /// Stop tracking a pattern as an LFS file
    Untrack { pattern: String },
    /// Show which working-tree files are LFS pointers, and cache state
    Status,
    /// Upload LFS objects for pointer files reachable from HEAD
    Push,
    /// Download LFS objects for pointer files in the working tree
    Pull,
}

pub fn run(cmd: LfsCommand) -> Result<()> {
    match cmd {
        LfsCommand::Install => {
            lfs::install()?;
            println!(
                "{} LFS enabled for this repo (.gitattributes ready — use `ghx lfs track <pattern>`)",
                "✓".green().bold()
            );
            Ok(())
        }
        LfsCommand::Track { pattern } => {
            lfs::track(&pattern)?;
            println!("{} Tracking {}", "✓".green().bold(), pattern.cyan());
            Ok(())
        }
        LfsCommand::Untrack { pattern } => {
            lfs::untrack(&pattern)?;
            println!("{} Untracked {}", "✓".green().bold(), pattern.cyan());
            Ok(())
        }
        LfsCommand::Status => {
            let entries = lfs::status()?;
            if entries.is_empty() {
                println!("{}", "no LFS pointer files in the working tree".dimmed());
                return Ok(());
            }
            for e in entries {
                let (tag, note) = match e.state {
                    lfs::LfsFileState::PointerCached => ("cached".green(), ""),
                    lfs::LfsFileState::PointerMissing => {
                        ("missing".yellow(), " (run `ghx lfs pull`)")
                    }
                };
                println!(
                    "{:<10} {}  {} ({} bytes){}",
                    tag,
                    e.path.display(),
                    e.oid[..12.min(e.oid.len())].dimmed(),
                    e.size,
                    note.dimmed()
                );
            }
            Ok(())
        }
        LfsCommand::Push => {
            let repo = GhRepo::detect()?;
            let n = lfs::push_objects(&repo)?;
            println!("{} Uploaded {n} LFS object(s)", "✓".green().bold());
            Ok(())
        }
        LfsCommand::Pull => {
            let repo = GhRepo::detect()?;
            let n = lfs::pull_objects(&repo)?;
            println!("{} Hydrated {n} LFS object(s)", "✓".green().bold());
            Ok(())
        }
    }
}
