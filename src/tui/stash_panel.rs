//! Stash list panel: apply (pop) or drop a stash, or save a new one.

use crate::git;
use crate::tui::{palette, Panel, PanelSignal};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

enum View {
    List,
    SaveMessage,
}

pub struct StashPanel {
    entries: Vec<git::StashEntry>,
    state: ListState,
    view: View,
    message: String,
    status: Option<String>,
}

impl StashPanel {
    pub fn new() -> Self {
        let mut panel = Self {
            entries: Vec::new(),
            state: ListState::default(),
            view: View::List,
            message: String::new(),
            status: None,
        };
        panel.reload();
        panel
    }

    fn reload(&mut self) {
        match git::stash_list() {
            Ok(entries) => {
                if !entries.is_empty() {
                    self.state.select(Some(0));
                } else {
                    self.state.select(None);
                }
                self.entries = entries;
            }
            Err(e) => self.status = Some(format!("error: {e}")),
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.state.selected().and_then(|i| self.entries.get(i)).map(|e| e.index)
    }

    fn pop(&mut self) {
        let Some(idx) = self.selected_index() else { return };
        match git::stash_pop(idx) {
            Ok(()) => self.status = Some(format!("applied stash@{{{idx}}}")),
            Err(e) => self.status = Some(format!("error: {e}")),
        }
        self.reload();
    }

    fn drop(&mut self) {
        let Some(idx) = self.selected_index() else { return };
        match git::stash_drop(idx) {
            Ok(()) => self.status = Some(format!("dropped stash@{{{idx}}}")),
            Err(e) => self.status = Some(format!("error: {e}")),
        }
        self.reload();
    }

    fn save(&mut self) {
        let msg = if self.message.trim().is_empty() {
            None
        } else {
            Some(self.message.as_str())
        };
        match git::stash_save(msg) {
            Ok(()) => self.status = Some("stashed changes".to_string()),
            Err(e) => self.status = Some(format!("error: {e}")),
        }
        self.message.clear();
        self.view = View::List;
        self.reload();
    }
}

impl Default for StashPanel {
    fn default() -> Self {
        Self::new()
    }
}

fn border(title: &str, focused: bool) -> Block<'static> {
    let (r, g, b) = if focused { palette::CYAN } else { palette::COMMENT };
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .border_style(Style::default().fg(Color::Rgb(r, g, b)))
}

impl Panel for StashPanel {
    fn title(&self) -> &str {
        "Stash"
    }

    fn key_hints(&self) -> &str {
        match self.view {
            View::List => "j/k: move  gg/G: top/bottom  Enter: apply+drop  d: drop  s: save new  r: refresh",
            View::SaveMessage => "type message (optional)  Enter: save  Esc: cancel",
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        if let View::SaveMessage = self.view {
            let block = border("save stash message (optional)", focused);
            frame.render_widget(Paragraph::new(self.message.as_str()).block(block), area);
            return;
        }

        if self.entries.is_empty() {
            let title = self.status.clone().unwrap_or_else(|| "no stashes".to_string());
            frame.render_widget(
                Paragraph::new(title).block(border(self.title(), focused)),
                area,
            );
            return;
        }

        let (orange_r, orange_g, orange_b) = palette::ORANGE;
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("stash@{{{}}} ", e.index),
                        Style::default()
                            .fg(Color::Rgb(orange_r, orange_g, orange_b))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(e.message.clone()),
                ]))
            })
            .collect();

        let title = self.status.clone().unwrap_or_else(|| self.title().to_string());
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg({
                        let (r, g, b) = if focused { palette::CYAN } else { palette::COMMENT };
                        Color::Rgb(r, g, b)
                    })),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match self.view {
            View::SaveMessage => match key.code {
                KeyCode::Esc => {
                    self.message.clear();
                    self.view = View::List;
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Enter => {
                    self.save();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Backspace => {
                    self.message.pop();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char(c) => {
                    self.message.push(c);
                    Ok(PanelSignal::Handled)
                }
                _ => Ok(PanelSignal::Ignored),
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
                KeyCode::Enter => {
                    self.pop();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('d') => {
                    self.drop();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('s') => {
                    self.view = View::SaveMessage;
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('r') => {
                    self.reload();
                    Ok(PanelSignal::Handled)
                }
                _ => Ok(PanelSignal::Ignored),
            },
        }
    }
}
