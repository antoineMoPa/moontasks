//! The board a server is on, and which of its tasks a tool was asked about.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    cli::task_shell::TaskOfShell,
    git::project_root,
    moontasks::{TASK_DIR_ENV_VAR, store, wire::handles},
};

/// What a handle may be written with in front of it, as it is on the wire: `@fix-the-races`.
pub(super) const HANDLE_MARK: char = '@';

/// The board of one repo on this machine, as a server started in it sees it.
pub(super) struct Board {
    /// The folder the server was started in, which an agent it starts comes up in: the
    /// board's repo, or a person's own work tree of it.
    pub(super) folder: PathBuf,
    /// The checkout the board's folder of tasks is in.
    pub(super) repo_path: PathBuf,
    /// The folder of the task whose shell the server was started in, when that task is on
    /// this board: the task a tool is about when it names none, and who a post is from.
    pub(super) own_task_dir: Option<OsString>,
}

/// One task of the board, as a tool named it.
pub(super) struct NamedTask {
    /// Its folder's name, which is what the store knows it by.
    pub(super) id: String,
    /// Its handle on this board, which is what is said back.
    pub(super) handle: String,
}

impl Board {
    /// The board of the repo a folder is in, with the task this process was started for
    /// read out of its environment.
    pub(super) fn of_folder(folder: &Path) -> Result<Self> {
        let folder = project_root(folder)
            .with_context(|| format!("{} is no folder to serve the board of", folder.display()))?;
        let repo_path = store::board_checkout(&folder)?;
        Ok(Self::with_own_task(
            folder,
            repo_path,
            crate::moon_env_var_os(TASK_DIR_ENV_VAR),
        ))
    }

    /// A board, keeping the task folder the shell was told only if it is one of this board's:
    /// a server pointed at another repo with `--repo` speaks for no task of it.
    pub(super) fn with_own_task(
        folder: PathBuf,
        repo_path: PathBuf,
        task_dir: Option<OsString>,
    ) -> Self {
        let own_task_dir = task_dir.filter(|task_dir| {
            TaskOfShell::of("moon mcp", Some(task_dir.as_os_str()))
                .is_ok_and(|task| is_the_same_folder(&task.repo_path, &repo_path))
        });
        Self {
            folder,
            repo_path,
            own_task_dir,
        }
    }

    /// The folder's name of every task of the board.
    pub(super) fn list_task_folders(&self) -> Result<Vec<String>> {
        store::list_task_ids(&self.repo_path)
    }

    /// The task a tool is about: the one it named by handle, or this shell's own when it
    /// named none.
    pub(super) fn resolve_task_named(&self, named: Option<&str>) -> Result<NamedTask> {
        let folders = self.list_task_folders()?;
        let id = match named.map(str::trim).filter(|named| !named.is_empty()) {
            Some(named) => {
                let typed = named.strip_prefix(HANDLE_MARK).unwrap_or(named);
                match handles::resolve(typed, &folders) {
                    Ok(id) => id.to_string(),
                    Err(unknown @ handles::Unresolved::Unknown { .. }) => bail!(
                        "{unknown}, {}: tasks_list prints the handles there are",
                        store::tasks_root(&self.repo_path).display()
                    ),
                    Err(ambiguous) => return Err(ambiguous.into()),
                }
            }
            None => self.find_own_task_id(&folders)?,
        };
        Ok(NamedTask {
            handle: handles::handle_of(&id, &folders),
            id,
        })
    }

    fn find_own_task_id(&self, folders: &[String]) -> Result<String> {
        let own = self
            .own_task_dir
            .as_deref()
            .and_then(|task_dir| TaskOfShell::of("moon mcp", Some(task_dir)).ok())
            .map(|task| task.task_id)
            .filter(|task_id| folders.contains(task_id));
        match own {
            Some(task_id) => Ok(task_id),
            None => bail!(
                "which task? This server was not started from a task's shell, so `task` has \
                 to name one by its handle: tasks_list prints them"
            ),
        }
    }
}

/// Whether two paths are one folder, however each was spelled.
fn is_the_same_folder(one: &Path, other: &Path) -> bool {
    let resolved = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    resolved(one) == resolved(other)
}
