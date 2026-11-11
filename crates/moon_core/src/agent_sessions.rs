//! Agent sessions - lists the sessions each agent already has for a repo, read from where that
//! agent stores them on disk.
//!
//! A task resource remembers the session id its agent was started with, but that id stops
//! meaning anything when the user switches sessions inside the agent or the agent never
//! persisted it. This is the way back: list what the agents actually have on disk, so a
//! task can be attached to one of them.
//!
//! On disk is under the home of the Unix user the agent runs as, which on a server that gives
//! each person one is not the server's - see `homes`. The agent keeps its login there too,
//! which is how a person is told they have none before they start a task - see `logins`.

// Reading the agents' own files is the server's: the window in a browser is only told what
// they hold.
#[cfg(not(target_arch = "wasm32"))]
mod homes;
#[cfg(not(target_arch = "wasm32"))]
mod logins;
#[cfg(not(target_arch = "wasm32"))]
mod on_disk;
#[cfg(not(target_arch = "wasm32"))]
mod open_now;

#[cfg(not(target_arch = "wasm32"))]
pub use homes::{AgentHome, person_who_started};
#[cfg(not(target_arch = "wasm32"))]
pub use logins::{agent_options_for, availability_for, availability_in};
#[cfg(not(target_arch = "wasm32"))]
pub use on_disk::list_for_session;
#[cfg(not(target_arch = "wasm32"))]
pub use open_now::OpenSessions;

use serde::{Deserialize, Serialize};

use crate::api::AgentKind;

/// One session an agent has on disk, as the attach modal lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentSessionView {
    pub agent: AgentKind,
    /// The id the agent itself resumes the session by.
    pub id: String,
    pub title: String,
    pub updated_at_unix: u64,
}
