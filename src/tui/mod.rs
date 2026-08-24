//! Reusable TUI framework for `ghx tui`.
//!
//! # Adding a new panel without touching the other 3
//!
//! 1. Create `src/tui/your_panel.rs` with a struct implementing the `Panel`
//!    trait below (`title`, `key_hints`, `render`, `handle_input`). Look at
//!    `branches.rs` for the smallest complete example.
//! 2. Add `mod your_panel;` here and re-export the struct if `main.rs` needs
//!    to construct it directly.
//! 3. Add one variant to the `TuiPanel` enum in `main.rs` (mirrors how
//!    `Stash`/`Remote` subcommands are added) and one arm in the match that
//!    builds a `vec![Box::new(YourPanel::new(...))]` for standalone mode.
//! 4. Optionally add it to the default panel set built in `run_composed`
//!    below so it shows up in the full desktop-mode layout.
//!
//! No other panel's code changes. `App` only ever talks to panels through
//! the `Panel` trait object, and `Layout::for_panel_count` only cares how
//! many panels are active, not what they are.

mod branches;
mod diff_panel;
mod log_panel;
mod markdown;
mod prs;
mod stash_panel;
mod status_panel;

pub use branches::BranchesPanel;
pub use diff_panel::DiffPanel;
pub use log_panel::LogPanel;
pub use prs::PrsPanel;
pub use stash_panel::StashPanel;
pub use status_panel::StatusPanel;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout as RLayout, Rect};
use ratatui::prelude::CrosstermBackend;
use ratatui::Frame;
use ratatui::Terminal;
use std::io::{self, IsTerminal};
use std::time::Duration;

/// Tokyo Night Storm palette — kept in sync with `main.rs::print_tree`.
pub mod palette {
    pub const COMMENT: (u8, u8, u8) = (86, 95, 137);
    pub const CYAN: (u8, u8, u8) = (125, 207, 255);
    pub const TEAL: (u8, u8, u8) = (115, 218, 202);
    pub const GREEN: (u8, u8, u8) = (158, 206, 106);
    pub const ORANGE: (u8, u8, u8) = (224, 175, 104);
}

/// Whether both stdin and stdout are attached to a real terminal. Any TUI
/// entry point must check this first — never enter raw mode / alternate
/// screen otherwise, since that would hang or corrupt output for a
/// non-interactive caller (a script, an AI agent, a pipe).
pub fn is_interactive() -> bool {
    io::stdout().is_terminal() && io::stdin().is_terminal()
}

/// A signal a panel can hand back to the app after handling a key.
pub enum PanelSignal {
    /// Key was handled, nothing else to do.
    Handled,
    /// Key was not relevant to this panel.
    Ignored,
    /// Panel wants the whole app to exit (e.g. its own quit key).
    #[allow(dead_code)]
    Quit,
}

/// Something that can be shown as one tile of the TUI, standalone or
/// composed alongside others. A panel owns its own state and is otherwise
/// opaque to `App` — it never reaches into another panel's data.
pub trait Panel {
    /// Short name shown in the panel's border/title.
    fn title(&self) -> &str;
    /// One-line key legend shown in the footer when this panel is focused.
    fn key_hints(&self) -> &str;
    /// Draw the panel into `area`. `focused` indicates whether it currently
    /// has input focus (panels typically use this to style their border).
    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool);
    /// Handle one key event. Only called on the currently focused panel.
    fn handle_input(&mut self, key: KeyEvent) -> Result<PanelSignal>;
}

/// Arranges N active panels on screen. Single panel fills the whole area;
/// multiple panels are tiled in a grid that grows by rows as the count
/// increases, so composition is not limited to any fixed number of panels.
pub struct Layout;

impl Layout {
    pub fn split(area: Rect, count: usize) -> Vec<Rect> {
        if count == 0 {
            return Vec::new();
        }
        if count == 1 {
            return vec![area];
        }

        // Aim for a roughly square grid: cols = ceil(sqrt(count)).
        let cols = (count as f64).sqrt().ceil() as usize;
        let rows = count.div_ceil(cols);

        let row_areas = RLayout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Ratio(1, rows as u32); rows])
            .split(area);

        let mut out = Vec::with_capacity(count);
        let mut remaining = count;
        for row_area in row_areas.iter() {
            let cols_in_row = cols.min(remaining);
            let col_areas = RLayout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, cols_in_row as u32); cols_in_row])
                .split(*row_area);
            out.extend(col_areas.iter().copied());
            remaining -= cols_in_row;
        }
        out
    }
}

/// Owns the active panel set, which one has focus, and routes input.
pub struct App {
    panels: Vec<Box<dyn Panel>>,
    focused: usize,
}

impl App {
    pub fn new(panels: Vec<Box<dyn Panel>>) -> Self {
        Self { panels, focused: 0 }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let size = frame.area();
        let footer_h = 1;
        let body = Rect {
            height: size.height.saturating_sub(footer_h),
            ..size
        };
        let footer = Rect {
            y: size.y + body.height,
            height: footer_h,
            ..size
        };

        let areas = Layout::split(body, self.panels.len());
        for (i, (panel, area)) in self.panels.iter_mut().zip(areas).enumerate() {
            panel.render(frame, area, i == self.focused);
        }

        if let Some(panel) = self.panels.get(self.focused) {
            use ratatui::style::{Color, Style};
            use ratatui::text::Line;
            use ratatui::widgets::Paragraph;
            let (r, g, b) = palette::COMMENT;
            let switch_hint = if self.panels.len() > 1 {
                "Tab/Shift+Tab: switch panel  Alt+1-9: jump to panel  "
            } else {
                ""
            };
            let hint = format!(" {}  |  {switch_hint}q: quit ", panel.key_hints());
            frame.render_widget(
                Paragraph::new(Line::from(hint)).style(Style::default().fg(Color::Rgb(r, g, b))),
                footer,
            );
        }
    }

    /// Runs the event loop until the user quits. Caller must have already
    /// verified `is_interactive()`.
    pub fn run(mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if !event::poll(Duration::from_millis(200))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if key.code == KeyCode::Tab && self.panels.len() > 1 {
                self.focused = (self.focused + 1) % self.panels.len();
                continue;
            }
            if key.code == KeyCode::BackTab && self.panels.len() > 1 {
                self.focused = (self.focused + self.panels.len() - 1) % self.panels.len();
                continue;
            }
            // Alt+<digit> jumps straight to a panel by position, without
            // colliding with plain digit characters typed into a panel's
            // own text input (commit message, stash message, filter box).
            if key.modifiers.contains(event::KeyModifiers::ALT) {
                if let KeyCode::Char(c @ '1'..='9') = key.code {
                    let idx = c as usize - '1' as usize;
                    if idx < self.panels.len() {
                        self.focused = idx;
                        continue;
                    }
                }
            }

            if let Some(panel) = self.panels.get_mut(self.focused) {
                match panel.handle_input(key)? {
                    PanelSignal::Quit => return Ok(()),
                    PanelSignal::Handled => continue,
                    PanelSignal::Ignored => {}
                }
            }

            // Global fallback keys, only if the focused panel didn't want the key.
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                    return Ok(())
                }
                _ => {}
            }
        }
    }
}
