//! Moontasks: the sprint board moon runs agents from.
//!
//! [`store`] is the `.moontasks` folder on disk and [`service`] is everything the board
//! do to it.

// The day a card made ahead of time shows up on, which the server reads the board against.
#[cfg(not(target_arch = "wasm32"))]
pub mod appears_on;
pub mod column_sort;
// What the board starts by itself: column hooks and schedules.
pub mod hooks;
pub mod remote_tracker;
pub mod review_request;
pub mod reviews;
pub mod sub_tasks;
// The board's folder is read and written by the server - see `store`.
#[cfg(not(target_arch = "wasm32"))]
pub mod service;
pub mod store;
// How the server starts a task's agents and what it tells them, which a window in a browser
// only ever asks it to do.
#[cfg(not(target_arch = "wasm32"))]
mod task_processes;
// What the agents of a board tell each other, from their shells: posted by a command, and
// kept and delivered by the server and its window.
#[cfg(not(target_arch = "wasm32"))]
pub mod wire;

use serde::{Deserialize, Serialize};

use crate::{api::AgentKind, commit_suggestion::CommitSuggestion};
#[cfg(not(target_arch = "wasm32"))]
pub use reviews::REVIEWS_BRIEF_FILE_NAME;
pub use store::{BoardColumn, ColumnEnd, ColumnId, ColumnSort, TaskResourceKind};
#[cfg(not(target_arch = "wasm32"))]
pub use task_processes::*;

/// One task, as the board draws it.
#[derive(Clone, Serialize, Deserialize)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    pub status: ColumnId,
    pub created_at_unix: u64,
    /// When the card arrived in its column - see [`store::TaskMetadata::entered_column_at_unix`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entered_column_at_unix: Option<u64>,
    /// The task folder itself, so the board can offer it to a shell or a file browser.
    pub dir_path: String,
    /// The folder the task's agents and shells are started in, and that its `review` opens:
    /// the one this window is open on. That is the repo the board belongs to, except for a
    /// person in a work tree of their own on a board several people share, whose board is
    /// the checkout's - see `board_checkout` in the store.
    pub repo_path: String,
    /// What the card is marked with, in the spelling the store keeps - see
    /// [`store::tag_of`].
    #[serde(default)]
    pub tags: Vec<String>,
    /// The address of the task's issue in a tracker kept elsewhere, empty for a task with
    /// none - see [`remote_tracker`].
    #[serde(default)]
    pub remote_task_tracker_url: String,
    /// The card this one is a sub-task of, read off the links in that card's `subtasks`
    /// folder - see [`sub_tasks`].
    #[serde(default)]
    pub parent: Option<String>,
    /// The whole of the task's `notes.md`, empty while nothing has been written in it. The
    /// card draws its first lines as the task's description, and typing there writes it back.
    pub notes: String,
    /// The documents the task's `file_attachments.txt` lists, one per line, as written there:
    /// a path from the task's folder, or an absolute one. The task's pane lists them so a
    /// document is found from the task.
    #[serde(default)]
    pub attachments: Vec<String>,
    /// The files that sit in the task's folder itself, by name, which the task's pane lists
    /// under `Files` beside the attachments - see [`store::read_folder_files`]. Empty on a
    /// card, which lists only the attachments.
    #[serde(default)]
    pub folder_files: Vec<String>,
    pub resources: Vec<TaskResourceView>,
    /// Whether the hooks have stopped starting runs on this card: they have started
    /// [`hooks::HOOK_RUNS_IN_A_ROW_CAP`] in a row with no person moving it - see
    /// [`store::TaskMetadata::hook_runs_in_a_row`]. The card says so.
    #[serde(default)]
    pub hooks_stopped: bool,
}

/// The board task, as the board draws it above its columns: the one task that is on no
/// column, and so has no card - see `create_board_task` in the store. It is what the agents
/// and shells started from the board itself are runs of.
#[derive(Clone, Serialize, Deserialize)]
pub struct BoardTaskView {
    pub id: String,
    pub title: String,
    /// The folder the board task's agents work in - see [`TaskView::repo_path`].
    pub repo_path: String,
    pub resources: Vec<TaskResourceView>,
}

/// What a list of runs and a `[start]` menu are drawn for: a task's card, or the board task.
/// The two are started in, listed and stopped the same way, so they are drawn by the same
/// code, from this.
#[derive(Clone, Copy)]
pub struct RunsOf<'holder> {
    /// The id of the task the runs are recorded on.
    pub id: &'holder str,
    pub title: &'holder str,
    pub repo_path: &'holder str,
    pub resources: &'holder [TaskResourceView],
}

impl TaskView {
    pub fn runs(&self) -> RunsOf<'_> {
        RunsOf {
            id: &self.id,
            title: &self.title,
            repo_path: &self.repo_path,
            resources: &self.resources,
        }
    }
}

impl BoardTaskView {
    pub fn runs(&self) -> RunsOf<'_> {
        RunsOf {
            id: &self.id,
            title: &self.title,
            repo_path: &self.repo_path,
            resources: &self.resources,
        }
    }
}

/// A shell, an agent run or a linked file belonging to a task.
#[derive(Clone, Serialize, Deserialize)]
pub struct TaskResourceView {
    pub id: String,
    pub kind: TaskResourceKind,
    pub agent: AgentKind,
    pub label: String,
    /// The file a linked file opens, relative to the repo root. `Some` for a file, and for a
    /// visualization: its copy in the task's folder - named in full where that folder is not
    /// under [`TaskView::repo_path`], which is a window open on a work tree of the board's
    /// checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// The process a run is going in when that is not a shell of this moon: another moon's -
    /// the window beside a `moon serve` - or the agent's own, started outside any moon and put
    /// on the task afterwards. It is going, and it is not this moon's to open, stop or
    /// resume. `None` for a run that is going here, or not going.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub going_elsewhere_in: Option<u32>,
    /// The shell it is attached to, while it is still running.
    pub terminal_id: Option<String>,
    pub running: bool,
    /// How long the shell behind a running agent run has printed nothing, in whole seconds.
    /// An agent draws a spinner while it works and stops once it is waiting - on the person,
    /// or on a question it asked - so a run quiet for a while is one to look at, and the card
    /// marks it. Nothing for a shell, a file, a run that has ended, or one that has not
    /// printed yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_for_secs: Option<u64>,
    /// What the shell behind it is asking a person for, if anything: a bell, or the
    /// notification an agent sends when it is waiting on a question or a permission - see
    /// [`crate::attention`]. The card marks it, and typing into the shell takes it off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<crate::api::TerminalAttentionView>,
    /// Whether the run can be started again where it left off, which needs the agent to have
    /// been told its session id when it started.
    pub resumable: bool,
    pub started_at_unix: u64,
    /// The GitHub login of whoever started the run, on a server that gives each person a Unix
    /// user: the run is theirs - it runs as them and is resumed as them, whoever asks - and
    /// the card says so. `None` everywhere else, and for a shell or a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<String>,
    /// The git work tree the run was started in, by its path, for the card to say: on a board
    /// several people share, a person may work in a work tree of their own - see
    /// `board_checkout` in the store. `None` for a run in the checkout the board is in, and
    /// for a shell or a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_tree: Option<String>,
    /// The hook that started the run, by the heading of its column or the name of its
    /// schedule - see [`hooks`]. `None` for a run a person started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by_hook: Option<String>,
    /// The review file a review hook started the run for - drawn on that review's card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub for_review: Option<String>,
}

/// One review file of the board, as the board draws it - see [`reviews`]. They arrive in the
/// lane's order: status by status, and within a status in deploy order.
#[derive(Clone, Serialize, Deserialize)]
pub struct ReviewRequestView {
    /// The card the review was asked for on, or empty for one that names none.
    pub task_id: String,
    /// Its place in the list, which tells two rows apart.
    pub index: usize,
    /// The repo as the file named it - `repos/turbocharger`. Empty for the board's own repo.
    pub path_under_repo: String,
    /// The working copy the review opens: the repo, or the worktree its branch is checked out
    /// in. The same string the submodule hub carries for the same repo.
    pub repo_path: String,
    /// The repo's directory name, which is what the row says.
    pub name: String,
    /// The branch the commit belongs on, if the file named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The commit message the file holds, which the repo's commit pane offers instead of
    /// starting an agent to write one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<CommitSuggestion>,
    /// How many files are changed where this is reviewed.
    pub changed_files: usize,
    /// Whether a commit with the file's subject is one of the last on the branch it is reviewed
    /// on: work done in the repo since is another commit.
    #[serde(default)]
    pub committed: bool,
    /// When the file was last written, in seconds since the epoch. Several reviews can name the
    /// same repo, and the one written last is the one whose work is in the repo now.
    #[serde(default)]
    pub written_at_unix: u64,
    /// Which file, and where its review stands.
    pub file: reviews::ReviewFileView,
}

impl ReviewRequestView {
    /// Whether the review still wants looking at: its file is not in the done folder yet.
    pub fn pending(&self) -> bool {
        self.file.status != reviews::DONE_REVIEW_STATUS
    }

    /// Whether there is still work to commit for it: it is pending, its commit has not been
    /// made, and its working copy has changes - which is what a diff of it has to show.
    pub fn has_work_to_commit(&self) -> bool {
        self.pending() && !self.committed && self.changed_files > 0
    }
}

/// What starting a task's resource asked for.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct StartResourceRequest {
    pub kind: TaskResourceKind,
    /// Which agent to run, for an agent resource.
    pub agent: AgentKind,
    /// The folder it comes up in.
    pub opens_in: StartFolder,
    /// Whether nobody is at the window for the agent as it comes up, which is what
    /// `moon agent start` says. An agent asking whether it trusts a folder it has not been run
    /// in is then told that it does - see `trust` in [`crate::terminal`] - since starting it
    /// there is what said so, and the Enter after the line it is told next would otherwise be
    /// the answer.
    #[serde(default)]
    pub unattended: bool,
}

/// Which folder a task's shell or agent comes up in.
///
/// An agent is always started in the repo: the work is there, and the task folder is what the
/// brief points it at. A shell is offered both, because a task folder is a place you read -
/// the notes, the brief, whatever the agent left in it.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartFolder {
    /// The repo the window is open on: the one the board is over, or a person's own work tree
    /// of it.
    Repo,
    /// The task's own folder under `.moontasks`.
    TaskFolder,
}

/// A session an agent already has, being put on a task as a new resource.
///
/// This is the way back when a task's recorded session id stopped pointing anywhere - the
/// user switched sessions inside the agent, or the agent never persisted the one it was
/// started on. The id here is one read off the agent's own records, so it is known to exist.
#[derive(Clone, Serialize, Deserialize)]
pub struct AttachResourceRequest {
    pub agent: AgentKind,
    pub agent_session_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    /// The column the new card joins - the one whose `+` opened the pane the title was
    /// written on.
    pub status: ColumnId,
    /// Which end of that column it joins, which is the `+` that was pressed: the one on the
    /// heading puts the card on top, the one under the last card puts it at the bottom.
    pub joins: ColumnEnd,
}

// Read by the server's routes only: a remote window writes this and the next as `json!`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize, Deserialize)]
pub struct TaskTitleRequest {
    pub title: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize, Deserialize)]
pub struct TaskRemoteTrackerRequest {
    pub url: String,
}

/// What a card is marked with, set whole rather than one tag at a time: the tag menu knows the
/// list it wants when it closes, and sending that is one write instead of a diff.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize, Deserialize)]
pub struct TaskTagsRequest {
    pub tags: Vec<String>,
}

/// A file of the repo being put on a task's card, by the path the file pane opens it with:
/// relative to the repo root.
#[derive(Serialize, Deserialize)]
pub struct LinkFileRequest {
    pub file_path: String,
}

/// A document being taken off a task's `file_attachments.txt`, by its line as the board lists
/// it - see [`TaskView::attachments`].
#[derive(Serialize, Deserialize)]
pub struct RemoveAttachmentRequest {
    pub listed: String,
}

/// The answer to opening a task's notes: where the file pane finds the file, relative to the
/// repo root, which is how every file pane path is addressed.
#[derive(Serialize, Deserialize)]
pub struct TaskNotesPayload {
    pub file_path: String,
}

/// Where the work log is, as the file pane addresses it - the answer to opening it, which
/// makes the file when the project has none yet.
#[derive(Serialize, Deserialize)]
pub struct WorkLogPayload {
    pub file_path: String,
}

/// Where the wire's broadcasts are kept, as the file pane addresses the file - the answer to
/// opening it, which makes the file on a board nobody has posted to yet.
#[derive(Serialize, Deserialize)]
pub struct WirePayload {
    pub file_path: String,
}

/// Where dragged cards were let go of: which cards, the column, and how many of that
/// column's other cards are above them. More than one card is a drag made with a selection.
#[derive(Serialize, Deserialize)]
pub struct TaskPlacementRequest {
    pub task_ids: Vec<String>,
    pub status: ColumnId,
    pub position: usize,
    /// The card they were dropped onto, or among the sub-tasks of, which they become
    /// sub-tasks of - see [`sub_tasks`].
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TerminalOpened {
    pub terminal_id: String,
}

/// A column being renamed: what it is to be called.
#[derive(Serialize, Deserialize)]
pub struct ColumnLabelRequest {
    pub label: String,
}

/// A column being added: what it is called, and where among the others it goes.
#[derive(Serialize, Deserialize)]
pub struct NewColumnRequest {
    pub label: String,
    /// How many columns are to its left. Nothing puts it at the right-hand end, which is
    /// where the board's own `+` adds one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<usize>,
}

/// Which end of a column cards moved into it go to, or nothing for wherever they were dropped.
#[derive(Serialize, Deserialize)]
pub struct ColumnArrivalsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrivals: Option<ColumnEnd>,
}

/// The order a column keeps its cards in by itself, or nothing for the order they are dragged
/// into.
#[derive(Serialize, Deserialize)]
pub struct ColumnSortRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<ColumnSort>,
}

/// Whether a column draws a line under a day's work of its cards.
#[derive(Serialize, Deserialize)]
pub struct ColumnDaysWorkRequest {
    pub marks_a_days_work: bool,
}

/// Where a dragged column was let go of: how many of the other columns are to its left.
#[derive(Serialize, Deserialize)]
pub struct ColumnPlacementRequest {
    pub position: usize,
}
