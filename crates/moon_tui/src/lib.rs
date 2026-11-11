//! This crate runs moon in a terminal: the task board drawn with ratatui, the agents and shells
//! it starts as tabs drawn live, and the commands that need no window - on [`moon_core`], the
//! same core the window draws.
//!
//! The full `moon` opens it as `moon tui`; the `moon` this crate builds is it and nothing more.

mod app;
mod board_actions;
mod board_keys;
mod board_session;
mod draw;
mod line_input;
pub mod lite_cli;
mod panes;
mod pointer;
mod run_menu;
mod shell_links;
mod watched_run;

use std::path::PathBuf;

use anyhow::Result;

pub use lite_cli::help_text;

/// Runs `moon` without a window: reads the command line and does what it says.
pub fn run_moon_without_a_window() -> Result<()> {
    moon_core::shell_locale::adopt_utf8_locale();
    let words: Vec<String> = std::env::args().skip(1).collect();
    lite_cli::run_lite_command(lite_cli::parse_lite_command(&words)?)
}

/// Shows the board of the repo `folder` is in - the one this was started in when none is
/// named - in this terminal, until `q`. Minerve is offered only where `moon __minerve` runs it.
pub fn run_the_tui_with(folder: Option<PathBuf>, offers_minerve: bool) -> Result<()> {
    let session = board_session::BoardSession::open_the_board_of(folder)?;
    let mut app = app::App::new(session, offers_minerve)?;
    let mut screen = moon_tui_terminal::screen::take_the_terminal()?;
    let ran = app.run(&mut screen);
    moon_tui_terminal::screen::give_the_terminal_back();
    ran
}

/// Runs [`run_the_tui_with`] for this binary, which has no Minerve in it.
pub fn run_the_tui(folder: Option<PathBuf>) -> Result<()> {
    run_the_tui_with(folder, false)
}
