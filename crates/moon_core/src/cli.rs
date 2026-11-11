//! The commands of `moon` that need no window: the board's cards (`moon tasks new|list|move`),
//! the wire, the agents, and the board served to an agent over MCP. The full moon and
//! `moon_tui` both answer them, with the same words and the same help.

pub mod agent;
pub mod mcp;
pub mod task_shell;
pub mod tasks;
pub mod wire;

pub use crate::PROGRAM;
