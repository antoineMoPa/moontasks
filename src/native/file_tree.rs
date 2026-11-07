//! The tree of the repo's files at the left of a file tab, and back / forward through the
//! files the window has shown.
//!
//! One tree per window, shared by every file tab: whether it is shown, how wide, which folders
//! are open. Each tab draws it with its own file marked. Folders are listed as they are opened,
//! through [`crate::backend::Backend::repo_folder`], and listed again now and then while they
//! stay open so a file made by something else shows up. What git ignores is left out unless
//! asked for. The files a review has changed carry the dot and the counts the review's own
//! list gives them. A filter typed over it asks the palette's file search instead, and lays
//! what it finds out as a tree.

mod drawing;
pub(crate) mod history;
mod rows;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use web_time::Instant;

use crate::{
    api::{SearchProgress, SearchScope, SessionPayload, folders::RepoEntry},
    backend::SearchListener,
    native::{
        app::App,
        panes::Pane,
        review::files::{FileStageStatus, build_sidebar_files},
    },
};

use history::{History, Shown};

/// How wide the tree opens before it has been dragged.
const STARTING_WIDTH: f32 = 240.0;
/// Narrower than this and a name of any length is all ellipsis.
const NARROWEST: f32 = 140.0;
/// How long an open folder's listing is trusted before it is asked for again.
const LISTED_FOR: Duration = Duration::from_secs(4);

pub(crate) struct FileTree {
    pub(crate) shown: bool,
    pub(crate) width: f32,
    pub(crate) show_ignored: bool,
    /// What is typed over the tree. Empty: the tree as it is.
    pub(crate) filter: String,
    /// By session, since two file tabs can be on two repos.
    repos: HashMap<String, RepoTree>,
    pub(crate) history: History,
}

impl Default for FileTree {
    fn default() -> Self {
        Self {
            shown: true,
            width: STARTING_WIDTH,
            show_ignored: false,
            filter: String::new(),
            repos: HashMap::new(),
            history: History::default(),
        }
    }
}

#[derive(Default)]
pub(super) struct RepoTree {
    open: HashSet<String>,
    /// By path from the root, `""` for the root itself.
    folders: HashMap<String, Folder>,
    /// The file the tree last unfolded down to, so it does it once as a file comes forward
    /// rather than holding folders open the user has closed.
    revealed: Option<String>,
    /// Set as a file is revealed, until its row has been drawn and scrolled to.
    scroll_to_revealed: bool,
    changes: Changes,
    filtered: Option<Filtered>,
}

#[derive(Default)]
pub(super) struct Folder {
    entries: Option<Vec<RepoEntry>>,
    error: Option<String>,
    asking: bool,
    listed_at: Option<Instant>,
}

impl Folder {
    fn is_stale(&self) -> bool {
        !self.asking
            && self
                .listed_at
                .is_none_or(|listed_at| listed_at.elapsed() >= LISTED_FOR)
    }
}

/// What the review of the repo has changed, as its file list counts it. Worked out again only
/// when the review's payload is a new one.
#[derive(Default)]
struct Changes {
    /// The payload it was worked out from, by address.
    of: usize,
    files: HashMap<String, Change>,
    /// Every folder a changed file is under.
    folders: HashSet<String>,
    added: usize,
    removed: usize,
}

#[derive(Clone, Copy)]
struct Change {
    status: FileStageStatus,
    added: usize,
    removed: usize,
}

impl Changes {
    fn of(payload: Option<&SessionPayload>, of: usize) -> Self {
        let mut changes = Self {
            of,
            ..Self::default()
        };
        // A commit's review lists that commit's files, which say nothing about the tree as
        // it is on disk.
        let Some(payload) = payload.filter(|payload| payload.active_commit.is_none()) else {
            return changes;
        };
        for file in build_sidebar_files(payload) {
            changes.added += file.added_line_count;
            changes.removed += file.removed_line_count;
            changes.folders.extend(rows::ancestors(&file.file_path));
            changes.files.insert(
                file.file_path,
                Change {
                    status: file.status,
                    added: file.added_line_count,
                    removed: file.removed_line_count,
                },
            );
        }
        changes
    }
}

/// The paths a filter found.
struct Filtered {
    query: String,
    show_ignored: bool,
    paths: Option<Vec<String>>,
    /// More matched than the search hands back.
    cut_short: bool,
    error: Option<String>,
}

/// What the file search says, kept to the last report.
#[derive(Default)]
struct Found {
    paths: Vec<String>,
    cut_short: bool,
}

impl SearchListener<String> for Found {
    fn wanted(&mut self) -> bool {
        true
    }

    fn found(&mut self, progress: SearchProgress<String>) {
        self.paths = progress.matches;
        self.cut_short = progress.truncated;
    }
}

/// Every frame: the file in front goes into the history, and the mouse's back and forward
/// buttons move along it over a file tab.
pub(crate) fn follow(app: &mut App, ctx: &egui::Context) {
    let in_front = match app.active_pane() {
        Some((
            _,
            Pane::File {
                session_id,
                file_path,
                revision: None,
                ..
            },
        )) => Some(Shown {
            session_id: session_id.clone(),
            file_path: file_path.clone(),
        }),
        _ => None,
    };
    let over_a_file = in_front.is_some();
    if let Some(shown) = &in_front {
        // Unfolds the tree down to the file the frame it comes forward; the tabs draw it.
        app.reveal_in_tree(&shown.session_id, &shown.file_path);
    }
    app.model.file_tree.history.saw_in_front(in_front);
    if over_a_file {
        let (back, forward) = ctx.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Extra1),
                input.pointer.button_pressed(egui::PointerButton::Extra2),
            )
        });
        if back {
            app.step_through_files(-1);
        } else if forward {
            app.step_through_files(1);
        }
    }
}

#[cfg(test)]
impl FileTree {
    /// What a session's tree holds, for a test to look at: the folders that are open and the
    /// ones that have been listed, each sorted.
    pub(crate) fn probe_for_test(&self, session_id: &str) -> (Vec<String>, Vec<String>) {
        let Some(repo) = self.repos.get(session_id) else {
            return Default::default();
        };
        let mut open: Vec<String> = repo.open.iter().cloned().collect();
        let mut listed: Vec<String> = repo
            .folders
            .iter()
            .filter(|(_, folder)| folder.entries.is_some())
            .map(|(path, _)| path.clone())
            .collect();
        open.sort();
        listed.sort();
        (open, listed)
    }

    /// The names a listed folder holds with whether each is ignored.
    pub(crate) fn entries_for_test(&self, session_id: &str, folder: &str) -> Vec<(String, bool)> {
        self.repos
            .get(session_id)
            .and_then(|repo| repo.folders.get(folder))
            .and_then(|folder| folder.entries.as_ref())
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| (entry.name.clone(), entry.ignored))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl App {
    /// Back (`-1`) or forward (`1`) through the files shown, reopening a file whose tab has
    /// been closed since.
    pub(crate) fn step_through_files(&mut self, by: isize) {
        if let Some(shown) = self.model.file_tree.history.step(by) {
            self.open_file_pane_at(&shown.session_id, &shown.file_path, None);
        }
    }

    pub(crate) fn go_to_file_in_history(&mut self, index: usize) {
        if let Some(shown) = self.model.file_tree.history.go_to(index) {
            self.open_file_pane_at(&shown.session_id, &shown.file_path, None);
        }
    }

    /// Ask for a folder's listing, unless it is already being asked for.
    fn list_tree_folder(&mut self, session_id: &str, folder: &str) {
        let repo = self
            .model
            .file_tree
            .repos
            .entry(session_id.to_string())
            .or_default();
        let listed = repo.folders.entry(folder.to_string()).or_default();
        if listed.asking {
            return;
        }
        listed.asking = true;
        let (asked_session, asked_folder) = (session_id.to_string(), folder.to_string());
        let (session_id, folder) = (session_id.to_string(), folder.to_string());
        self.tasks.spawn(
            move |backend| backend.repo_folder(&asked_session, &asked_folder),
            move |model, result| {
                let Some(repo) = model.file_tree.repos.get_mut(&session_id) else {
                    return;
                };
                let listed = repo.folders.entry(folder).or_default();
                listed.asking = false;
                listed.listed_at = Some(Instant::now());
                match result {
                    Ok(listing) => {
                        listed.entries = Some(listing.entries);
                        listed.error = None;
                    }
                    // A listing from before stays up: a folder that cannot be read for a
                    // moment is better shown as it was than emptied.
                    Err(error) => listed.error = Some(format!("{error}")),
                }
            },
        );
    }

    /// Keep the filter's search on what is typed.
    fn search_tree_filter(&mut self, session_id: &str) {
        let query = self.model.file_tree.filter.trim().to_string();
        let show_ignored = self.model.file_tree.show_ignored;
        let repo = self
            .model
            .file_tree
            .repos
            .entry(session_id.to_string())
            .or_default();
        if query.is_empty() {
            repo.filtered = None;
            return;
        }
        if repo.filtered.as_ref().is_some_and(|filtered| {
            filtered.query == query && filtered.show_ignored == show_ignored
        }) {
            return;
        }
        repo.filtered = Some(Filtered {
            query: query.clone(),
            show_ignored,
            paths: None,
            cut_short: false,
            error: None,
        });
        let asked_session = session_id.to_string();
        let asked_query = query.clone();
        let session_id = session_id.to_string();
        self.tasks.spawn(
            move |backend| {
                let mut found = Found::default();
                backend.find_files(
                    &asked_session,
                    &asked_query,
                    SearchScope::including_ignored(show_ignored),
                    &mut found,
                )?;
                Ok(found)
            },
            move |model, result| {
                let Some(filtered) = model
                    .file_tree
                    .repos
                    .get_mut(&session_id)
                    .and_then(|repo| repo.filtered.as_mut())
                    .filter(|filtered| filtered.query == query)
                else {
                    return;
                };
                match result {
                    Ok(found) => {
                        filtered.paths = Some(found.paths);
                        filtered.cut_short = found.cut_short;
                    }
                    Err(error) => filtered.error = Some(format!("{error}")),
                }
            },
        );
    }

    /// Bring the counts of what the review changed up to its latest payload.
    fn follow_tree_changes(&mut self, session_id: &str) {
        let payload: Option<Arc<SessionPayload>> = self
            .model
            .reviews
            .get(session_id)
            .and_then(|review| review.payload.clone());
        let of = payload
            .as_ref()
            .map_or(0, |payload| Arc::as_ptr(payload) as usize);
        let repo = self
            .model
            .file_tree
            .repos
            .entry(session_id.to_string())
            .or_default();
        if repo.changes.of != of || of == 0 {
            repo.changes = Changes::of(payload.as_deref(), of);
        }
    }

    /// Unfold the tree down to `file_path` and scroll it into view, once as it comes forward.
    fn reveal_in_tree(&mut self, session_id: &str, file_path: &str) {
        let repo = self
            .model
            .file_tree
            .repos
            .entry(session_id.to_string())
            .or_default();
        if repo.revealed.as_deref() == Some(file_path) {
            return;
        }
        repo.revealed = Some(file_path.to_string());
        repo.scroll_to_revealed = true;
        repo.open.extend(rows::ancestors(file_path));
    }
}
