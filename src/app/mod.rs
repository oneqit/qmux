pub mod actions;
pub mod event;
pub mod mode;
pub mod ui;

use crate::app::event::{handle_key, EventOutcome};
use crate::app::mode::Mode;
use anyhow::Result;
use crossterm::{
    event::{poll, read, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};
use tui_textarea::TextArea;

#[derive(Clone)]
pub struct AgentRow {
    pub pane_id: String,
    pub name: String,
    pub preset: String,
    pub selected: bool,
}

pub const LAYOUTS: &[&str] = &[
    "main-horizontal-mirrored", // default: main (controller) on the bottom
    "main-horizontal",          // main on top
    "main-vertical",            // main on the left
    "main-vertical-mirrored",   // main on the right
];

/// Rows reserved for the controller pane in horizontal layouts.
pub const MAIN_PANE_HEIGHT: u32 = 18;
/// Columns reserved for the controller pane in vertical layouts.
pub const MAIN_PANE_WIDTH: u32 = 50;

pub struct App {
    pub window_id: String,
    pub agents: Vec<AgentRow>,
    pub input: TextArea<'static>,
    pub status: Option<String>,
    pub mode: Mode,
    pub cursor: Option<usize>,
    pub layout_index: usize,
    pub help_scroll: u16,
    pub input_viewport_top: (u16, u16),
    pub tmux: Box<dyn crate::tmux::TmuxRunner>,
}

impl App {
    pub fn new(tmux: Box<dyn crate::tmux::TmuxRunner>, window_id: String) -> Self {
        Self {
            window_id,
            agents: Vec::new(),
            input: TextArea::default(),
            status: None,
            mode: Mode::Normal,
            cursor: None,
            layout_index: 0,
            help_scroll: 0,
            input_viewport_top: (0, 0),
            tmux,
        }
    }

    /// Kept for unit tests that don't touch tmux.
    #[cfg(test)]
    pub fn empty() -> Self {
        struct Null;
        impl crate::tmux::TmuxRunner for Null {
            fn run(&self, _args: &[&str]) -> anyhow::Result<String> {
                Ok(String::new())
            }
        }
        Self::new(Box::new(Null), "@test".to_string())
    }

    pub fn current_layout(&self) -> &'static str {
        LAYOUTS[self.layout_index % LAYOUTS.len()]
    }

    pub fn cycle_layout(&mut self) {
        self.layout_index = (self.layout_index + 1) % LAYOUTS.len();
        let layout = self.current_layout();
        let target = self.window_id.clone();
        let _ = crate::tmux::ensure_controller_first(self.tmux.as_ref(), &target);
        let layout_result = crate::tmux::select_layout(self.tmux.as_ref(), &target, layout);
        self.refresh();
        self.status = Some(match layout_result {
            Ok(_) => format!("layout: {}", layout),
            Err(e) => format!("layout error: {}", e),
        });
    }

    pub fn apply_current_layout(&mut self) {
        let layout = self.current_layout();
        let target = self.window_id.clone();
        let _ = crate::tmux::ensure_controller_first(self.tmux.as_ref(), &target);
        let layout_result = crate::tmux::select_layout(self.tmux.as_ref(), &target, layout);
        self.refresh();
        self.status = Some(match layout_result {
            Ok(_) => format!("layout: {}", layout),
            Err(e) => format!("layout error: {}", e),
        });
    }

    pub fn cursor_down(&mut self) {
        if self.agents.is_empty() {
            self.cursor = None;
            return;
        }
        self.cursor = Some(match self.cursor {
            None => 0,
            Some(i) if i + 1 < self.agents.len() => i + 1,
            Some(i) => i,
        });
    }

    pub fn cursor_up(&mut self) {
        if self.agents.is_empty() {
            self.cursor = None;
            return;
        }
        self.cursor = Some(match self.cursor {
            None => 0,
            Some(0) => 0,
            Some(i) => i - 1,
        });
    }

    pub fn refresh(&mut self) {
        match crate::agent::fetch_agents(self.tmux.as_ref(), &self.window_id) {
            Ok(a) => {
                // Preserve selection by pane_id.
                let selected: std::collections::HashSet<String> = self
                    .agents
                    .iter()
                    .filter(|x| x.selected)
                    .map(|x| x.pane_id.clone())
                    .collect();
                self.agents = a
                    .into_iter()
                    .map(|mut r| {
                        r.selected = selected.contains(&r.pane_id);
                        r
                    })
                    .collect();
                self.status = None;
            }
            Err(e) => {
                self.status = Some(format!("refresh error: {}", e));
            }
        }
        // Clamp cursor.
        if self.agents.is_empty() {
            self.cursor = None;
        } else if let Some(c) = self.cursor {
            if c >= self.agents.len() {
                self.cursor = Some(self.agents.len() - 1);
            }
        } else {
            self.cursor = Some(0);
        }
    }
}

pub fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    install_panic_hook();

    let tmux = crate::tmux::RealTmux;
    let window_id = crate::tmux::current_window_id(&tmux)?;
    let mut app = App::new(Box::new(tmux), window_id);
    app.refresh();
    // Shrink the controller pane; tmux persists these options and they take
    // effect on subsequent select-layout calls.
    let _ = crate::tmux::set_window_option(
        app.tmux.as_ref(),
        &app.window_id,
        "main-pane-height",
        &MAIN_PANE_HEIGHT.to_string(),
    );
    let _ = crate::tmux::set_window_option(
        app.tmux.as_ref(),
        &app.window_id,
        "main-pane-width",
        &MAIN_PANE_WIDTH.to_string(),
    );
    // Don't apply layout on startup — preserves the user's existing pane
    // splits when qmux takes over a pane. Layout is applied on spawn/kill
    // and via the `l`/`L` keys.
    let res = run_loop(&mut terminal, &mut app);

    // Always clean up, even on error.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

const STATUS_TIMEOUT: Duration = Duration::from_secs(3);

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let mut last_status = app.status.clone();
    let mut last_status_at = Instant::now();
    loop {
        if app.status != last_status {
            last_status = app.status.clone();
            last_status_at = Instant::now();
        } else if app.status.is_some() && last_status_at.elapsed() >= STATUS_TIMEOUT {
            app.status = None;
            last_status = None;
        }

        terminal.draw(|f| ui::render(f, app))?;

        if poll(Duration::from_millis(400))? {
            if let Event::Key(key) = read()? {
                match handle_key(app, key) {
                    EventOutcome::Continue => {}
                    EventOutcome::Quit => return Ok(()),
                }
            }
        }
    }
}

fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        default(info);
    }));
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::tmux::mock::MockTmux;

    /// list-panes output where the controller is NOT at pane-index 0.
    /// Used across the layout-anchoring tests — simulates the "user had
    /// splits, then ran qmux" scenario.
    const CTRL_NOT_FIRST: &str = "\
%1|agent|alice|claude|0|0\n\
%0|controller|||20|0\n";

    fn find_call<'a>(calls: &'a [Vec<String>], first: &str) -> Option<&'a Vec<String>> {
        calls
            .iter()
            .find(|c| c.first().map(String::as_str) == Some(first))
    }

    fn index_of(calls: &[Vec<String>], first: &str) -> Option<usize> {
        calls
            .iter()
            .position(|c| c.first().map(String::as_str) == Some(first))
    }

    #[test]
    fn apply_current_layout_swaps_controller_to_first_before_select() {
        let m = MockTmux::new()
            .with_ok(CTRL_NOT_FIRST) // list-panes (from ensure_controller_first)
            .with_ok("") // swap-pane
            .with_ok("") // select-layout
            .with_ok(CTRL_NOT_FIRST); // list-panes (from refresh)
        let handle = m.calls_handle();
        let mut app = App::new(Box::new(m), "@1".into());
        app.apply_current_layout();
        let calls = handle.borrow();
        let swap_idx = index_of(&calls, "swap-pane").expect("swap-pane should be called");
        let sel_idx = index_of(&calls, "select-layout").expect("select-layout should be called");
        assert!(
            swap_idx < sel_idx,
            "swap-pane must happen before select-layout"
        );
        let swap = &calls[swap_idx];
        assert_eq!(swap, &vec!["swap-pane", "-d", "-s", "%0", "-t", "%1"]);
    }

    #[test]
    fn cycle_layout_swaps_controller_to_first_before_select() {
        let m = MockTmux::new()
            .with_ok(CTRL_NOT_FIRST)
            .with_ok("")
            .with_ok("")
            .with_ok(CTRL_NOT_FIRST); // list-panes (from refresh)
        let handle = m.calls_handle();
        let mut app = App::new(Box::new(m), "@1".into());
        app.cycle_layout();
        let calls = handle.borrow();
        assert!(
            find_call(&calls, "swap-pane").is_some(),
            "swap-pane should run"
        );
        let swap_idx = index_of(&calls, "swap-pane").unwrap();
        let sel_idx = index_of(&calls, "select-layout").unwrap();
        assert!(
            swap_idx < sel_idx,
            "swap-pane must happen before select-layout"
        );
    }
}
