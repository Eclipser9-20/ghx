//! Syntax-highlighted terminal output for diffs and per-line code (bat/
//! delta-style), sharing the Tokyo Night Storm palette used elsewhere.

use colored::Colorize;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

const THEME: &str = "base16-ocean.dark";

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    fn syntax_for<'a>(&'a self, path: &str) -> &'a SyntaxReference {
        self.syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
    }

    /// Prints a single line of source code, colored by token.
    pub fn print_line(&self, path: &str, code: &str) {
        let theme = &self.theme_set.themes[THEME];
        let mut hl = HighlightLines::new(self.syntax_for(path), theme);
        match hl.highlight_line(code, &self.syntax_set) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    print!("{}", paint(text, style));
                }
                println!();
            }
            Err(_) => println!("{code}"),
        }
    }

    /// Prints a unified diff (as produced by `ghx diff`/`git diff`),
    /// bat/delta-style: file headers and hunk headers dimmed, added/removed
    /// lines solid green/red, unchanged context lines syntax-highlighted
    /// by the file's language.
    pub fn print_diff(&self, raw: &str) {
        if raw.is_empty() {
            println!("{}", "no changes".dimmed());
            return;
        }

        let theme = &self.theme_set.themes[THEME];
        let mut current_path = String::new();
        let mut hl: Option<HighlightLines> = None;

        for line in raw.lines() {
            if line.starts_with("diff --git") {
                if let Some(p) = extract_path(line) {
                    current_path = p;
                }
                hl = Some(HighlightLines::new(self.syntax_for(&current_path), theme));
                println!("{}", line.truecolor(224, 175, 104).bold());
            } else if line.starts_with("@@") {
                println!("{}", line.truecolor(86, 95, 137));
            } else if line.starts_with("+++") || line.starts_with("---") {
                println!("{}", line.truecolor(86, 95, 137));
            } else if let Some(rest) = line.strip_prefix('+') {
                println!("{}{}", "+".truecolor(158, 206, 106), rest.truecolor(158, 206, 106));
            } else if let Some(rest) = line.strip_prefix('-') {
                println!("{}{}", "-".truecolor(255, 122, 133), rest.truecolor(255, 122, 133));
            } else if let Some(rest) = line.strip_prefix(' ') {
                print!(" ");
                match hl.as_mut().and_then(|h| h.highlight_line(rest, &self.syntax_set).ok()) {
                    Some(ranges) => {
                        for (style, text) in ranges {
                            print!("{}", paint(text, style));
                        }
                        println!();
                    }
                    None => println!("{rest}"),
                }
            } else {
                println!("{line}");
            }
        }
    }
}

fn paint(text: &str, style: SynStyle) -> colored::ColoredString {
    let c = style.foreground;
    text.truecolor(c.r, c.g, c.b)
}

fn extract_path(header: &str) -> Option<String> {
    header
        .split_whitespace()
        .find(|tok| tok.starts_with("b/"))
        .map(|tok| tok.trim_start_matches("b/").to_string())
}
