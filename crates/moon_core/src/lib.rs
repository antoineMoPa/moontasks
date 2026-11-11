//! moon's core: the task board, the agents and shells working on it, the wire, the socket other
//! moons and shells reach a moon through, git and committing - everything moon does that has
//! no window. The full `moon` draws it with egui; `moon_tui` draws it in a terminal.
//!
//! The modules gated off wasm32 are the server side - the shells, the socket, git and the
//! agents - which the build for the browser reaches over HTTP instead. The modules left in
//! keep their server halves in submodules gated the same way.

#[cfg(not(target_arch = "wasm32"))]
pub mod agent;
pub mod agent_conversation;
#[cfg(not(target_arch = "wasm32"))]
pub mod agent_programs;
pub mod agent_sessions;
pub mod api;
pub mod attention;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
pub mod comments;
pub mod commit_suggestion;
pub mod committing;
#[cfg(not(target_arch = "wasm32"))]
pub mod git;
#[cfg(not(target_arch = "wasm32"))]
pub mod gpg;
#[cfg(not(target_arch = "wasm32"))]
pub mod instances;
pub mod moontasks;
#[cfg(not(target_arch = "wasm32"))]
pub mod moved_hunks;
#[cfg(not(target_arch = "wasm32"))]
pub mod sessions;
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell_locale;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell_path;
pub mod shell_quoting;
#[cfg(not(target_arch = "wasm32"))]
pub mod terminal;
#[cfg(not(target_arch = "wasm32"))]
pub mod unix_users;
pub mod visualizations;

#[cfg(test)]
mod test_fixture;

/// The name moon is typed as, in what it prints and in what it tells its agents to type.
pub const PROGRAM: &str = "moon";

/// The variable `name` (a `MOONTASKS_*` one), or its `MOONREVIEW_*` name from before moon was
/// moontasks, which agents started by an older moon still have.
pub fn moon_env_var_os(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name)
        .or_else(|| std::env::var_os(name.replacen("MOONTASKS_", "MOONREVIEW_", 1)))
}

/// The first argument that makes `moon` run as minerve, with minerve's own arguments after it.
#[cfg(not(target_arch = "wasm32"))]
pub const MINERVE_SUBCOMMAND: &str = "__minerve";

/// The executable `moon __minerve` is run from: this one. A test runs from
/// `target/<profile>/deps`, where the `moon` built beside it is.
#[cfg(not(target_arch = "wasm32"))]
pub fn minerve_executable() -> std::io::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    if cfg!(any(test, feature = "test-support")) {
        let beside = exe
            .parent()
            .and_then(std::path::Path::parent)
            .map(|dir| dir.join("moon"));
        return beside.ok_or_else(|| std::io::Error::other("the test binary has no target folder"));
    }
    Ok(exe)
}
