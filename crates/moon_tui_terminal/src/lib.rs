//! This crate draws a shell live in a ratatui frame: what the shell prints goes through Ghostty's
//! emulator, and its grid is copied cell by cell into the buffer. Keys, pastes and the mouse go
//! back to the shell encoded the way the program asked for. What leads to the shell - a PTY of
//! this process, a socket to another - is a [`ShellLink`].
//!
//! It also takes the terminal over for a full-screen app, and gives it back.

mod live_terminal;
pub mod screen;
mod terminal_keys;

pub use live_terminal::{LiveTerminal, ShellLink};
