//! moonreview - the whole application as one library, behind one executable.
//!
//! `moon` opens a window on one of three things - a review, the task board, or a shell -
//! which are three [`cli::Frame`]s over the same everything else; and it reaches the windows
//! that are already open, which is what `moon open` does. `src/bin/moon.rs` is a single call
//! to [`run`].

// The modules gated off wasm32 are the server side - the HTTP server, the shells, the socket a
// `moon open` reaches a window through, and the git, searches, agents and scripts only they
// run - which the build for the browser reaches over HTTP instead. See `web`. The modules
// left in keep their server halves in submodules gated the same way.
#[cfg(not(target_arch = "wasm32"))]
mod agent;
#[cfg(not(target_arch = "wasm32"))]
mod agent_programs;
mod agent_conversation;
mod agent_sessions;
mod api;
pub(crate) mod attention;
mod backend;
mod cli;
mod comments;
#[cfg(not(target_arch = "wasm32"))]
mod desktop_entries;
#[cfg(not(target_arch = "wasm32"))]
mod display;
mod commit_suggestion;
mod committing;
#[cfg(not(target_arch = "wasm32"))]
mod extensions;
#[cfg(not(target_arch = "wasm32"))]
mod git;
#[cfg(not(target_arch = "wasm32"))]
mod gpg;
#[cfg(not(target_arch = "wasm32"))]
mod instances;
#[cfg(not(target_arch = "wasm32"))]
mod lsp;
mod moontasks;
#[cfg(not(target_arch = "wasm32"))]
mod moved_hunks;
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
mod settings;
#[cfg(not(target_arch = "wasm32"))]
mod shell_locale;
#[cfg(not(target_arch = "wasm32"))]
mod shell_path;
mod shell_quoting;
#[cfg(not(target_arch = "wasm32"))]
mod terminal;
#[cfg(not(target_arch = "wasm32"))]
mod unix_users;
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
    // `moon __minerve ...` is minerve itself, linked into moon so one binary ships both: what a
    // Minerve pane starts (see `native::minerve_pane`). Never a word moon's own command line knows.
    if std::env::args().nth(1).as_deref() == Some(MINERVE_SUBCOMMAND) {
        minerve::main_with_args(std::env::args_os().skip(1));
        return Ok(());
    }
    pass_keys::handoff::take_from_environment();
    cli::run()
}

/// The first argument that makes `moon` run as minerve, with minerve's own arguments after it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MINERVE_SUBCOMMAND: &str = "__minerve";

/// The executable `moon __minerve` is run from: this one. A test runs from
/// `target/<profile>/deps`, where the `moon` built beside it is.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn minerve_executable() -> std::io::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    if cfg!(test) {
        let beside = exe.parent().and_then(std::path::Path::parent).map(|dir| dir.join("moon"));
        return beside.ok_or_else(|| std::io::Error::other("the test binary has no target folder"));
    }
    Ok(exe)
}
