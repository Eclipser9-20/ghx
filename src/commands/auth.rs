use crate::api::Client;
use crate::config::Config;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::Deserialize;
use serde_json::Value;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

#[derive(clap::Subcommand)]
pub enum AuthCommand {
    /// Log in to GitHub
    Login {
        /// Read a personal access token from stdin instead of the device flow
        #[arg(long)]
        with_token: bool,
    },
    /// Show the currently authenticated user
    Status,
    /// Log out and forget the stored token
    Logout,
}

pub fn run(cmd: AuthCommand) -> Result<()> {
    match cmd {
        AuthCommand::Login { with_token } => {
            if with_token {
                login_with_token()
            } else {
                login_device_flow()
            }
        }
        AuthCommand::Status => status(),
        AuthCommand::Logout => logout(),
    }
}

fn login_with_token() -> Result<()> {
    eprint!("Paste your GitHub personal access token: ");
    io::stderr().flush().ok();
    let mut token = String::new();
    io::stdin()
        .read_line(&mut token)
        .context("reading token from stdin")?;
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("no token provided");
    }
    save_and_verify(token)
}

#[derive(Deserialize)]
struct DeviceCodeResp {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
    #[allow(dead_code)]
    expires_in: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TokenPollResp {
    Ok { access_token: String },
    Pending { error: String },
}

fn login_device_flow() -> Result<()> {
    let client_id = std::env::var("GHX_CLIENT_ID").context(
        "device-flow login requires a GitHub OAuth App client id.\n\
         Create one at https://github.com/settings/applications/new \
         (enable \"Device Flow\" under the app's settings), then set:\n  \
         GHX_CLIENT_ID=<your client id>\n\
         Or use `ghx auth login --with-token` with a personal access token instead \
         (Settings -> Developer settings -> Personal access tokens).",
    )?;

    let http = reqwest::blocking::Client::builder()
        .user_agent(concat!("ghx/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let device: DeviceCodeResp = http
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", client_id.as_str()), ("scope", "repo read:org")])
        .send()
        .context("requesting device code")?
        .json()
        .context("parsing device code response")?;

    println!(
        "First, copy your one-time code: {}",
        device.user_code.bold().green()
    );
    println!("Then visit: {}", device.verification_uri.underline());
    let _ = open::that(&device.verification_uri);

    loop {
        thread::sleep(Duration::from_secs(device.interval));

        let resp: Value = http
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id.as_str()),
                ("device_code", device.device_code.as_str()),
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code",
                ),
            ])
            .send()
            .context("polling for access token")?
            .json()
            .context("parsing token poll response")?;

        let parsed: TokenPollResp =
            serde_json::from_value(resp.clone()).context("unexpected token response shape")?;

        match parsed {
            TokenPollResp::Ok { access_token } => return save_and_verify(access_token),
            TokenPollResp::Pending { error } => match error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
                "expired_token" => bail!("device code expired — run `ghx auth login` again"),
                "access_denied" => bail!("authorization was denied"),
                other => bail!("device flow error: {other}"),
            },
        }
    }
}

fn save_and_verify(token: String) -> Result<()> {
    let client = Client::new(Some(token.clone()))?;
    let user: Value = client
        .get("/user")
        .context("token did not work — verify it's valid and has the required scopes")?;
    let login = user
        .get("login")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let mut cfg = Config::load()?;
    cfg.token = Some(token);
    cfg.username = Some(login.clone());
    cfg.save()?;

    println!("{} Logged in as {}", "✓".green().bold(), login.bold());
    Ok(())
}

fn status() -> Result<()> {
    let token = Config::resolve_token()?;
    let Some(token) = token else {
        println!("{} Not logged in. Run `ghx auth login`.", "✗".red().bold());
        return Ok(());
    };

    let client = Client::new(Some(token))?;
    match client.get::<Value>("/user") {
        Ok(user) => {
            let login = user.get("login").and_then(|v| v.as_str()).unwrap_or("?");
            println!("{} Logged in as {}", "✓".green().bold(), login.bold());
        }
        Err(e) => {
            println!("{} Stored token is invalid: {e}", "✗".red().bold());
        }
    }
    Ok(())
}

fn logout() -> Result<()> {
    let mut cfg = Config::load()?;
    cfg.token = None;
    cfg.username = None;
    cfg.save()?;
    println!("{} Logged out", "✓".green().bold());
    Ok(())
}
