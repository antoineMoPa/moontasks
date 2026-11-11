//! What the server holds between requests: its sessions, and the agents and shells working
//! on them. Never compiled for the browser, whose window holds none of it - it asks the
//! server, through the types beside this module.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Instant,
};

use anyhow::{Result, anyhow, bail};

use serde::{Deserialize, Serialize};

use super::{AgentKind, DiffTarget, FileChangeKind, ImageDiffView};
use crate::comments::CommentDispatchState;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Mutex<ServerState>>,
    pub agent_availability: AgentAvailability,
    pub last_activity: Arc<Mutex<Instant>>,
    pub terminals: Arc<crate::terminal::TerminalRegistry>,
    /// The `settings.json` every window on this server reads and changes, fixed when the
    /// state is built - under test that is on the test's own thread, which is what gives
    /// each test a file of its own. See [`crate::settings::path`].
    pub settings_path: Option<std::path::PathBuf>,
    /// The same for the palette's history - the window's, which this crate only carries.
    pub palette_history_path: Option<std::path::PathBuf>,
}

impl AppState {
    /// A fresh state for a test: started in no repo, with no palette history.
    #[cfg(test)]
    pub fn for_a_test(last_activity: Arc<Mutex<Instant>>) -> Self {
        Self::build(last_activity, None, None)
    }

    /// The state of a moon that has just started: no sessions, the agents this machine has,
    /// no shells, and `home_repo` - the repo it was started in, if any.
    pub fn build(
        last_activity: Arc<Mutex<Instant>>,
        home_repo: Option<PathBuf>,
        palette_history_path: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ServerState {
                home_repo,
                ..ServerState::default()
            })),
            agent_availability: crate::agent::detect_agent_availability(),
            last_activity: Arc::clone(&last_activity),
            terminals: Arc::new(crate::terminal::TerminalRegistry::new(last_activity)),
            settings_path: crate::settings::path(),
            palette_history_path,
        }
    }
}

#[derive(Default)]
pub struct ServerState {
    pub sessions: HashMap<String, RepoSession>,
    /// The repo this process is for: the one it was started in, or else the first one it
    /// opened - which, for a window started from a launcher, in `/`, is the repo the window is
    /// on. What a browser opening `/moon` without saying which repo is shown - see
    /// `crate::server::web_page`.
    pub home_repo: Option<PathBuf>,
}

pub struct RepoSession {
    pub namespace: Option<String>,
    /// Who this session works for on a server that gives each person a Unix user: what it
    /// starts runs as them - see [`crate::unix_users`]. `None` everywhere else, where
    /// everything runs as the server's own user.
    pub person: Option<crate::unix_users::Person>,
    pub repo_path: PathBuf,
    pub diff_target: DiffTarget,
    pub active_commit: Option<String>,
    pub comments: HashMap<String, String>,
    pub comment_contexts: HashMap<String, HunkCommentContext>,
    pub selected_agent: AgentKind,
    pub comment_dispatches: HashMap<String, CommentDispatchState>,
    /// The files outside the repo a language server has named as answers to this session's
    /// go-to-definition questions, which are the only files outside it that may be read - see
    /// [`crate::git::FilesNamedOutsideTheRepo`].
    pub files_named_outside_the_repo: crate::git::FilesNamedOutsideTheRepo,
}

/// Whose a browser's session is: the profile its selection and comments are kept within, and,
/// on a server that gives each person a Unix user, that person.
pub struct SessionOwner {
    pub namespace: String,
    pub person: Option<crate::unix_users::Person>,
}

/// A profile and no person, which is every session of a server that runs as one user.
impl From<&str> for SessionOwner {
    fn from(namespace: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            person: None,
        }
    }
}

impl RepoSession {
    /// What this session's review is of - see [`ReviewTarget`].
    pub fn review_target(&self) -> ReviewTarget {
        ReviewTarget {
            repo_path: self.repo_path.clone(),
            diff_target: self.diff_target.clone(),
            active_commit: self.active_commit.clone(),
        }
    }
}

/// What a session's review is of: the repo, and which of its changes - the part of a
/// [`RepoSession`] that git is asked about. Taken off the session as a value of its own so
/// git can be asked with the server's lock released: the diff of a repo with a lot changed
/// takes a while, and everything else the server does - starting a shell, staging a hunk -
/// waits on that one lock. Compared after the asking, so an answer about a target the
/// session has since moved off is asked again rather than shown.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReviewTarget {
    pub repo_path: PathBuf,
    pub diff_target: DiffTarget,
    pub active_commit: Option<String>,
}

pub type CancelToken = Arc<AtomicBool>;

/// Live stdout/stderr of one agent run, shared by every comment that run addresses.
pub type AgentLog = Arc<Mutex<String>>;

/// Older output is dropped once a run exceeds this, so a chatty agent cannot grow the session forever.
const AGENT_LOG_MAX_BYTES: usize = 200_000;

pub fn append_to_agent_log(log: &AgentLog, chunk: &str) {
    let Ok(mut text) = log.lock() else {
        return;
    };

    text.push_str(chunk);
    if text.len() <= AGENT_LOG_MAX_BYTES {
        return;
    }

    let drop_until = text
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| text.len() - index <= AGENT_LOG_MAX_BYTES)
        .unwrap_or(text.len());
    text.replace_range(..drop_until, "");
}

pub fn read_agent_log(log: &AgentLog) -> Result<String> {
    log.lock()
        .map(|text| text.clone())
        .map_err(|_| anyhow!("agent log lock poisoned"))
}

#[derive(Clone)]
pub struct DiffHunk {
    pub id: String,
    pub file_path: String,
    pub change_kind: FileChangeKind,
    pub header: String,
    pub patch: String,
    pub staged: bool,
    pub image_diff: Option<ImageDiffView>,
}

/// Push back the idle shutdown: the server only stops once nothing has touched it for a while.
pub fn mark_activity(last_activity: &Mutex<Instant>) {
    if let Ok(mut last_activity) = last_activity.lock() {
        *last_activity = Instant::now();
    }
}

pub fn with_session<T, F>(state: &AppState, session_id: &str, f: F) -> Result<T>
where
    F: FnOnce(&mut RepoSession) -> Result<T>,
{
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow!("state lock poisoned"))?;
    let session = guard
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| anyhow!("unknown session"))?;
    f(session)
}

/// The person a session works for - see [`RepoSession::person`].
pub fn person_of(state: &AppState, session_id: &str) -> Result<Option<crate::unix_users::Person>> {
    with_session(state, session_id, |session| Ok(session.person.clone()))
}

pub fn ensure_session_is_writable(state: &AppState, session_id: &str) -> Result<()> {
    with_session(state, session_id, |session| {
        if session.diff_target.base.is_some() || session.diff_target.comparison.is_some() {
            bail!("this review is read-only");
        }
        Ok(())
    })
}

pub fn lookup_hunk(
    state: &AppState,
    session_id: &str,
    hunk_id: &str,
) -> Result<(PathBuf, String, bool)> {
    with_session(state, session_id, |session| {
        let hunk = crate::git::collect_session_hunks(session)?
            .into_iter()
            .find(|hunk| hunk.id == hunk_id)
            .ok_or_else(|| anyhow!("hunk no longer exists"))?;
        Ok((session.repo_path.clone(), hunk.patch, hunk.staged))
    })
}

pub fn lookup_hunks(
    state: &AppState,
    session_id: &str,
    hunk_ids: &[String],
) -> Result<(PathBuf, Vec<(String, bool)>)> {
    with_session(state, session_id, |session| {
        let hunks = crate::git::collect_session_hunks(session)?;
        let mut patches = Vec::with_capacity(hunk_ids.len());

        for hunk_id in hunk_ids {
            let hunk = hunks
                .iter()
                .find(|hunk| hunk.id == *hunk_id)
                .ok_or_else(|| anyhow!("hunk no longer exists"))?;
            patches.push((hunk.patch.clone(), hunk.staged));
        }

        Ok((session.repo_path.clone(), patches))
    })
}

#[derive(Clone, Default)]
pub struct HunkCommentContext {
    pub file_path: String,
    pub header: String,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub struct AgentAvailability {
    pub claude: bool,
    pub codex: bool,
    pub opencode: bool,
    #[serde(default)]
    pub pi: bool,
    #[serde(default)]
    pub minerve: bool,
}

pub fn stable_id<T: Hash>(value: &T) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn session_id_for_view(
    path: &Path,
    diff_target: &DiffTarget,
    active_commit: Option<&str>,
) -> String {
    stable_id(&(
        path.display().to_string(),
        diff_target.base.clone(),
        diff_target.pathspec.clone(),
        diff_target.comparison.clone(),
        active_commit.map(ToOwned::to_owned),
    ))
}
