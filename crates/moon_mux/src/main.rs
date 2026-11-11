//! `moon-mux` multiplexes shells in one terminal: tabs and splits from moon_tui_frames, each
//! shell drawn live by moon_tui_terminal. Tabs are dragged with the mouse to reorder them, to
//! another frame, or against an edge to split; the line between two frames resizes them.

mod mux;
mod pty_shell;

use anyhow::Result;

const HELP: &str = "\
moon-mux - shells in tabs and splits, in one terminal

usage: moon-mux [program [args...]]
  Starts the program - your $SHELL when none is named - in the first tab. Every other tab
  starts your $SHELL, in the folder moon-mux was started in.

ctrl-a and then a key works the tabs; `ctrl-a ?` lists the keys.";

fn main() -> Result<()> {
    let words: Vec<String> = std::env::args().skip(1).collect();
    match words.first().map(String::as_str) {
        Some("-h" | "--help" | "help") => {
            println!("{HELP}");
            return Ok(());
        }
        Some("-V" | "--version") => {
            println!("moon-mux {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }
    let first_program = (!words.is_empty()).then_some(words);
    let folder = std::env::current_dir()?;
    let mut screen = moon_tui_terminal::screen::take_the_terminal()?;
    let ran =
        mux::Mux::start(first_program, folder, &screen).and_then(|mut mux| mux.run(&mut screen));
    moon_tui_terminal::screen::give_the_terminal_back();
    ran
}
