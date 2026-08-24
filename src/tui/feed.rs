//! GitHub activity feed: the authenticated user's notifications, paged via
//! the `/notifications` REST API, with genuine infinite scroll — the next
//! page is fetched only once the selection nears the bottom of what's
//! already loaded, never on every scroll tick.

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

const PAGE_SIZE: u32 = 30;
/// Fetch the next page once the selection is within this many rows of the
/// bottom of what's currently loaded.
const PREFETCH_MARGIN: usize = 5;

pub struct FeedPanel {
    client: Client,
    items: Vec<Value>,
    state: ListState,
    page: u32,
    exhausted: bool,
    loading: bool,
    error: Option<String>,
    list_rect: Rect,
}

impl FeedPanel {
    pub fn new(client: Client) -> Self {
        let mut panel = Self {
            client,
            items: Vec::new(),
            state: ListState::default(),
            page: 0,
            exhausted: false,
            loading: false,
            error: None,
            list_rect: Rect::default(),
        };
        panel.load_next_page();
        panel
    }

    fn load_next_page(&mut self) {
        if self.exhausted || self.loading {
            return;
        }
        self.loading = true;
        self.page += 1;
        let path = format!(
            "/notifications?all=true&per_page={PAGE_SIZE}&page={}",
            self.page
        );
        match self.client.get::<Vec<Value>>(&path) {
            Ok(page) => {
                self.error = None;
                if page.len() < PAGE_SIZE as usize {
                    self.exhausted = true;
                }
                let was_empty = self.items.is_empty();
                self.items.extend(page);
                if was_empty && !self.items.is_empty() {
                    self.state.select(Some(0));
                }
            }
            Err(e) => {
                self.exhausted = true; // don't hammer a failing endpoint
                self.error = Some(e.to_string());
            }
        }
        self.loading = false;
    }

    /// Called after moving the selection: fetches the next page once we're
    /// close enough to the end of what's loaded.
    fn maybe_prefetch(&mut self) {
        if let Some(i) = self.state.selected() {
            if i + PREFETCH_MARGIN >= self.items.len() {
                self.load_next_page();
            }
        }
    }

    fn reload(&mut self) {
        self.items.clear();
        self.state.select(None);
        self.page = 0;
        self.exhausted = false;
        self.load_next_page();
    }

    fn mark_selected_read(&mut self) {
        let Some(i) = self.state.selected() else { return };
        let Some(id) = self.items.get(i).and_then(|n| n["id"].as_str()) else {
            return;
        };
        let path = format!("/notifications/threads/{id}");
        if self.client.patch::<Value>(&path, &Value::Null).is_ok() {
            if let Some(n) = self.items.get_mut(i) {
                n["unread"] = Value::Bool(false);
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

impl Panel for FeedPanel {
    fn title(&self) -> &str {
        "Feed"
    }

    fn key_hints(&self) -> &str {
        "j/k: move  gg/G: top/bottom  Enter: mark read  r: refresh  (scrolls load more)"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        if let Some(err) = &self.error {
            frame.render_widget(
                Paragraph::new(err.as_str())
                    .style(Style::default().fg(Color::Red))
                    .block(border(self.title(), focused)),
                area,
            );
            return;
        }

        if self.items.is_empty() {
            let text = if self.loading { "loading…" } else { "no recent activity" };
            frame.render_widget(
                Paragraph::new(text).block(border(self.title(), focused)),
                area,
            );
            return;
        }

        let (cyan_r, cyan_g, cyan_b) = palette::CYAN;
        let (comment_r, comment_g, comment_b) = palette::COMMENT;
        let (green_r, green_g, green_b) = palette::GREEN;

        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|n| {
                let unread = n["unread"].as_bool().unwrap_or(false);
                let repo = n["repository"]["full_name"].as_str().unwrap_or("?");
                let kind = n["subject"]["type"].as_str().unwrap_or("?");
                let title = n["subject"]["title"].as_str().unwrap_or("?");
                let reason = n["reason"].as_str().unwrap_or("?");

                let marker = if unread {
                    Span::styled("● ", Style::default().fg(Color::Rgb(green_r, green_g, green_b)))
                } else {
                    Span::raw("  ")
                };
                ListItem::new(Line::from(vec![
                    marker,
                    Span::styled(
                        format!("{repo}  "),
                        Style::default().fg(Color::Rgb(cyan_r, cyan_g, cyan_b)),
                    ),
                    Span::styled(format!("{kind}: "), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(title.to_string()),
                    Span::styled(
                        format!("  ({reason})"),
                        Style::default().fg(Color::Rgb(comment_r, comment_g, comment_b)),
                    ),
                ]))
            })
            .collect();

        let title = if self.loading {
            format!("{} — loading more…", self.title())
        } else {
            self.title().to_string()
        };
        let list = List::new(items)
            .block(border(&title, focused))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        self.list_rect = area;
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn select_at(&mut self, x: u16, y: u16) {
        if let Some(i) =
            crate::tui::row_index_at(self.list_rect, self.state.offset(), self.items.len(), x, y)
        {
            self.state.select(Some(i));
            self.maybe_prefetch();
        }
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                let len = self.items.len();
                if len > 0 {
                    let next = self.state.selected().map_or(0, |i| (i + 1).min(len - 1));
                    self.state.select(Some(next));
                }
                self.maybe_prefetch();
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
                self.maybe_prefetch();
                Ok(PanelSignal::Handled)
            }
            KeyCode::Enter => {
                self.mark_selected_read();
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
