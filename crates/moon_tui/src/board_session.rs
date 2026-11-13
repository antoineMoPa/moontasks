//! This module is the moon this terminal is: a session on the board's repo, holding the agents
//! and shells it starts the way a window holds its own, and answering `moon agent` and
//! `moon wire` from the shells it started over the socket every moon listens on.
//!
//! What it starts ends when it does, as a window's does: an agent's run stays on its card,
//! and `resume` brings it back.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, channel},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use moon_core::{
    api::{AgentKind, AppState, OpenSessionRequest},
    git::project_root,
    instances::{
        self, StartsApplications,
        server::{NobodyElse, ServerAsks},
    },
    moontasks::{
        ColumnEnd, CreateTaskRequest, ReviewRequestView, StartFolder, StartResourceRequest,
        TaskView, service,
        store::{self, BoardColumn, ColumnId, TaskResourceKind},
    },
    sessions,
    terminal::TerminalSession,
};

/// Holds everything the board shows, read at once.
pub struct BoardSnapshot {
    pub columns: Vec<BoardColumn>,
    /// Lists every card, in the order the board draws them - each column reads its own out of
    /// it.
    pub tasks: Vec<TaskView>,
    /// Holds the review requests, read apart and less often - see
    /// [`BoardSession::watch_the_review_requests`] - since they are what is slow to read on a big
    /// board: every request's changed files.
    pub review_requests: Vec<ReviewRequestView>,
}

impl BoardSnapshot {
    /// Returns the cards of one column, in the board's order.
    pub fn cards_in(&self, column: &ColumnId) -> Vec<&TaskView> {
        self.tasks
            .iter()
            .filter(|task| task.status == *column)
            .collect()
    }

    pub fn task(&self, task_id: &str) -> Option<&TaskView> {
        self.tasks.iter().find(|task| task.id == task_id)
    }

    /// Says whether a task has a review file not yet done.
    pub fn has_a_review_waiting(&self, task_id: &str) -> bool {
        self.review_requests
            .iter()
            .any(|request| request.task_id == task_id && request.pending())
    }
}

/// This is the session this terminal holds on the board.
pub struct BoardSession {
    pub state: AppState,
    pub session_id: String,
    pub repo_path: PathBuf,
    /// Holds where the shells this moon started reach it. It is dropped with the session.
    _asks: Option<ServerAsks>,
}

/// A moon in a terminal starts no program with windows: it has no screen to put them on.
struct StartsNoApplications;

impl StartsApplications for StartsNoApplications {
    fn start(&self, _command: &str, _folder: &Path) -> Result<String> {
        bail!("this moon runs in a terminal, which has no screen to open a program's windows on")
    }
}

impl BoardSession {
    /// Opens the board of the repo `folder` is in - the folder this was started in when none is
    /// named.
    pub fn open_the_board_of(folder: Option<PathBuf>) -> Result<Self> {
        let folder = match folder {
            Some(folder) => folder,
            None => {
                std::env::current_dir().context("failed to read the folder moon was started in")?
            }
        };
        let repo_path = project_root(&folder)?;
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let state = AppState::build(last_activity, Some(repo_path.clone()), None);
        let session_id = sessions::open_session(
            &state,
            OpenSessionRequest {
                repo_path: repo_path.display().to_string(),
                diff_target: None,
                active_commit: None,
            },
        )?
        .session_id;
        // It is not fatal: the board works without it, and only `moon agent` and a tagged
        // `moon wire post` typed in this moon's shells need it.
        let asks = ServerAsks::listen(Arc::new(StartsNoApplications))
            .inspect(|asks| asks.shells_held_by(state.clone(), Arc::new(NobodyElse)))
            .ok();
        Ok(Self {
            state,
            session_id,
            repo_path,
            _asks: asks,
        })
    }

    /// Reads the board's columns and cards as they are on disk now, with what this moon has
    /// running on them. The review requests are left empty: they come from
    /// [`Self::watch_the_review_requests`].
    pub fn read_the_board(&self) -> Result<BoardSnapshot> {
        let columns = service::list_columns(&self.state, &self.session_id)?;
        let tasks = service::list_tasks(&self.state, &self.session_id)?;
        Ok(BoardSnapshot {
            columns,
            tasks,
            review_requests: Vec::new(),
        })
    }

    /// Reads the review requests over and over on a thread of their own - each one's changed
    /// files take seconds on a big board, which the keys and the mouse should not wait on -
    /// and sends each reading. The thread ends when the receiver is dropped.
    pub fn watch_the_review_requests(&self, every: Duration) -> Receiver<Vec<ReviewRequestView>> {
        let (send, receive) = channel();
        let (state, session_id) = (self.state.clone(), self.session_id.clone());
        std::thread::spawn(move || {
            loop {
                let requests =
                    service::list_review_requests(&state, &session_id).unwrap_or_default();
                if send.send(requests).is_err() {
                    return;
                }
                std::thread::sleep(every);
            }
        });
        receive
    }

    /// Puts cards at `position` among the others of `column` - which is what dragging them
    /// there, or shift and an arrow, does.
    pub fn place_cards(
        &self,
        task_ids: &[String],
        column: &ColumnId,
        position: usize,
    ) -> Result<()> {
        service::place_tasks(
            &self.state,
            &self.session_id,
            task_ids,
            column.clone(),
            position,
            None,
        )
    }

    /// Puts a column at a place among the others.
    pub fn place_column(&self, column: &ColumnId, position: usize) -> Result<()> {
        service::place_column(&self.state, &self.session_id, column, position)
    }

    /// Makes a card at one end of a column, and says which task it is.
    pub fn create_card(&self, title: &str, column: &ColumnId, joins: ColumnEnd) -> Result<String> {
        let task = service::create_task(
            &self.state,
            &self.session_id,
            &CreateTaskRequest {
                title: title.to_string(),
                status: column.clone(),
                joins,
            },
        )?;
        Ok(task.id)
    }

    pub fn rename_card(&self, task_id: &str, title: &str) -> Result<()> {
        service::rename_task(&self.state, &self.session_id, task_id, title)
    }

    pub fn set_card_tags(&self, task_id: &str, tags: &[String]) -> Result<()> {
        service::set_tags(&self.state, &self.session_id, task_id, tags)
    }

    /// Returns the task's `notes.md`, for `$EDITOR` - made when it is not there yet.
    pub fn notes_file_of(&self, task_id: &str) -> Result<PathBuf> {
        let board = store::board_checkout(&self.repo_path)?;
        let path = store::task_dir(&board, task_id)?.join("notes.md");
        if !path.exists() {
            std::fs::write(&path, "")
                .with_context(|| format!("failed to make {}", path.display()))?;
        }
        Ok(path)
    }

    /// Starts an agent on a task, in the repo, and says which shell it is in.
    pub fn start_agent_on(&self, task_id: &str, agent: AgentKind) -> Result<String> {
        service::start_resource(
            &self.state,
            &self.session_id,
            task_id,
            StartResourceRequest {
                kind: TaskResourceKind::Agent,
                agent,
                opens_in: StartFolder::Repo,
                unattended: false,
            },
        )
    }

    /// Starts a shell of a task, in the repo or in the task's own folder.
    pub fn start_shell_on(&self, task_id: &str, opens_in: StartFolder) -> Result<String> {
        service::start_resource(
            &self.state,
            &self.session_id,
            task_id,
            StartResourceRequest {
                kind: TaskResourceKind::Shell,
                agent: AgentKind::None,
                opens_in,
                unattended: false,
            },
        )
    }

    pub fn stop_run(&self, task_id: &str, run_id: &str) -> Result<()> {
        service::stop_resource(&self.state, &self.session_id, task_id, run_id)
    }

    /// Ends a run if it is going and takes it off its task for good.
    pub fn delete_run(&self, task_id: &str, run_id: &str) -> Result<()> {
        service::delete_resource(&self.state, &self.session_id, task_id, run_id)
    }

    /// Starts a past run again where it left off, and says which shell it is in.
    pub fn resume_run(&self, task_id: &str, run_id: &str) -> Result<String> {
        service::resume_resource(&self.state, &self.session_id, task_id, run_id)
    }

    /// Types a line into a shell and sends it: one of this moon's, or one another moon holds.
    pub fn tell(&self, terminal_id: &str, held_by: Option<u32>, line: &str) -> Result<()> {
        match held_by {
            Some(moon) if moon != std::process::id() => instances::tell(moon, terminal_id, line),
            _ => self.state.terminals.tell(terminal_id, line),
        }
    }

    /// Attaches to one of this moon's shells, to draw and to type into.
    pub fn attach(
        &self,
        terminal_id: &str,
    ) -> Result<(std::sync::mpsc::Receiver<Vec<u8>>, Arc<TerminalSession>)> {
        self.state.terminals.attach(terminal_id)
    }
}
