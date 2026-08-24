# ghx

A single binary that replaces both `git` and the GitHub CLI (`gh`). Git operations are
implemented natively on top of libgit2 — ghx never shells out to a `git` binary — and GitHub
operations talk directly to the REST API.

## Install

**Unix (Linux/macOS):**
```
curl -fsSL https://raw.githubusercontent.com/Eclipser9-20/ghx/master/install.sh | sh
```

**Windows (PowerShell, run as Administrator):**
```
irm https://raw.githubusercontent.com/Eclipser9-20/ghx/master/install.ps1 | iex
```

Both installers set up a system-wide install with a dedicated group
(`_GHXmaintenance`) so the installing user — and anyone later added to that
group — can run `ghx --update` or `ghx --uninstall` without re-elevating.

To update or remove ghx:
```
ghx --update stable   # or beta, or dev
ghx --uninstall
```

## Commands

Run `ghx --help` alone for a full tree of every subcommand. Run `ghx <command> --help`
for detailed flags on a specific command.

### GitHub

| Command | Description |
|---|---|
| `ghx auth` | Log in / out, check auth status |
| `ghx repo` | View, create, delete repos; visibility, collaborators, branch protection |
| `ghx pr` | List, view, create, review, merge pull requests |
| `ghx issue` | List, view, create, close issues |
| `ghx run` | List, view, watch logs, rerun, or cancel Actions workflow runs |
| `ghx org` | List orgs, view members and teams |
| `ghx webhook` | List, create, delete repository webhooks |
| `ghx notifications` | List, read, and watch notifications (with desktop alerts) |
| `ghx raw owner/repo/path` | Print a file's contents straight from a repo, no URL needed |

### Git

| Command | Description |
|---|---|
| `ghx status` / `ghx log` / `ghx diff` | Inspect the working tree and history |
| `ghx add` / `ghx commit` | Stage and record changes (`commit --generate` writes the message from the diff via an AI backend) |
| `ghx branch` / `ghx checkout` | List/create/delete branches, switch branches |
| `ghx clone` / `ghx fetch` / `ghx pull` / `ghx push` | Standard remote sync operations |
| `ghx merge` | Merge a branch into the current one, including fast-forwards |
| `ghx stash` | Save, list, pop, drop stashed changes |
| `ghx tag` / `ghx remote` | Manage tags and remotes |
| `ghx reset` | Soft/mixed/hard reset to a revision |
| `ghx lfs` | Track large files as pointer files, backed by GitHub's LFS batch API |

### TUI

`ghx tui` launches an interactive terminal UI with pull request, diff, and branch-switcher
panels. Any panel also runs standalone: `ghx tui prs`, `ghx tui diff`, `ghx tui branches`.
The TUI is automatically skipped in non-interactive contexts (pipes, CI, scripts).

## Authentication

```
ghx auth login
```

Tokens are stored in the OS credential store (Windows Credential Manager, macOS Keychain,
or the Linux Secret Service) keyed by username. On a machine with no reachable credential
store, ghx falls back to a plaintext config file with a warning. `GHX_TOKEN`, `GH_TOKEN`,
and `GITHUB_TOKEN` environment variables all take precedence over a stored login.

## Private email

Commits and tags normally use the name and email from your git config. To keep a real
email address out of history, enable privacy mode and ghx will instead sign commits with
**your** GitHub no-reply address (`<your-login>@users.noreply.github.com`), derived from
the authenticated account:

```
GHX_PRIVATE_EMAIL=1 ghx commit -m "..."      # per-invocation / per-environment
```

or set it persistently in ghx's config file (`private_email: true`). It is off by default,
so normal behaviour is unchanged. This is useful when multiple machines or tools commit on
your behalf and you don't want to configure `user.email` on each one.

## AI-generated commit messages

`ghx commit --generate` sends the staged diff to an AI backend and uses the result as the
commit message. It looks for `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, or
`OPENROUTER_API_KEY` in that order and needs one of them set.

## Building from source

```
git clone https://github.com/Eclipser9-20/ghx
cd ghx
cargo build --release
```

`build.sh` / `build.ps1` produce a codesigned release build; `build-from-source.sh` is a
minimal build-and-install script for users without prebuilt binaries for their platform.

## License

MIT
