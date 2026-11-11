//! This is `moon` without a window: the board in a terminal, and the commands that need no
//! window.

fn main() {
    if let Err(error) = moon_tui::run_moon_without_a_window() {
        eprintln!("moon: {error:#}");
        std::process::exit(1);
    }
}
