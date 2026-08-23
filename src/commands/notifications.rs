use crate::api::Client;
use anyhow::Result;
use colored::Colorize;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

#[derive(clap::Subcommand)]
pub enum NotificationsCommand {
    /// List notifications
    List {
        /// Show all notifications, not just unread
        #[arg(long)]
        all: bool,
        /// Show only notifications you're participating in
        #[arg(long)]
        participating: bool,
    },
    /// Mark a notification thread as read
    Read { thread_id: u64 },
    /// Poll for new notifications and print them as they arrive
    Watch {
        #[arg(long, default_value_t = 60)]
        interval: u64,
    },
}

pub fn run(client: &Client, cmd: NotificationsCommand) -> Result<()> {
    match cmd {
        NotificationsCommand::List { all, participating } => list(client, all, participating),
        NotificationsCommand::Read { thread_id } => read(client, thread_id),
        NotificationsCommand::Watch { interval } => watch(client, interval),
    }
}

fn query(all: bool, participating: bool) -> String {
    let mut params = Vec::new();
    if all {
        params.push("all=true".to_string());
    }
    if participating {
        params.push("participating=true".to_string());
    }
    if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    }
}

fn print_notification(n: &Value) {
    let id = n["id"].as_str().unwrap_or("?");
    let reason = n["reason"].as_str().unwrap_or("?");
    let title = n["subject"]["title"].as_str().unwrap_or("?");
    let kind = n["subject"]["type"].as_str().unwrap_or("?");
    let repo = n["repository"]["full_name"].as_str().unwrap_or("?");
    println!(
        "{}  {}  {}  {}  {}",
        id.bold(),
        repo.cyan(),
        kind.dimmed(),
        title,
        format!("({reason})").yellow()
    );
}

fn list(client: &Client, all: bool, participating: bool) -> Result<()> {
    let path = format!("/notifications{}", query(all, participating));
    let notifications: Vec<Value> = client.get(&path)?;
    for n in &notifications {
        print_notification(n);
    }
    Ok(())
}

fn read(client: &Client, thread_id: u64) -> Result<()> {
    let _: Value = client.patch(&format!("/notifications/threads/{thread_id}"), &Value::Null)?;
    println!("{} Marked thread {thread_id} as read", "✓".green().bold());
    Ok(())
}

fn watch(client: &Client, interval: u64) -> Result<()> {
    let mut interval = interval;
    let mut seen: HashSet<String> = HashSet::new();
    let mut first_poll = true;

    println!(
        "{}",
        format!("Watching notifications every {interval}s (Ctrl+C to stop)...").dimmed()
    );

    loop {
        let (notifications, headers): (Vec<Value>, reqwest::header::HeaderMap) =
            client.get_with_headers("/notifications")?;

        if let Some(poll_interval) = headers
            .iter()
            .find(|(name, _)| name.as_str().eq_ignore_ascii_case("x-poll-interval"))
            .and_then(|(_, v)| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            interval = poll_interval;
        }

        for n in &notifications {
            let id = n["id"].as_str().unwrap_or("").to_string();
            if id.is_empty() || seen.contains(&id) {
                continue;
            }
            seen.insert(id.clone());
            if first_poll {
                continue;
            }
            print_notification(n);
            notify_desktop(n);
        }
        first_poll = false;

        std::thread::sleep(Duration::from_secs(interval));
    }
}

fn notify_desktop(n: &Value) {
    let title = n["subject"]["title"].as_str().unwrap_or("New notification");
    let repo = n["repository"]["full_name"].as_str().unwrap_or("");
    let body = format!("{repo}: {title}");

    let result = std::panic::catch_unwind(|| {
        notify_rust::Notification::new()
            .summary("ghx notification")
            .body(&body)
            .show()
    });

    match result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => eprintln!("warning: failed to show desktop notification: {e}"),
        Err(_) => eprintln!("warning: desktop notification backend panicked"),
    }
}
