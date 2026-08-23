//! Diff viewer panel: syntect-highlighted code with a git2-produced patch,
//! +/- lines colored with the Tokyo Night Storm palette, a bordered
//! line-number gutter, and file-boundary headers (bat/delta-like).

use crate::tui::{palette, Panel, PanelSignal};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

struct DiffLine {
    gutter: String,
    kind: LineKind,
    text: String,
}

enum LineKind {
    Add,
    Remove,
    Context,
    FileHeader,
    HunkHeader,
}

pub struct DiffPanel {
    lines: Vec<DiffLine>,
    scroll: u16,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl DiffPanel {
    pub fn new(raw_diff: &str) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let lines = parse_diff(raw_diff);
        Self {
            lines,
            scroll: 0,
            syntax_set,
            theme_set,
        }
    }

    fn current_syntax<'a>(&'a self, path: &str) -> &'a syntect::parsing::SyntaxReference {
        self.syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
    }

    fn render_lines(&self) -> Vec<Line<'static>> {
        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let (comment_r, comment_g, comment_b) = palette::COMMENT;
        let (green_r, green_g, green_b) = palette::GREEN;
        let (orange_r, orange_g, orange_b) = palette::ORANGE;

        let mut out = Vec::new();
        let mut current_path = String::new();
        let mut highlighter: Option<HighlightLines> = None;

        for dl in &self.lines {
            match dl.kind {
                LineKind::FileHeader => {
                    if let Some(p) = extract_path(&dl.text) {
                        current_path = p;
                    }
                    let syntax = self.current_syntax(&current_path);
                    highlighter = Some(HighlightLines::new(syntax, theme));
                    out.push(Line::from(Span::styled(
                        dl.text.clone(),
                        Style::default()
                            .fg(Color::Rgb(orange_r, orange_g, orange_b))
                            .add_modifier(Modifier::BOLD),
                    )));
                    continue;
                }
                LineKind::HunkHeader => {
                    out.push(Line::from(Span::styled(
                        format!("{}{}", dl.gutter, dl.text),
                        Style::default().fg(Color::Rgb(comment_r, comment_g, comment_b)),
                    )));
                    continue;
                }
                _ => {}
            }

            let gutter_style = Style::default().fg(Color::Rgb(comment_r, comment_g, comment_b));
            let mut spans = vec![Span::styled(dl.gutter.clone(), gutter_style)];

            let base_color = match dl.kind {
                LineKind::Add => Some(Color::Rgb(green_r, green_g, green_b)),
                LineKind::Remove => Some(Color::Rgb(255, 122, 133)), // red, matches storm red
                _ => None,
            };

            if let Some(hl) = highlighter.as_mut() {
                if let Ok(ranges) = hl.highlight_line(&dl.text, &self.syntax_set) {
                    for (style, text) in ranges {
                        let color = base_color.unwrap_or_else(|| syn_to_color(style));
                        spans.push(Span::styled(text.to_string(), Style::default().fg(color)));
                    }
                } else {
                    spans.push(plain_span(&dl.text, base_color));
                }
            } else {
                spans.push(plain_span(&dl.text, base_color));
            }

            out.push(Line::from(spans));
        }
        out
    }
}

fn plain_span(text: &str, color: Option<Color>) -> Span<'static> {
    match color {
        Some(c) => Span::styled(text.to_string(), Style::default().fg(c)),
        None => Span::raw(text.to_string()),
    }
}

fn syn_to_color(style: SynStyle) -> Color {
    Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b)
}

fn extract_path(header: &str) -> Option<String> {
    // "diff --git a/foo.rs b/foo.rs" -> "foo.rs"
    header
        .split_whitespace()
        .find(|tok| tok.starts_with("b/"))
        .map(|tok| tok.trim_start_matches("b/").to_string())
}

fn parse_diff(raw: &str) -> Vec<DiffLine> {
    let mut out = Vec::new();
    let mut old_no = 0u32;
    let mut new_no = 0u32;

    for line in raw.lines() {
        if line.starts_with("diff --git") {
            out.push(DiffLine {
                gutter: String::new(),
                kind: LineKind::FileHeader,
                text: line.to_string(),
            });
        } else if line.starts_with("@@") {
            if let Some((o, n)) = parse_hunk_header(line) {
                old_no = o;
                new_no = n;
            }
            out.push(DiffLine {
                gutter: "     | ".to_string(),
                kind: LineKind::HunkHeader,
                text: line.to_string(),
            });
        } else if let Some(rest) = line.strip_prefix('+') {
            if !line.starts_with("+++") {
                out.push(DiffLine {
                    gutter: format!("     {new_no:>4} +"),
                    kind: LineKind::Add,
                    text: rest.to_string(),
                });
                new_no += 1;
            }
        } else if let Some(rest) = line.strip_prefix('-') {
            if !line.starts_with("---") {
                out.push(DiffLine {
                    gutter: format!("{old_no:>4}      -"),
                    kind: LineKind::Remove,
                    text: rest.to_string(),
                });
                old_no += 1;
            }
        } else if let Some(rest) = line.strip_prefix(' ') {
            out.push(DiffLine {
                gutter: format!("{old_no:>4} {new_no:>4}  "),
                kind: LineKind::Context,
                text: rest.to_string(),
            });
            old_no += 1;
            new_no += 1;
        }
    }
    out
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    // "@@ -12,7 +12,8 @@ ..."
    let mid = line.strip_prefix("@@ ")?;
    let end = mid.find(" @@")?;
    let ranges = &mid[..end];
    let mut parts = ranges.split_whitespace();
    let old = parts.next()?.trim_start_matches('-');
    let new = parts.next()?.trim_start_matches('+');
    let old_start: u32 = old.split(',').next()?.parse().ok()?;
    let new_start: u32 = new.split(',').next()?.parse().ok()?;
    Some((old_start, new_start))
}

impl Panel for DiffPanel {
    fn title(&self) -> &str {
        "Diff"
    }

    fn key_hints(&self) -> &str {
        "j/k or arrows: scroll"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let (r, g, b) = if focused {
            palette::CYAN
        } else {
            palette::COMMENT
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title().to_string())
            .border_style(Style::default().fg(Color::Rgb(r, g, b)));

        let lines = self.render_lines();
        if lines.is_empty() {
            frame.render_widget(
                Paragraph::new("no changes").block(block),
                area,
            );
            return;
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            area,
        );
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                Ok(PanelSignal::Handled)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                Ok(PanelSignal::Handled)
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(20);
                Ok(PanelSignal::Handled)
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(20);
                Ok(PanelSignal::Handled)
            }
            _ => Ok(PanelSignal::Ignored),
        }
    }
}
