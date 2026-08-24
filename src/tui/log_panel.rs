//! Scrollable commit history panel, with per-commit diff viewing.

use crate::git;
use crate::tui::{palette, DiffPanel, Panel, PanelSignal};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

enum View {
    List,
    Diff(Box<DiffPanel>),
}

pub struct LogPanel {
    entries: Vec<git::LogEntry>,
    state: ListState,
    view: View,
    error: Option<String>,
    list_rect: Rect,
}

impl LogPanel {
    pub fn new(limit: usize) -> Self {
        let mut panel = Self {
            entries: Vec::new(),
            state: ListState::default(),
            view: View::List,
            error: None,
            list_rect: Rect::default(),
        };
        panel.reload(limit);
        panel
    }

    fn reload(&mut self, limit: usize) {
        match git::log(limit) {
            Ok(entries) => {
                if !entries.is_empty() {
                    self.state.select(Some(0));
                }
                self.entries = entries;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn open_diff(&mut self) {
        let Some(entry) = self.state.selected().and_then(|i| self.entries.get(i)) else {
            return;
        };
        match git::diff_commit(&entry.id) {
            Ok(diff) => self.view = View::Diff(Box::new(DiffPanel::new(&diff))),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}

impl Default for LogPanel {
    fn default() -> Self {
        Self::new(100)
    }
}

fn border(title: &str, focused: bool) -> Block<'static> {
    let (r, g, b) = if focused { palette::CYAN } else { palette::COMMENT };
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .border_style(Style::default().fg(Color::Rgb(r, g, b)))
}

impl Panel for LogPanel {
    fn title(&self) -> &str {
        "Log"
    }

    fn key_hints(&self) -> &str {
        match self.view {
            View::List => "j/k: move  gg/G: top/bottom  Enter/l: view diff  r: refresh",
            View::Diff(_) => "j/k: scroll  Esc: back",
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        if let View::Diff(diff) = &mut self.view {
            diff.render(frame, area, focused);
            return;
        }

        if let Some(err) = &self.error {
            frame.render_widget(
                ratatui::widgets::Paragraph::new(err.as_str())
                    .style(Style::default().fg(Color::Red))
                    .block(border(self.title(), focused)),
                area,
            );
            return;
        }

        let (green_r, green_g, green_b) = palette::GREEN;
        let (comment_r, comment_g, comment_b) = palette::COMMENT;
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", e.id),
                        Style::default()
                            .fg(Color::Rgb(green_r, green_g, green_b))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("{}  ", e.summary)),
                    Span::styled(
                        format!("{} · {}", e.author, e.time),
                        Style::default().fg(Color::Rgb(comment_r, comment_g, comment_b)),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(border(self.title(), focused))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        self.list_rect = area;
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn select_at(&mut self, x: u16, y: u16) {
        if matches!(self.view, View::Diff(_)) {
            return;
        }
        if let Some(i) = crate::tui::row_index_at(
            self.list_rect,
            self.state.offset(),
            self.entries.len(),
            x,
            y,
        ) {
            self.state.select(Some(i));
        }
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match &mut self.view {
            View::Diff(diff) => match key.code {
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    self.view = View::List;
                    Ok(PanelSignal::Handled)
                }
                _ => diff.handle_input(key),
            },
            View::List => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = self.entries.len();
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
                    if !self.entries.is_empty() {
                        self.state.select(Some(0));
                    }
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('G') => {
                    if !self.entries.is_empty() {
                        self.state.select(Some(self.entries.len() - 1));
                    }
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    self.open_diff();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('r') => {
                    self.reload(100);
                    Ok(PanelSignal::Handled)
                }
                _ => Ok(PanelSignal::Ignored),
            },
        }
    }
}
