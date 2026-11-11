//! The shells themselves are moon_core's - see [`moon_core::terminal`]. What is left here is
//! how the HTTP server reaches them.

pub(crate) use moon_core::terminal::*;

mod routes;

pub(crate) use routes::{
    close_terminal, create_terminal, list_terminals, rename_terminal, run_in_shell,
    start_workspace_shell, start_workspace_shell_in_folder, start_workspace_shell_running,
    submit_to_agent, terminal_conversation, terminal_socket, terminal_view,
    terminals_running_a_command, terminals_wanting_attention,
};

#[cfg(test)]
mod tests;
