//! moon - the whole application as one library, behind one executable.
//!
//! `moon` opens a window on one of three things - a review, the task board, or a shell -
//! which are three [`cli::Frame`]s over the same everything else; and it reaches the windows
//! that are already open, which is what `moon open` does. `src/bin/moon.rs` is a single call
//! to [`run`].

// The modules gated off wasm32 are the server side - the HTTP server, the shells, the socket a
// `moon open` reaches a window through, and the git, searches, agents and scripts only they
// run - which the build for the browser reaches over HTTP instead. See `web`. The modules
// left in keep their server halves in submodules gated the same way.
// What moon does without a window is moon_core's, re-exported under the names the rest of
// this crate has always used for it - see `crates/moon_core`. `api`, `instances`, `terminal`
// and `visualizations` keep a window or HTTP side of their own here, beside what they re-export.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use moon_core::{
    agent, git, gpg, moved_hunks, shell_locale, shell_path, unix_users,
};
pub(crate) use moon_core::{
    agent_conversation, agent_sessions, attention, comments, commit_suggestion, committing,
    moontasks, settings, shell_quoting,
};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use moon_core::MINERVE_SUBCOMMAND;

mod api;
mod backend;
mod cli;
#[cfg(not(target_arch = "wasm32"))]
mod desktop_entries;
#[cfg(not(target_arch = "wasm32"))]
mod display;
#[cfg(not(target_arch = "wasm32"))]
mod extensions;
#[cfg(not(target_arch = "wasm32"))]
mod instances;
#[cfg(not(target_arch = "wasm32"))]
mod lsp;
mod native;
#[cfg(not(target_arch = "wasm32"))]
mod opencode;
mod palette_history;
#[cfg(not(target_arch = "wasm32"))]
mod pass_keys;
mod project;
#[cfg(not(target_arch = "wasm32"))]
mod search;
#[cfg(not(target_arch = "wasm32"))]
mod server;
#[cfg(test)]
mod server_tests;
#[cfg(not(target_arch = "wasm32"))]
mod service;
#[cfg(not(target_arch = "wasm32"))]
mod terminal;
mod visualizations;
#[cfg(target_arch = "wasm32")]
mod web;

/// Run `moon`: the command line decides whether that is a window, the server behind one, or
/// a word to a window that is already open.
#[cfg(not(target_arch = "wasm32"))]
pub fn run() -> anyhow::Result<()> {
    // Two crypto backends are linked (ring, and minerve's aws-lc-rs): rustls panics on the
    // first TLS connection unless one is the default.
    let _ = rustls::crypto::ring::default_provider().install_default();
    settings::move_the_old_home_folder();
    // `moon __minerve ...` is minerve itself, linked into moon so one binary ships both: what a
    // Minerve pane starts (see `native::minerve_pane`). Never a word moon's own command line knows.
    if std::env::args().nth(1).as_deref() == Some(MINERVE_SUBCOMMAND) {
        // Minerve reads its first argument as its own name.
        minerve::main_with_args(std::iter::once("minerve".into()).chain(std::env::args_os().skip(2)));
        return Ok(());
    }
    pass_keys::handoff::take_from_environment();
    cli::run()
}



