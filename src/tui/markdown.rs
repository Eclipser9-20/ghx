//! Small pragmatic Markdown-to-ratatui renderer. Handles the subset that
//! actually shows up in PR/issue bodies: headers, bold/italic, fenced code
//! blocks, and bullet lists. Not CommonMark — line/regex based by design.

use crate::tui::palette;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn render(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for raw in text.lines() {
        let line = raw.trim_end();

        if let Some(rest) = line.trim_start().strip_prefix("```") {
            in_code_block = !in_code_block;
            let (r, g, b) = palette::COMMENT;
            lines.push(Line::from(Span::styled(
                format!("  {rest}"),
                Style::default().fg(Color::Rgb(r, g, b)),
            )));
            continue;
        }

        if in_code_block {
            let (r, g, b) = palette::TEAL;
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::Rgb(r, g, b)),
            )));
            continue;
        }

        if let Some(rest) = line.strip_prefix("### ") {
            lines.push(header_line(rest, 1));
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            lines.push(header_line(rest, 2));
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            lines.push(header_line(rest, 3));
            continue;
        }

        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            let mut spans = vec![Span::raw("  • ")];
            spans.extend(inline_spans(rest));
            lines.push(Line::from(spans));
            continue;
        }

        if line.is_empty() {
            lines.push(Line::default());
            continue;
        }

        lines.push(Line::from(inline_spans(line)));
    }

    lines
}

fn header_line(text: &str, level: u8) -> Line<'static> {
    let (r, g, b) = palette::CYAN;
    let mut style = Style::default()
        .fg(Color::Rgb(r, g, b))
        .add_modifier(Modifier::BOLD);
    if level == 3 {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    Line::from(Span::styled(text.to_string(), style))
}

/// Splits one line into styled spans for `**bold**`, `*italic*`/`_italic_`,
/// and `` `inline code` ``. Simple left-to-right scan, not a real parser —
/// good enough for typical PR body text.
fn inline_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    let flush = |buf: &mut String, spans: &mut Vec<Span<'static>>| {
        if !buf.is_empty() {
            spans.push(Span::raw(std::mem::take(buf)));
        }
    };

    while i < chars.len() {
        if chars[i] == '`' {
            if let Some(end) = find_close(&chars, i + 1, '`', 1) {
                flush(&mut buf, &mut spans);
                let code: String = chars[i + 1..end].iter().collect();
                let (r, g, b) = palette::TEAL;
                spans.push(Span::styled(code, Style::default().fg(Color::Rgb(r, g, b))));
                i = end + 1;
                continue;
            }
        } else if chars[i] == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = find_close(&chars, i + 2, '*', 2) {
                flush(&mut buf, &mut spans);
                let bold: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(bold, Style::default().add_modifier(Modifier::BOLD)));
                i = end + 2;
                continue;
            }
        } else if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            if let Some(end) = find_close(&chars, i + 1, marker, 1) {
                flush(&mut buf, &mut spans);
                let italic: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    italic,
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
                continue;
            }
        }
        buf.push(chars[i]);
        i += 1;
    }
    flush(&mut buf, &mut spans);
    spans
}

/// Finds the index of a closing marker (of `width` repeated chars) starting
/// the scan at `from`. Returns the index of the first marker char.
fn find_close(chars: &[char], from: usize, marker: char, width: usize) -> Option<usize> {
    let mut j = from;
    while j + width <= chars.len() {
        if chars[j..j + width].iter().all(|c| *c == marker) {
            return Some(j);
        }
        j += 1;
    }
    None
}
