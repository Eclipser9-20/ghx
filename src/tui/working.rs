//! "Working on" dashboard: the open pull requests you authored, the ones
//! waiting on your review, and the issues assigned to you — one flat,
//! grouped list backed by three `/search/issues` queries.

use crate::api::Client;
use crate::tui::{palette, Panel, PanelSignal};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use serde_json::Value;

const GROUPS: [(&str, &str); 3] = [
    ("Yours", "is:open is:pr author:@me"),
    ("Review requested", "is:open is:pr review-requested:@me"),
    ("Assigned", "is:open is:issue assignee:@me"),
];

struct Item {
    group: &'static str,
    repo: String,
    number: u64,
    title: String,
    url: String,
}

pub struct WorkingPanel {
    client: Client,
    items: Vec<Item>,
    state: ListState,
    loading: bool,
    error: Option<String>,
}

/// `/search/issues` returns the API url; the repo slug is the two path
/// segments after `/repos/`.
fn repo_slug(item: &Value) -> String {
    item["repository_url"]
        .as_str()
        .and_then(|u| u.rsplit("/repos/").next())
        .unwrap_or("?")
        .to_string()
}

impl WorkingPanel {
    pub fn new(client: Client) -> Self {
        let mut panel = Self {
            client,
            items: Vec::new(),
            state: ListState::default(),
            loading: false,
            error: None,
        };
        panel.reload();
        panel
    }

    fn reload(&mut self) {
        self.loading = true;
        self.items.clear();
        self.error = None;

        for (group, query) in GROUPS {
            let path = format!("/search/issues?per_page=30&q={}", query.replace(' ', "+"));
            match self.client.get::<Value>(&path) {
                Ok(data) => {
                    for item in data["items"].as_array().cloned().unwrap_or_default() {
                        self.items.push(Item {
                            group,
                            repo: repo_slug(&item),
                            number: item["number"].as_u64().unwrap_or(0),
                            title: item["title"].as_str().unwrap_or("?").to_string(),
                            url: item["html_url"].as_str().unwrap_or("").to_string(),
                        });
                    }
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        }

        self.state
            .select(if self.items.is_empty() { None } else { Some(0) });
        self.loading = false;
    }

    fn open_selected(&self) {
        if let Some(item) = self.state.selected().and_then(|i| self.items.get(i)) {
            if !item.url.is_empty() {
                let _ = open::that(&item.url);
            }
        }
    }
}

fn border(title: &str, focused: bool) -> Block<'static> {
    let (r, g, b) = if focused { palette::CYAN } else { palette::COMMENT };
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .border_style(Style::default().fg(Color::Rgb(r, g, b)))
}

impl Panel for WorkingPanel {
    fn title(&self) -> &str {
        "Working on"
    }

    fn key_hints(&self) -> &str {
        "j/k: move  gg/G: top/bottom  Enter: open in browser  r: refresh"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        if self.items.is_empty() {
            let text = match (&self.error, self.loading) {
                (Some(err), _) => err.clone(),
                (None, true) => "loading…".to_string(),
                (None, false) => "nothing open with your name on it".to_string(),
            };
            let style = if self.error.is_some() {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            frame.render_widget(
                Paragraph::new(text)
                    .style(style)
                    .block(border(self.title(), focused)),
                area,
            );
            return;
        }

        let (cyan_r, cyan_g, cyan_b) = palette::CYAN;
        let (orange_r, orange_g, orange_b) = palette::ORANGE;

        let mut last_group = "";
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|item| {
                let group = if item.group == last_group {
                    "  ".to_string()
                } else {
                    last_group = item.group;
                    format!("{}  ", item.group)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{group:<20}"),
                        Style::default()
                            .fg(Color::Rgb(orange_r, orange_g, orange_b))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}#{}  ", item.repo, item.number),
                        Style::default().fg(Color::Rgb(cyan_r, cyan_g, cyan_b)),
                    ),
                    Span::raw(item.title.clone()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(border(self.title(), focused))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                let len = self.items.len();
                if len > 0 {
                    let next = self.state.selected().map_or(0, |i| (i + 1).min(len - 1));
                    self.state.select(Some(next));
                }
                Ok(PanelSignal::Handled)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.state.selected().map_or(0, |i| i.saturating_sub(1));
                self.state.select(Some(next));
                Ok(PanelSignal::Handled)
            }
            KeyCode::Char('g') => {
                if !self.items.is_empty() {
                    self.state.select(Some(0));
                }
                Ok(PanelSignal::Handled)
            }
            KeyCode::Char('G') => {
                if !self.items.is_empty() {
                    self.state.select(Some(self.items.len() - 1));
                }
                Ok(PanelSignal::Handled)
            }
            KeyCode::Enter => {
                self.open_selected();
                Ok(PanelSignal::Handled)
            }
            KeyCode::Char('r') => {
                self.reload();
                Ok(PanelSignal::Handled)
            }
            _ => Ok(PanelSignal::Ignored),
        }
    }
}
