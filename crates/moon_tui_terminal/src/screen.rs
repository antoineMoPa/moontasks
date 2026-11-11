//! This module takes this terminal over for a full-screen app, and always gives it back: on the
//! way out, on a panic, and for as long as `$EDITOR` has it.

use std::io::{Stdout, stdout};

use anyhow::{Context, Result};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{
            DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        },
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};

pub type Screen = Terminal<CrosstermBackend<Stdout>>;

/// Turns on raw keys, the alternate screen, the mouse and pastes, for the app.
pub fn take_the_terminal() -> Result<Screen> {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        give_the_terminal_back();
        previous_hook(info);
    }));
    enable_raw_mode().context("failed to put the terminal in raw mode")?;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
    .context("failed to take the terminal's screen")?;
    Terminal::new(CrosstermBackend::new(stdout())).context("failed to draw on the terminal")
}

/// Puts the terminal back as it was before the app took it.
pub fn give_the_terminal_back() {
    let _ = execute!(
        stdout(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
}

/// Runs `$EDITOR` on a file, with the terminal its own while it does.
pub fn edit_in_the_editor(screen: &mut Screen, path: &std::path::Path) -> Result<()> {
    give_the_terminal_back();
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    // It goes through the shell, as git does, so an editor set as `code --wait` runs.
    let ran = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("moon-editor")
        .arg(path)
        .status();
    enable_raw_mode().context("failed to put the terminal back in raw mode")?;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
    .context("failed to take the terminal's screen back")?;
    screen.clear()?;
    let status = ran.with_context(|| format!("failed to start {editor}"))?;
    anyhow::ensure!(status.success(), "{editor} ended with {status}");
    Ok(())
}
