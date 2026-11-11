//! This module leads the shells drawn in tabs to where they run: this moon's own, held in its
//! terminal registry, and those another moon holds, over the socket attached to them.

use std::sync::Arc;

use anyhow::Result;
use moon_core::{instances::attach::AttachedShell, terminal::TerminalSession};
use moon_tui_terminal::ShellLink;

/// Leads to one of this moon's shells.
pub struct CoreShell(pub Arc<TerminalSession>);

/// Leads to a shell another moon holds, attached to.
pub struct Attached(pub AttachedShell);

impl ShellLink for CoreShell {
    fn write_input(&self, bytes: &[u8]) -> Result<()> {
        self.0.write_input(bytes)
    }
    fn write_reply(&self, bytes: &[u8]) -> Result<()> {
        self.0.write_reply(bytes)
    }
    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.0.resize(cols, rows)
    }
    fn has_exited(&self) -> bool {
        self.0.has_exited()
    }
}

impl ShellLink for Attached {
    fn write_input(&self, bytes: &[u8]) -> Result<()> {
        self.0.write_input(bytes)
    }
    fn write_reply(&self, bytes: &[u8]) -> Result<()> {
        self.0.write_reply(bytes)
    }
    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.0.resize(cols, rows)
    }
    fn has_exited(&self) -> bool {
        self.0.has_exited()
    }
}
