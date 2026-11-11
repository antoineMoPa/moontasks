//! Opening a session on a repo: what a window, a browser, a shell's `moon agent start` or
//! `moon tui` holds while it works there - the review it is looking at, and the agents and
//! shells it starts, which run in the session's name.

use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::{
    api::{AgentKind, AppState, OpenSessionRequest, RepoSession, SessionOpened},
    git::{project_root, run_git},
};

pub fn open_session(state: &AppState, request: OpenSessionRequest) -> Result<SessionOpened> {
    open_session_for_profile(state, request, None)
}

/// Browser reviews keep selection and comments within the admitted person's profile.
pub fn open_session_for_profile(
    state: &AppState,
    request: OpenSessionRequest,
    owner: Option<crate::api::SessionOwner>,
) -> Result<SessionOpened> {
    let (namespace, person) = match owner {
        Some(owner) => (Some(owner.namespace), owner.person),
        None => (None, None),
    };
    let repo_path = project_root(PathBuf::from(request.repo_path))?;
    let diff_target = request.diff_target.unwrap_or_default();
    let active_commit = request
        .active_commit
        .clone()
        .filter(|commit| !commit.trim().is_empty());
    if let Some(commit) = &active_commit {
        let commit_ref = format!("{commit}^{{commit}}");
        let _ = run_git(&repo_path, &["rev-parse", "--verify", &commit_ref])
            .with_context(|| format!("failed to load commit {commit}"))?;
    }
    let session_id =
        crate::api::session_id_for_view(&repo_path, &diff_target, active_commit.as_deref());

    let session_id = match namespace.as_deref() {
        Some(namespace) => crate::api::stable_id(&(namespace, session_id)),
        None => session_id,
    };

    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow!("state lock poisoned"))?;
    guard.home_repo.get_or_insert_with(|| repo_path.clone());
    match guard.sessions.get_mut(&session_id) {
        Some(session) => {
            session.repo_path = repo_path;
            session.diff_target = diff_target;
            session.active_commit = active_commit;
            session.person = person;
        }
        None => {
            guard.sessions.insert(
                session_id.clone(),
                RepoSession {
                    namespace,
                    person,
                    repo_path,
                    diff_target,
                    active_commit,
                    comments: HashMap::new(),
                    comment_contexts: HashMap::new(),
                    selected_agent: AgentKind::None,
                    comment_dispatches: HashMap::new(),
                    files_named_outside_the_repo: crate::git::FilesNamedOutsideTheRepo::default(),
                },
            );
        }
    }

    Ok(SessionOpened { session_id })
}
