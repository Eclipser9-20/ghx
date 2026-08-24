mod ai;
mod api;
mod commands;
mod config;
mod filter;
mod git;
mod highlight;
mod lfs;
mod palette;
mod tui;
mod uninstall;
mod update;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{
    ai as aicmd, apicmd, auth, browse, filter as filtercmd, git as gitcmd, grep as grepcmd, issue, label,
    lfs as lfscmd, notifications, oops as oopscmd, org, pr, prune as prunecmd, repo,
    run as runcmd, save as savecmd, tree as treecmd, webhook,
};

#[derive(Parser)]
#[command(name = "ghx", version, about = "A GitHub CLI, in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Update ghx: stable, beta, dev, or an exact release tag to roll back/forward to
    #[arg(long, value_name = "CHANNEL")]
    update: Option<String>,

    /// Allow --update to switch to a less-tested channel than what's installed
    #[arg(long)]
    yes: bool,

    /// Remove ghx (the installed binary, and credentials/config it stored)
    #[arg(long)]
    uninstall: bool,
}

#[derive(Subcommand)]
enum Command {
    // ---- GitHub -----------------------------------------------------
    /// Authenticate with GitHub
    Auth {
        #[command(subcommand)]
        cmd: auth::AuthCommand,
    },
    /// Work with repositories
    Repo {
        #[command(subcommand)]
        cmd: repo::RepoCommand,
    },
    /// Work with pull requests
    Pr {
        #[command(subcommand)]
        cmd: pr::PrCommand,
    },
    /// Work with issues
    Issue {
        #[command(subcommand)]
        cmd: issue::IssueCommand,
    },
    /// Work with GitHub Actions workflow runs
    Run {
        #[command(subcommand)]
        cmd: runcmd::RunCommand,
    },
    /// Work with organizations
    Org {
        #[command(subcommand)]
        cmd: org::OrgCommand,
    },
    /// Work with repository webhooks
    Webhook {
        #[command(subcommand)]
        cmd: webhook::WebhookCommand,
    },
    /// Work with notifications
    Notifications {
        #[command(subcommand)]
        cmd: notifications::NotificationsCommand,
    },
    /// Print a file's contents from a repo (owner/repo/path/to/file)
    Raw {
        spec: String,
        #[arg(long = "ref")]
        git_ref: Option<String>,
    },
    /// Launch the interactive TUI (all panels, or one standalone panel)
    Tui {
        #[command(subcommand)]
        panel: Option<TuiPanel>,
    },

    // ---- native git operations (backed by libgit2) ----
    /// Show working tree status
    Status,
    /// Show commit history
    Log {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Show changes (working tree by default, or staged with --staged)
    Diff {
        #[arg(long)]
        staged: bool,
    },
    /// List, create, or delete branches
    Branch {
        /// Create a branch with this name
        #[arg(long)]
        create: Option<String>,
        /// Delete a branch with this name
        #[arg(long)]
        delete: Option<String>,
    },
    /// Switch to a branch or revision (no argument opens a fuzzy branch picker)
    Checkout { refname: Option<String> },
    /// Stage files (defaults to all changes)
    Add { paths: Vec<String> },
    /// Record staged changes
    Commit {
        #[arg(short = 'm', long)]
        message: Option<String>,
        /// Generate the message from the staged diff via an AI backend
        #[arg(long)]
        generate: bool,
    },
    /// Download objects/refs from a remote
    Fetch {
        #[arg(default_value = "origin")]
        remote: String,
    },
    /// Fetch and fast-forward the current branch
    Pull {
        #[arg(default_value = "origin")]
        remote: String,
    },
    /// Upload the current (or given) branch to a remote
    Push {
        #[arg(default_value = "origin")]
        remote: String,
        branch: Option<String>,
        /// Overwrite the remote branch unconditionally
        #[arg(short = 'f', long)]
        force: bool,
        /// Overwrite the remote branch, but abort if it moved since our last fetch
        #[arg(long)]
        force_with_lease: bool,
    },
    /// Clone a repository by URL
    Clone { url: String, dir: Option<String> },
    /// Stash uncommitted changes
    Stash {
        #[command(subcommand)]
        cmd: gitcmd::StashCommand,
    },
    /// List, create, or delete tags
    Tag {
        /// Create a tag with this name
        #[arg(long)]
        create: Option<String>,
        /// Annotate the created tag with this message (implies an annotated, not lightweight, tag)
        #[arg(short = 'm', long)]
        message: Option<String>,
        /// Delete a tag with this name
        #[arg(long)]
        delete: Option<String>,
        /// Also push a newly created tag to this remote
        #[arg(long, requires = "create")]
        push: Option<String>,
    },
    /// List, add, or remove remotes
    Remote {
        #[command(subcommand)]
        cmd: gitcmd::RemoteCommand,
    },
    /// Reset the current branch to a revision
    Reset {
        /// soft (keep changes staged), mixed (keep changes unstaged), or hard (discard changes)
        #[arg(long, default_value = "mixed", value_parser = ["soft", "mixed", "hard"])]
        mode: String,
        #[arg(default_value = "HEAD")]
        target: String,
    },
    /// Merge a branch into the current branch
    Merge { branch: String },
    /// Git LFS: track large files as pointer files, backed by GitHub's LFS
    /// batch API
    Lfs {
        #[command(subcommand)]
        cmd: lfscmd::LfsCommand,
    },
    /// Replay commits from the current branch onto another branch
    Rebase {
        /// Branch or revision to rebase onto
        onto: String,
    },
    /// Apply a single commit from elsewhere onto the current branch
    CherryPick {
        /// Commit to cherry-pick
        commit: String,
    },
    /// Show who last changed each line of a file
    Blame {
        /// Path to the file, relative to the repo root
        path: String,
    },
    /// List, add, or remove worktrees
    Worktree {
        #[command(subcommand)]
        cmd: gitcmd::WorktreeCommand,
    },
    /// Rewrite history to keep or remove a path, or scrub text from blobs
    /// (a native, simplified replacement for git-filter-repo)
    Filter {
        /// Keep (or, with --invert-paths, remove) only history touching this path
        #[arg(long)]
        path: Option<String>,
        /// Invert --path: remove the given path from all history instead of keeping only it
        #[arg(long)]
        invert_paths: bool,
        /// Scrub text from blob contents across history, given as PATTERN=REPLACEMENT (repeatable)
        #[arg(long = "replace-text", value_name = "PATTERN=REPLACEMENT")]
        replace_text: Vec<String>,
        /// Actually rewrite history (without this, only a dry-run summary is printed)
        #[arg(long)]
        yes: bool,
    },
    /// Draw commit history as a graph, or the tracked file tree
    Tree {
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// Show the file tree at HEAD instead of commit history
        #[arg(long)]
        files: bool,
    },
    /// Undo a recent HEAD move using the reflog
    Oops {
        /// Which reflog step to go back to (1 = the most recent move)
        index: Option<usize>,
    },
    /// Delete local branches that are gone from origin and already merged
    Prune {
        /// Actually delete the branches (without this, only a dry-run list is printed)
        #[arg(long)]
        yes: bool,
    },
    /// Snapshot the working tree (tracked and untracked) into a named slot
    Save {
        name: Option<String>,
        /// Show saved slots
        #[arg(long)]
        list: bool,
    },
    /// Restore a named snapshot, switching back to the branch it came from
    Load { name: String },
    /// AI-assisted helpers over the current diff
    Ai {
        #[command(subcommand)]
        cmd: aicmd::AiCommand,
    },
    /// Search tracked files for a regex pattern
    Grep {
        /// Regular expression to search for
        pattern: String,
        /// Limit the search to this path prefix
        path: Option<String>,
    },
    /// Work with repository labels
    Label {
        #[command(subcommand)]
        cmd: label::LabelCommand,
    },
    /// Browse a repo's file tree remotely, like `ls -la` (owner/repo[/path])
    Ls {
        spec: String,
        /// Show entries starting with '.'
        #[arg(short = 'a', long)]
        all: bool,
    },
    /// Download a single file from a repo to local disk (owner/repo/path/to/file)
    Cp {
        spec: String,
        /// Local path to write the file to
        local_path: String,
    },
    /// Delete a file from a repo, creating a real commit (owner/repo/path/to/file)
    Rm {
        spec: String,
        /// Commit message for the deletion
        #[arg(short = 'm', long)]
        message: String,
        /// Actually delete the file (without this, only a dry-run summary is printed)
        #[arg(long)]
        yes: bool,
    },
    /// Make a raw authenticated request against the GitHub API
    Api {
        /// HTTP method (GET, POST, PATCH, PUT, DELETE)
        #[arg(long, default_value = "GET")]
        method: String,
        /// API path, e.g. /repos/owner/repo
        path: String,
        /// JSON request body
        #[arg(long)]
        body: Option<String>,
    },
}

#[derive(Subcommand)]
enum TuiPanel {
    /// Pull request list + detail panel only
    Prs,
    /// Diff viewer panel only (working tree diff)
    Diff {
        #[arg(long)]
        staged: bool,
    },
    /// Branch switcher panel only
    Branches,
    /// Status/stage/commit panel only
    Status,
    /// Commit history panel only
    Log {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Stash panel only
    Stash,
    /// GitHub activity feed panel only, with infinite scroll
    Feed,
    /// Open PRs and issues with your name on them
    Working,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // A bare `ghx`, `ghx --help`/`-h`, or `ghx help` prints the tree
    // overview instead of clap's default flat help — everything else goes
    // through clap normally so per-subcommand `--help` stays detailed.
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let wants_tree = raw.is_empty()
        || raw.iter().any(|a| a == "--help" || a == "-h" || a == "help") && raw.len() == 1;
    if wants_tree {
        print_tree();
        return Ok(());
    }

    let cli = parse_with_correction(&raw)?;

    if cli.uninstall {
        return uninstall::run();
    }

    if let Some(channel) = cli.update {
        let token = config::Config::resolve_token_no_keychain()?;
        let client = api::Client::new(token)?;
        return update::run(&client, &channel, cli.yes);
    }

    let Some(command) = cli.command else {
        print_tree();
        return Ok(());
    };

    match command {
        Command::Auth { cmd } => return auth::run(cmd),
        Command::Status => return gitcmd::status(),
        Command::Log { limit } => return gitcmd::log(limit),
        Command::Diff { staged } => return gitcmd::diff(staged),
        Command::Branch { create, delete } => return gitcmd::branch(create, delete),
        Command::Checkout { refname } => {
            return match refname {
                Some(refname) => gitcmd::checkout(&refname),
                None if tui::is_interactive() => {
                    tui::App::new(vec![Box::new(tui::BranchesPanel::new())]).run()
                }
                None => anyhow::bail!(
                    "which branch? pass a branch name — the fuzzy picker needs an interactive terminal"
                ),
            }
        }
        Command::Add { paths } => return gitcmd::add(paths),
        Command::Commit { message, generate } => return gitcmd::commit(message, generate),
        Command::Fetch { remote } => return gitcmd::fetch(&remote),
        Command::Pull { remote } => return gitcmd::pull(&remote),
        Command::Push {
            remote,
            branch,
            force,
            force_with_lease,
        } => return gitcmd::push(&remote, branch.as_deref(), force, force_with_lease),
        Command::Clone { url, dir } => return gitcmd::clone(&url, dir.as_deref()),
        Command::Stash { cmd } => return gitcmd::stash(cmd),
        Command::Tag {
            create,
            message,
            delete,
            push,
        } => return gitcmd::tag(create, message, delete, push),
        Command::Remote { cmd } => return gitcmd::remote(cmd),
        Command::Reset { mode, target } => return gitcmd::reset(&mode, &target),
        Command::Merge { branch } => return gitcmd::merge(&branch),
        Command::Lfs { cmd } => return lfscmd::run(cmd),
        Command::Rebase { onto } => return gitcmd::rebase(&onto),
        Command::CherryPick { commit } => return gitcmd::cherry_pick(&commit),
        Command::Blame { path } => return gitcmd::blame(&path),
        Command::Worktree { cmd } => return gitcmd::worktree(cmd),
        Command::Filter {
            path,
            invert_paths,
            replace_text,
            yes,
        } => return filtercmd::run(path, invert_paths, replace_text, yes),
        Command::Ai { cmd } => return aicmd::run(cmd),
        Command::Tree { limit, files } => return treecmd::run(limit, files),
        Command::Oops { index } => return oopscmd::run(index),
        Command::Prune { yes } => return prunecmd::run(yes),
        Command::Save { name, list } => return savecmd::save(name, list),
        Command::Load { name } => return savecmd::load(&name),
        Command::Grep { pattern, path } => return grepcmd::run(&pattern, path),
        Command::Tui { panel } => return run_tui(panel),
        _ => {}
    }

    // Everything else needs a GitHub API client.
    let token = config::Config::resolve_token()?;
    let client = api::Client::new(token)?;
    match command {
        Command::Repo { cmd } => repo::run(&client, cmd),
        Command::Pr { cmd } => pr::run(&client, cmd),
        Command::Issue { cmd } => issue::run(&client, cmd),
        Command::Run { cmd } => runcmd::run(&client, cmd),
        Command::Raw { spec, git_ref } => repo::raw(&client, &spec, git_ref),
        Command::Org { cmd } => org::run(&client, cmd),
        Command::Webhook { cmd } => webhook::run(&client, cmd),
        Command::Notifications { cmd } => notifications::run(&client, cmd),
        Command::Label { cmd } => label::run(&client, cmd),
        Command::Api { method, path, body } => apicmd::run(&client, method, path, body),
        Command::Ls { spec, all } => browse::ls(&client, &spec, all),
        Command::Cp { spec, local_path } => browse::cp(&client, &spec, &local_path),
        Command::Rm { spec, message, yes } => browse::rm(&client, &spec, &message, yes),
        _ => unreachable!(),
    }
}

/// Parse the command line, and on an unrecognized top-level subcommand only,
/// retry once with the closest real command name substituted in. Every other
/// parse failure (bad flags, missing arguments) still surfaces clap's own
/// error, since guessing there would hide a real mistake.
fn parse_with_correction(raw: &[String]) -> Result<Cli> {
    let err = match Cli::try_parse() {
        Ok(cli) => return Ok(cli),
        Err(e) => e,
    };
    if err.kind() != clap::error::ErrorKind::InvalidSubcommand {
        err.exit();
    }

    let Some(pos) = raw.iter().position(|a| !a.starts_with('-')) else {
        err.exit();
    };
    let Some(correction) = closest_command(&raw[pos]) else {
        err.exit();
    };

    eprintln!("Correcting to `ghx {correction}`...");
    let mut argv: Vec<String> = vec!["ghx".to_string()];
    argv.extend_from_slice(&raw[..pos]);
    argv.push(correction);
    argv.extend_from_slice(&raw[pos + 1..]);

    match Cli::try_parse_from(argv) {
        Ok(cli) => Ok(cli),
        Err(e) => e.exit(),
    }
}

/// The single closest real command name, if it's within two edits and
/// strictly closer than every other candidate.
fn closest_command(typed: &str) -> Option<String> {
    use clap::CommandFactory;

    let cmd = Cli::command();
    let mut scored: Vec<(usize, String)> = cmd
        .get_subcommands()
        .map(|s| (levenshtein(typed, s.get_name()), s.get_name().to_string()))
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0));

    let (best, name) = scored.first()?.clone();
    if best > 2 {
        return None;
    }
    if scored.get(1).is_some_and(|(next, _)| *next == best) {
        return None;
    }
    Some(name)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];

    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + sub);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Builds and runs the panel set for `ghx tui`. `None` launches the full
/// composed desktop mode (every panel that has what it needs to run);
/// `Some(panel)` launches just that one panel standalone. Both paths go
/// through the same `tui::App`, so a standalone panel is not a special case
/// of the framework — it's the framework with a panel list of length 1.
fn run_tui(panel: Option<TuiPanel>) -> Result<()> {
    if !tui::is_interactive() {
        eprintln!("ghx tui requires an interactive terminal");
        std::process::exit(1);
    }

    let panels: Vec<Box<dyn tui::Panel>> = match panel {
        Some(TuiPanel::Prs) => {
            let token = config::Config::resolve_token()?;
            let client = api::Client::new(token)?;
            let repo = git::GhRepo::detect()?;
            vec![Box::new(tui::PrsPanel::new(client, repo.owner, repo.name))]
        }
        Some(TuiPanel::Diff { staged }) => {
            let diff_text = git::diff(staged)?;
            vec![Box::new(tui::DiffPanel::new(&diff_text))]
        }
        Some(TuiPanel::Branches) => {
            vec![Box::new(tui::BranchesPanel::new())]
        }
        Some(TuiPanel::Status) => {
            vec![Box::new(tui::StatusPanel::new())]
        }
        Some(TuiPanel::Log { limit }) => {
            vec![Box::new(tui::LogPanel::new(limit))]
        }
        Some(TuiPanel::Stash) => {
            vec![Box::new(tui::StashPanel::new())]
        }
        Some(TuiPanel::Feed) => {
            let token = config::Config::resolve_token()?;
            let client = api::Client::new(token)?;
            vec![Box::new(tui::FeedPanel::new(client))]
        }
        Some(TuiPanel::Working) => {
            let token = config::Config::resolve_token()?;
            let client = api::Client::new(token)?;
            vec![Box::new(tui::WorkingPanel::new(client))]
        }
        None => {
            let mut panels: Vec<Box<dyn tui::Panel>> = Vec::new();

            panels.push(Box::new(tui::StatusPanel::new()));
            panels.push(Box::new(tui::LogPanel::new(100)));

            let token = config::Config::resolve_token();
            if let (Ok(Some(token)), Ok(repo)) = (&token, git::GhRepo::detect()) {
                if let Ok(client) = api::Client::new(Some(token.clone())) {
                    panels.push(Box::new(tui::PrsPanel::new(client, repo.owner, repo.name)));
                }
            }

            let diff_text = git::diff(false).unwrap_or_default();
            panels.push(Box::new(tui::DiffPanel::new(&diff_text)));
            panels.push(Box::new(tui::BranchesPanel::new()));
            panels.push(Box::new(tui::StashPanel::new()));

            if let Ok(Some(gh_token)) = token {
                if let Ok(client) = api::Client::new(Some(gh_token.clone())) {
                    panels.push(Box::new(tui::FeedPanel::new(client)));
                }
                if let Ok(client) = api::Client::new(Some(gh_token)) {
                    panels.push(Box::new(tui::WorkingPanel::new(client)));
                }
            }
            panels
        }
    };

    if panels.is_empty() {
        anyhow::bail!(
            "nothing to show — not in a git repository and no GitHub token configured \
             (run `ghx auth login` first, or run `ghx tui` inside a git repo)"
        );
    }

    tui::App::new(panels).run()
}

fn print_tree() {
    use clap::CommandFactory;
    use colored::Colorize;

    // Tokyo Night Storm palette (no purple/magenta by request) — same
    // family as the LazyVim/VS Code/JetBrains "storm" neon-dark themes.
    let comment = (86, 95, 137); // #565f89 muted/dim
    let cyan = (125, 207, 255); // #7dcfff
    let teal = (115, 218, 202); // #73daca
    let green = (158, 206, 106); // #9ece6a
    let orange = (224, 175, 104); // #e0af68

    println!(
        "{} {}",
        "ghx".truecolor(teal.0, teal.1, teal.2).bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).truecolor(comment.0, comment.1, comment.2)
    );
    println!(
        "{}",
        "A fast, native GitHub and git CLI."
            .truecolor(comment.0, comment.1, comment.2)
    );
    println!();

    let cmd = Cli::command();
    let subcommands: Vec<_> = cmd.get_subcommands().collect();
    let count = subcommands.len();

    for (i, sub) in subcommands.iter().enumerate() {
        let is_last_top = i + 1 == count;
        let branch = if is_last_top { "└─" } else { "├─" };
        let children: Vec<_> = sub.get_subcommands().collect();

        // Every character on the line gets an explicit color, including
        // separator spaces — a plain (uncolored) cell renders as an
        // opaque default background in Windows Terminal, which shows up
        // as visible white/grey banding against a transparent background.
        let sp = " ".truecolor(comment.0, comment.1, comment.2);
        let sp2 = "  ".truecolor(comment.0, comment.1, comment.2);

        if children.is_empty() {
            // Leaf top-level command (most native git ops): show its own args.
            let args = format_args(sub);
            println!(
                "{}{sp}{}{sp}{}{sp2}{}",
                branch.truecolor(comment.0, comment.1, comment.2),
                sub.get_name().truecolor(green.0, green.1, green.2).bold(),
                args.truecolor(orange.0, orange.1, orange.2),
                sub.get_about()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    .truecolor(comment.0, comment.1, comment.2)
            );
            continue;
        }

        println!(
            "{}{sp}{}{sp2}{}",
            branch.truecolor(comment.0, comment.1, comment.2),
            sub.get_name().truecolor(cyan.0, cyan.1, cyan.2).bold(),
            sub.get_about()
                .map(|s| s.to_string())
                .unwrap_or_default()
                .truecolor(comment.0, comment.1, comment.2)
        );

        let prefix = (if is_last_top { "   " } else { "│  " })
            .truecolor(comment.0, comment.1, comment.2);
        let child_count = children.len();

        for (j, child) in children.iter().enumerate() {
            let is_last_child = j + 1 == child_count;
            let child_branch = if is_last_child { "└─" } else { "├─" };

            let args = format_args(child);
            println!(
                "{prefix}{}{sp}{}{sp}{}{sp2}{}",
                child_branch.truecolor(comment.0, comment.1, comment.2),
                child.get_name().truecolor(green.0, green.1, green.2),
                args.truecolor(orange.0, orange.1, orange.2),
                child
                    .get_about()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    .truecolor(comment.0, comment.1, comment.2)
            );
        }
    }

    println!();
    println!(
        "{}",
        "Run `ghx <command> [subcommand] --help` for full details on any command."
            .truecolor(comment.0, comment.1, comment.2)
            .italic()
    );
}

fn format_args(cmd: &clap::Command) -> String {
    let mut parts = Vec::new();

    for arg in cmd.get_positionals() {
        let name = arg.get_id().as_str();
        if arg.is_required_set() {
            parts.push(format!("<{name}>"));
        } else {
            parts.push(format!("[{name}]"));
        }
    }

    for arg in cmd.get_arguments() {
        if arg.is_positional() || arg.get_id() == "help" {
            continue;
        }
        let flag = match arg.get_long() {
            Some(long) => format!("--{long}"),
            None => match arg.get_short() {
                Some(short) => format!("-{short}"),
                None => continue,
            },
        };
        parts.push(format!("[{flag}]"));
    }

    parts.join(" ")
}
