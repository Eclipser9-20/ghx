//! Branch switcher panel: live fuzzy filtering over `git::branch_list()`,
//! arrow keys to move selection, Enter to `git::checkout()`.

use crate::git;
use crate::tui::{palette, Panel, PanelSignal};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout as RLayout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// A real modal distinction, nvim-style: `Normal` is for navigation (hjkl,
/// gg/G, Enter, Esc/q to leave the panel to the app), `Insert` is entered
/// via `i` or `/` and is the only mode where typed characters edit the
/// filter text.
enum Mode {
    Normal,
    Insert,
}

pub struct BranchesPanel {
    all: Vec<(String, bool)>,
    filter: String,
    filtered: Vec<usize>,
    state: ListState,
    status: Option<String>,
    mode: Mode,
}

impl BranchesPanel {
    pub fn new() -> Self {
        let all = git::branch_list().unwrap_or_default();
        let filtered: Vec<usize> = (0..all.len()).collect();
        let mut state = ListState::default();
        if !filtered.is_empty() {
            state.select(Some(0));
        }
        Self {
            all,
            filter: String::new(),
            filtered,
            state,
            status: None,
            mode: Mode::Normal,
        }
    }

    fn refilter(&mut self) {
        if self.filter.is_empty() {
            self.filtered = (0..self.all.len()).collect();
        } else {
            let mut scored: Vec<(i32, usize)> = self
                .all
                .iter()
                .enumerate()
                .filter_map(|(i, (name, _))| {
                    fuzzy_score(name, &self.filter).map(|score| (score, i))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.filtered = scored.into_iter().map(|(_, i)| i).collect();
        }
        self.state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    fn selected_name(&self) -> Option<&str> {
        let idx = self.state.selected()?;
        let real = *self.filtered.get(idx)?;
        self.all.get(real).map(|(n, _)| n.as_str())
    }

    fn checkout_selected(&mut self) {
        if let Some(name) = self.selected_name().map(str::to_string) {
            match git::checkout(&name) {
                Ok(()) => {
                    self.status = Some(format!("switched to {name}"));
                    self.all = git::branch_list().unwrap_or_default();
                    self.refilter();
                }
                Err(e) => self.status = Some(format!("error: {e}")),
            }
        }
    }
}

impl Default for BranchesPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Subsequence match with a score rewarding contiguous runs and matches
/// near the start of the string (closer to how fzf-style scorers behave).
/// Returns `None` when `pattern` isn't a subsequence of `text`.
fn fuzzy_score(text: &str, pattern: &str) -> Option<i32> {
    if pattern.is_empty() {
        return Some(0);
    }
    let text_lower = text.to_lowercase();
    let pattern_lower = pattern.to_lowercase();
    let t: Vec<char> = text_lower.chars().collect();
    let p: Vec<char> = pattern_lower.chars().collect();

    let mut score = 0i32;
    let mut ti = 0usize;
    let mut last_match: Option<usize> = None;

    for &pc in &p {
        let mut found = None;
        while ti < t.len() {
            if t[ti] == pc {
                found = Some(ti);
                break;
            }
            ti += 1;
        }
        let idx = found?;

        score += 10;
        if idx == 0 {
            score += 5;
        }
        if let Some(last) = last_match {
            if idx == last + 1 {
                score += 8; // contiguous run bonus
            }
        }
        last_match = Some(idx);
        ti = idx + 1;
    }

    // Shorter overall text with the same matches ranks slightly higher.
    score -= (t.len() as i32 - p.len() as i32).max(0) / 4;
    Some(score)
}

impl Panel for BranchesPanel {
    fn title(&self) -> &str {
        "Branches"
    }

    fn key_hints(&self) -> &str {
        match self.mode {
            Mode::Normal => "j/k: move  gg/G: top/bottom  /  i: filter  Enter/l: checkout",
            Mode::Insert => "type to filter  Esc: back to normal mode",
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let (r, g, b) = if focused { palette::CYAN } else { palette::COMMENT };
        let border_style = Style::default().fg(Color::Rgb(r, g, b));

        let chunks = RLayout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        let filter_title = match self.mode {
            Mode::Insert => "filter -- INSERT --",
            Mode::Normal => "filter",
        };
        let filter_text = if self.filter.is_empty() {
            "(i or / to filter)".to_string()
        } else {
            self.filter.clone()
        };
        let filter_style = match self.mode {
            Mode::Insert => Style::default().fg(Color::Rgb(palette::GREEN.0, palette::GREEN.1, palette::GREEN.2)),
            Mode::Normal => border_style,
        };
        frame.render_widget(
            Paragraph::new(filter_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(filter_title)
                    .border_style(filter_style),
            ),
            chunks[0],
        );

        let (green_r, green_g, green_b) = palette::GREEN;
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .map(|&i| {
                let (name, is_current) = &self.all[i];
                if *is_current {
                    ListItem::new(Line::from(vec![
                        Span::raw("* "),
                        Span::styled(
                            name.clone(),
                            Style::default()
                                .fg(Color::Rgb(green_r, green_g, green_b))
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]))
                } else {
                    ListItem::new(Line::from(format!("  {name}")))
                }
            })
            .collect();

        let title = self
            .status
            .clone()
            .unwrap_or_else(|| self.title().to_string());
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, chunks[1], &mut self.state);
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match self.mode {
            Mode::Insert => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Enter => {
                    self.mode = Mode::Normal;
                    self.checkout_selected();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.refilter();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let _ = c;
                    Ok(PanelSignal::Ignored)
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.refilter();
                    Ok(PanelSignal::Handled)
                }
                _ => Ok(PanelSignal::Ignored),
            },
            Mode::Normal => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = self.filtered.len();
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
                    if !self.filtered.is_empty() {
                        self.state.select(Some(0));
                    }
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('G') => {
                    if !self.filtered.is_empty() {
                        self.state.select(Some(self.filtered.len() - 1));
                    }
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('i') | KeyCode::Char('/') => {
                    self.mode = Mode::Insert;
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    self.checkout_selected();
                    Ok(PanelSignal::Handled)
                }
                _ => Ok(PanelSignal::Ignored),
            },
        }
    }
}
