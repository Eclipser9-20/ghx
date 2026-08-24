//! Status/stage/commit panel: the core git loop (status, add, unstage,
//! commit, optionally AI-generated commit message) without leaving the TUI.

use crate::git;
use crate::tui::{palette, DiffPanel, Panel, PanelSignal};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout as RLayout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

enum View {
    List,
    Diff(Box<DiffPanel>),
    CommitMessage,
}

pub struct StatusPanel {
    files: Vec<git::FileStatus>,
    state: ListState,
    view: View,
    message: String,
    status: Option<String>,
}

impl StatusPanel {
    pub fn new() -> Self {
        let mut panel = Self {
            files: Vec::new(),
            state: ListState::default(),
            view: View::List,
            message: String::new(),
            status: None,
        };
        panel.reload();
        panel
    }

    fn reload(&mut self) {
        match git::status_detailed() {
            Ok(files) => {
                if !files.is_empty()
                    && self.state.selected().is_none_or(|i| i >= files.len())
                {
                    self.state.select(Some(0));
                }
                self.files = files;
            }
            Err(e) => self.status = Some(format!("error: {e}")),
        }
    }

    fn selected(&self) -> Option<&git::FileStatus> {
        self.state.selected().and_then(|i| self.files.get(i))
    }

    fn toggle_stage(&mut self) {
        let Some(f) = self.selected() else { return };
        let path = f.path.clone();
        let result = if f.staged {
            git::unstage(&[path.clone()])
        } else {
            git::add(&[path.clone()])
        };
        match result {
            Ok(()) => self.status = Some(format!("{path} updated")),
            Err(e) => self.status = Some(format!("error: {e}")),
        }
        self.reload();
    }

    fn stage_all(&mut self) {
        match git::add_all() {
            Ok(()) => self.status = Some("staged all changes".to_string()),
            Err(e) => self.status = Some(format!("error: {e}")),
        }
        self.reload();
    }

    fn unstage_all(&mut self) {
        match git::unstage(&[]) {
            Ok(()) => self.status = Some("unstaged all changes".to_string()),
            Err(e) => self.status = Some(format!("error: {e}")),
        }
        self.reload();
    }

    fn open_diff(&mut self) {
        let Some(f) = self.selected() else { return };
        match git::diff_path(&f.path, f.staged) {
            Ok(diff) => self.view = View::Diff(Box::new(DiffPanel::new(&diff))),
            Err(e) => self.status = Some(format!("error: {e}")),
        }
    }

    fn generate_message(&mut self) {
        let staged_diff = git::diff(true).unwrap_or_default();
        match crate::ai::generate_commit_message(&staged_diff) {
            Ok(msg) => {
                self.message = msg;
                self.view = View::CommitMessage;
            }
            Err(e) => self.status = Some(format!("error: {e}")),
        }
    }

    fn commit(&mut self) {
        if self.message.trim().is_empty() {
            self.status = Some("commit message is empty".to_string());
            return;
        }
        match git::commit(&self.message) {
            Ok(id) => {
                self.status = Some(format!("committed {id}"));
                self.message.clear();
                self.view = View::List;
                self.reload();
            }
            Err(e) => self.status = Some(format!("error: {e}")),
        }
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let (r, g, b) = if focused { palette::CYAN } else { palette::COMMENT };
        let border_style = Style::default().fg(Color::Rgb(r, g, b));
        let (green_r, green_g, green_b) = palette::GREEN;
        let (orange_r, orange_g, orange_b) = palette::ORANGE;

        if self.files.is_empty() {
            let title = self.status.clone().unwrap_or_else(|| "clean working tree".to_string());
            frame.render_widget(
                Paragraph::new(title).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(self.title().to_string())
                        .border_style(border_style),
                ),
                area,
            );
            return;
        }

        let items: Vec<ListItem> = self
            .files
            .iter()
            .map(|f| {
                let (mark, color) = if f.staged {
                    ("[x]", (green_r, green_g, green_b))
                } else {
                    ("[ ]", (orange_r, orange_g, orange_b))
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{mark} ")),
                    Span::styled(
                        format!("{:<9}", f.code),
                        Style::default().fg(Color::Rgb(color.0, color.1, color.2)),
                    ),
                    Span::raw(f.path.clone()),
                ]))
            })
            .collect();

        let title = self.status.clone().unwrap_or_else(|| self.title().to_string());
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn render_commit_message(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let (r, g, b) = if focused { palette::CYAN } else { palette::COMMENT };
        let border_style = Style::default().fg(Color::Rgb(r, g, b));
        let block = Block::default()
            .borders(Borders::ALL)
            .title("commit message  (Enter: commit  Ctrl+g: regenerate  Esc: cancel)")
            .border_style(border_style);
        frame.render_widget(Paragraph::new(self.message.as_str()).block(block), area);
    }
}

impl Default for StatusPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for StatusPanel {
    fn title(&self) -> &str {
        "Status"
    }

    fn key_hints(&self) -> &str {
        match self.view {
            View::List => "j/k: move  space: stage/unstage  a: stage all  u: unstage all  Enter: diff  c: commit  g: AI message  r: refresh",
            View::Diff(_) => "j/k: scroll  Esc: back",
            View::CommitMessage => "type message  Enter: commit  Ctrl+g: regenerate with AI  Esc: cancel",
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let chunks = RLayout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(area);
        match &mut self.view {
            View::List => self.render_list(frame, chunks[0], focused),
            View::Diff(diff) => diff.render(frame, chunks[0], focused),
            View::CommitMessage => self.render_commit_message(frame, chunks[0], focused),
        }
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match &mut self.view {
            View::Diff(diff) => match key.code {
                KeyCode::Esc => {
                    self.view = View::List;
                    Ok(PanelSignal::Handled)
                }
                _ => diff.handle_input(key),
            },
            View::CommitMessage => match key.code {
                KeyCode::Esc => {
                    self.view = View::List;
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Enter => {
                    self.commit();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Backspace => {
                    self.message.pop();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.generate_message();
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
                    let len = self.files.len();
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
                KeyCode::Char(' ') => {
                    self.toggle_stage();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('a') => {
                    self.stage_all();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('u') => {
                    self.unstage_all();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Enter => {
                    self.open_diff();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('c') => {
                    self.view = View::CommitMessage;
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('g') => {
                    self.generate_message();
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
