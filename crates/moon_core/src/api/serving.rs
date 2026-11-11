//! Where the server listens, and what its routes are asked with: the halves of the API only the
//! server reads. Never compiled for the browser, whose window writes its requests out itself -
//! see `crate::backend::remote`.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{AgentKind, BlameOf, DEFAULT_PORT};

pub const DEFAULT_HOST: &str = "127.0.0.1";
const HOST_ENV_VAR: &str = "MOONTASKS_HOST";
const PORT_ENV_VAR: &str = "MOONTASKS_PORT";

pub fn bind_host() -> String {
    crate::moon_env_var_os(HOST_ENV_VAR)
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| DEFAULT_HOST.to_string())
}

pub fn client_host() -> String {
    match bind_host().as_str() {
        "0.0.0.0" | "::" => DEFAULT_HOST.to_string(),
        host => host.to_string(),
    }
}

/// The port the server is asked to listen on: `MOONTASKS_PORT`, or [`DEFAULT_PORT`]. Where
/// it ends up listening may be a port or two up - see [`record_bound_port`].
pub fn port() -> Result<u16> {
    match crate::moon_env_var_os(PORT_ENV_VAR) {
        Some(raw) => raw
            .to_str()
            .and_then(|raw| raw.parse::<u16>().ok())
            .with_context(|| format!("{PORT_ENV_VAR} must be a valid TCP port")),
        None => Ok(DEFAULT_PORT),
    }
}

/// The port this process's server is listening on, once it is: the asked port when that was
/// free, and the next free one after it when another server - a window, or a `moon serve` -
/// already had it. Every address handed out afterwards is on this port.
static BOUND_PORT: OnceLock<u16> = OnceLock::new();

/// Say which port the server bound - once, as a process serves once.
pub fn record_bound_port(port: u16) {
    BOUND_PORT
        .set(port)
        .expect("a process binds its server once");
}

/// The port addresses are written with: the one bound when the server is up, the one asked for
/// before that.
fn port_or_default() -> u16 {
    match BOUND_PORT.get() {
        Some(bound) => *bound,
        None => port().unwrap_or(DEFAULT_PORT),
    }
}

pub fn server_url() -> String {
    format!("http://{}:{}", client_host(), port_or_default())
}

pub fn export_server_url() -> String {
    format!("http://localhost:{}", port_or_default())
}

/// A blame asked about a file, and of which version of it.
#[derive(Serialize, Deserialize)]
pub struct BlameRequest {
    pub file_path: String,
    pub of: BlameOf,
}

/// A file as one commit has it, asked for by path and revision.
#[derive(Deserialize)]
pub struct FileAtQuery {
    pub file_path: String,
    pub revision: String,
}

#[derive(Deserialize)]
pub struct HunkRequest {
    pub hunk_id: String,
}

#[derive(Deserialize)]
pub struct HunkBatchRequest {
    pub hunk_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct FileRequest {
    pub file_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct WriteFileRequest {
    pub file_path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct FileQuery {
    pub file_path: String,
}

/// The folder a picker asks to have listed, as `list_folder` of the full moon reads one.
#[derive(Deserialize)]
pub struct FolderQuery {
    pub path: String,
}

#[derive(Deserialize)]
pub struct FileSearchQuery {
    pub query: String,
    /// Whether the files the repo's `.gitignore` leaves out are searched too - see
    /// [`SearchScope`], which this is on the wire.
    pub include_ignored: bool,
}

#[derive(Deserialize)]
pub struct CommitHistoryQuery {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct CancelCommentDispatchRequest {
    pub hunk_id: String,
    pub comment_index: usize,
}

#[derive(Deserialize)]
pub struct AgentLogQuery {
    pub dispatch_key: String,
}

#[derive(Deserialize)]
pub struct AgentSelectionRequest {
    pub agent: AgentKind,
}

#[derive(Deserialize)]
pub struct CommitSelectionRequest {
    pub commit: Option<String>,
}

#[derive(Deserialize)]
pub struct SelectionRequest {
    pub hunk_id: String,
    pub selection: String,
}
