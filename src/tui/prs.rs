//! PR list + detail panel. Reuses `crate::api::Client` for all HTTP.

use crate::api::Client;
use crate::tui::{markdown, palette, Panel, PanelSignal};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout as RLayout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use serde_json::Value;

enum View {
    List,
    Detail,
}

pub struct PrsPanel {
    client: Client,
    owner: String,
    repo: String,
    prs: Vec<Value>,
    comments: Vec<Value>,
    state: ListState,
    view: View,
    scroll: u16,
    error: Option<String>,
}

impl PrsPanel {
    pub fn new(client: Client, owner: String, repo: String) -> Self {
        let mut panel = Self {
            client,
            owner,
            repo,
            prs: Vec::new(),
            comments: Vec::new(),
            state: ListState::default(),
            view: View::List,
            scroll: 0,
            error: None,
        };
        panel.reload();
        panel
    }

    fn reload(&mut self) {
        let path = format!(
            "/repos/{}/{}/pulls?state=open&per_page=50",
            self.owner, self.repo
        );
        match self.client.get::<Vec<Value>>(&path) {
            Ok(prs) => {
                if !prs.is_empty() {
                    self.state.select(Some(0));
                }
                self.prs = prs;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn selected(&self) -> Option<&Value> {
        self.state.selected().and_then(|i| self.prs.get(i))
    }

    fn open_detail(&mut self) {
        let Some(pr) = self.selected() else { return };
        let number = pr["number"].as_u64().unwrap_or(0);
        let path = format!(
            "/repos/{}/{}/issues/{}/comments",
            self.owner, self.repo, number
        );
        self.comments = self.client.get::<Vec<Value>>(&path).unwrap_or_default();
        self.view = View::Detail;
        self.scroll = 0;
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let (cyan_r, cyan_g, cyan_b) = palette::CYAN;
        let (comment_r, comment_g, comment_b) = palette::COMMENT;
        let (green_r, green_g, green_b) = palette::GREEN;

        if let Some(err) = &self.error {
            let block = border(self.title(), focused);
            frame.render_widget(
                Paragraph::new(err.as_str())
                    .style(Style::default().fg(Color::Red))
                    .block(block)
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }

        let items: Vec<ListItem> = self
            .prs
            .iter()
            .map(|pr| {
                let number = pr["number"].as_u64().unwrap_or(0);
                let title = pr["title"].as_str().unwrap_or("?");
                let branch = pr["head"]["ref"].as_str().unwrap_or("?");
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("#{number} "),
                        Style::default()
                            .fg(Color::Rgb(green_r, green_g, green_b))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("{title}  ")),
                    Span::styled(
                        branch.to_string(),
                        Style::default().fg(Color::Rgb(cyan_r, cyan_g, cyan_b)),
                    ),
                ]))
            })
            .collect();

        if items.is_empty() {
            frame.render_widget(
                Paragraph::new("no open pull requests")
                    .style(Style::default().fg(Color::Rgb(comment_r, comment_g, comment_b)))
                    .block(border(self.title(), focused)),
                area,
            );
            return;
        }

        let list = List::new(items)
            .block(border(self.title(), focused))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn render_detail(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let Some(pr) = self.selected().cloned() else {
            self.view = View::List;
            return;
        };
        let number = pr["number"].as_u64().unwrap_or(0);
        let title = pr["title"].as_str().unwrap_or("?");
        let author = pr["user"]["login"].as_str().unwrap_or("?");
        let base = pr["base"]["ref"].as_str().unwrap_or("?");
        let head = pr["head"]["ref"].as_str().unwrap_or("?");
        let body = pr["body"].as_str().unwrap_or("_no description_");

        let mut lines = vec![
            Line::from(Span::styled(
                format!("#{number} {title}"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("{head} -> {base}  by {author}")),
            Line::default(),
        ];
        lines.extend(markdown::render(body));

        if !self.comments.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                format!("-- {} comments --", self.comments.len()),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for c in &self.comments {
                let user = c["user"]["login"].as_str().unwrap_or("?");
                let text = c["body"].as_str().unwrap_or("");
                lines.push(Line::default());
                lines.push(Line::from(Span::styled(
                    format!("@{user}"),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                lines.extend(markdown::render(text));
            }
        }

        let block = border(&format!("{} — detail (Esc: back)", self.title()), focused);
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            area,
        );
    }
}

fn border(title: &str, focused: bool) -> Block<'static> {
    let (r, g, b) = if focused {
        palette::CYAN
    } else {
        palette::COMMENT
    };
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .border_style(Style::default().fg(Color::Rgb(r, g, b)))
}

impl Panel for PrsPanel {
    fn title(&self) -> &str {
        "Pull Requests"
    }

    fn key_hints(&self) -> &str {
        match self.view {
            View::List => "j/k: move  Enter: view  r: refresh",
            View::Detail => "j/k: scroll  Esc: back",
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        // Split into a thin status line + body so a future contributor can
        // add a status/filter bar without touching the list/detail logic.
        let chunks = RLayout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(area);
        match self.view {
            View::List => self.render_list(frame, chunks[0], focused),
            View::Detail => self.render_detail(frame, chunks[0], focused),
        }
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match self.view {
            View::List => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = self.prs.len();
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
                KeyCode::Enter => {
                    self.open_detail();
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Char('r') => {
                    self.reload();
                    Ok(PanelSignal::Handled)
                }
                _ => Ok(PanelSignal::Ignored),
            },
            View::Detail => match key.code {
                KeyCode::Esc => {
                    self.view = View::List;
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.scroll = self.scroll.saturating_add(1);
                    Ok(PanelSignal::Handled)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.scroll = self.scroll.saturating_sub(1);
                    Ok(PanelSignal::Handled)
                }
                _ => Ok(PanelSignal::Ignored),
            },
        }
    }
}
