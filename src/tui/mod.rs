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
mod feed;
mod log_panel;
mod markdown;
mod prs;
mod stash_panel;
mod status_panel;

pub use branches::BranchesPanel;
pub use diff_panel::DiffPanel;
pub use feed::FeedPanel;
pub use log_panel::LogPanel;
pub use prs::PrsPanel;
pub use stash_panel::StashPanel;
pub use status_panel::StatusPanel;

use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::Rect;
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

/// Owns the active panel set, which one has focus, and routes input.
///
/// Multiple panels are presented as a persistent top tab bar (one cell per
/// panel, click or Tab/Alt+N to switch) over a single main content pane
/// showing the focused panel — a "sidebar" reads better as a top bar in a
/// terminal's wide-short aspect ratio than a narrow side column would.
pub struct App {
    panels: Vec<Box<dyn Panel>>,
    focused: usize,
    /// Clickable screen rect for each tab, rebuilt every frame so mouse
    /// clicks can be hit-tested against last frame's actual layout.
    tab_rects: Vec<Rect>,
    /// The main content pane's rect, so scroll events landing there can be
    /// forwarded to the focused panel as synthetic movement keys.
    body_rect: Rect,
}

impl App {
    pub fn new(panels: Vec<Box<dyn Panel>>) -> Self {
        Self {
            panels,
            focused: 0,
            tab_rects: Vec::new(),
            body_rect: Rect::default(),
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;

        let size = frame.area();
        let footer_h = 1;
        let tabs_h = if self.panels.len() > 1 { 1 } else { 0 };

        let tabs_area = Rect {
            height: tabs_h,
            ..size
        };
        let body = Rect {
            y: size.y + tabs_h,
            height: size.height.saturating_sub(footer_h + tabs_h),
            ..size
        };
        let footer = Rect {
            y: size.y + tabs_h + body.height,
            height: footer_h,
            ..size
        };
        self.body_rect = body;

        if self.panels.len() > 1 {
            self.tab_rects.clear();
            let (cyan_r, cyan_g, cyan_b) = palette::CYAN;
            let (comment_r, comment_g, comment_b) = palette::COMMENT;
            let mut spans = Vec::new();
            let mut x = tabs_area.x;
            for (i, panel) in self.panels.iter().enumerate() {
                let label = format!(" {} ", panel.title());
                let width = label.chars().count() as u16;
                self.tab_rects.push(Rect {
                    x,
                    y: tabs_area.y,
                    width,
                    height: 1,
                });
                x += width;
                let style = if i == self.focused {
                    Style::default()
                        .fg(Color::Rgb(cyan_r, cyan_g, cyan_b))
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::Rgb(comment_r, comment_g, comment_b))
                };
                spans.push(Span::styled(label, style));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), tabs_area);
        }

        if let Some(panel) = self.panels.get_mut(self.focused) {
            panel.render(frame, body, true);
        }

        if let Some(panel) = self.panels.get(self.focused) {
            let (r, g, b) = palette::COMMENT;
            let switch_hint = if self.panels.len() > 1 {
                "Tab/click: switch panel  Alt+1-9: jump  "
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
        execute!(stdout, EnterAlternateScreen, event::EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal);

        execute!(terminal.backend_mut(), event::DisableMouseCapture)?;
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
        x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
    }

    /// Handles a mouse event: clicking a tab switches the focused panel,
    /// clicking inside the body focuses it (a no-op in single-panel mode)
    /// and forwards the click as a synthetic Enter so list items become
    /// clickable-to-select-and-open, and the scroll wheel forwards as
    /// repeated Up/Down to whatever panel is focused.
    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<Option<()>> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                for (i, rect) in self.tab_rects.iter().enumerate() {
                    if Self::rect_contains(*rect, mouse.column, mouse.row) {
                        self.focused = i;
                        return Ok(None);
                    }
                }
                if Self::rect_contains(self.body_rect, mouse.column, mouse.row) {
                    let key = KeyEvent::from(KeyCode::Enter);
                    self.dispatch_key(key)?;
                }
            }
            MouseEventKind::ScrollDown => {
                for _ in 0..3 {
                    self.dispatch_key(KeyEvent::from(KeyCode::Down))?;
                }
            }
            MouseEventKind::ScrollUp => {
                for _ in 0..3 {
                    self.dispatch_key(KeyEvent::from(KeyCode::Up))?;
                }
            }
            _ => {}
        }
        Ok(None)
    }

    /// Routes one key through the focused panel, then the global fallback
    /// bindings, exactly like a keyboard-originated key would be. Shared by
    /// the real event loop and by mouse events synthesized into key presses
    /// (a click-to-select, a scroll tick), so both input paths behave
    /// identically to panels.
    fn dispatch_key(&mut self, key: KeyEvent) -> Result<Option<()>> {
        if key.code == KeyCode::Tab && self.panels.len() > 1 {
            self.focused = (self.focused + 1) % self.panels.len();
            return Ok(None);
        }
        if key.code == KeyCode::BackTab && self.panels.len() > 1 {
            self.focused = (self.focused + self.panels.len() - 1) % self.panels.len();
            return Ok(None);
        }
        // Alt+<digit> jumps straight to a panel by position, without
        // colliding with plain digit characters typed into a panel's own
        // text input (commit message, stash message, filter box).
        if key.modifiers.contains(event::KeyModifiers::ALT) {
            if let KeyCode::Char(c @ '1'..='9') = key.code {
                let idx = c as usize - '1' as usize;
                if idx < self.panels.len() {
                    self.focused = idx;
                    return Ok(None);
                }
            }
        }

        if let Some(panel) = self.panels.get_mut(self.focused) {
            match panel.handle_input(key)? {
                PanelSignal::Quit => return Ok(Some(())),
                PanelSignal::Handled => return Ok(None),
                PanelSignal::Ignored => {}
            }
        }

        // Global fallback keys, only if the focused panel didn't want the key.
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(Some(())),
            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                return Ok(Some(()))
            }
            _ => {}
        }
        Ok(None)
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if !event::poll(Duration::from_millis(200))? {
                continue;
            }
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if self.dispatch_key(key)?.is_some() {
                        return Ok(());
                    }
                }
                Event::Mouse(mouse) => {
                    if self.handle_mouse(mouse)?.is_some() {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}
