//! The hooks as the board reads and writes them, through a session like the rest of the
//! board's service.

use anyhow::{Result, bail};

use super::{
    BoardHooks, RemoveHookRequest, SaveHookRequest,
    files::{
        column_hook_path, read_board_hooks, remove_file, review_hook_path, schedule_path,
        write_column_hook, write_review_hook, write_schedule,
    },
    runner::claim_today_if_already_past,
};
use crate::{
    api::AppState,
    moontasks::{service::repo_of as repo_of_session, store},
};

/// Every hook on the session's board.
pub fn list_hooks(state: &AppState, session_id: &str) -> Result<BoardHooks> {
    let repo_path = repo_of_session(state, session_id)?;
    Ok(read_board_hooks(&repo_path))
}

/// Write a hook, in place of the one it replaces.
pub fn save_hook(state: &AppState, session_id: &str, request: SaveHookRequest) -> Result<()> {
    let repo_path = repo_of_session(state, session_id)?;
    match request {
        SaveHookRequest::Column { hook } => {
            if !store::read_board(&repo_path).has(&hook.column) {
                bail!("{} is not a column of this board", hook.column);
            }
            write_column_hook(&repo_path, &hook)
        }
        SaveHookRequest::Review { hook } => write_review_hook(&repo_path, &hook),
        SaveHookRequest::Schedule {
            mut schedule,
            renamed_from,
        } => {
            let name = super::schedule_name_of(&schedule.name);
            if name.is_empty() {
                bail!("a schedule needs a title");
            }
            schedule.name = name;
            let replaces_another = renamed_from.as_deref() != Some(schedule.name.as_str());
            if replaces_another && schedule_path(&repo_path, &schedule.name)?.exists() {
                bail!("there is already a schedule called {}", schedule.name);
            }
            let is_new = renamed_from.is_none();
            write_schedule(&repo_path, &schedule)?;
            if let Some(old_name) = renamed_from.filter(|old| *old != schedule.name) {
                remove_file(&schedule_path(&repo_path, &old_name)?)?;
            }
            if is_new {
                claim_today_if_already_past(&repo_path, &schedule);
            }
            Ok(())
        }
    }
}

/// Take a hook off the board, by taking its file away.
pub fn remove_hook(state: &AppState, session_id: &str, request: RemoveHookRequest) -> Result<()> {
    let repo_path = repo_of_session(state, session_id)?;
    match request {
        RemoveHookRequest::Column { column } => {
            remove_file(&column_hook_path(&repo_path, &column)?)
        }
        RemoveHookRequest::Schedule { name } => remove_file(&schedule_path(&repo_path, &name)?),
        RemoveHookRequest::Review { status } => {
            remove_file(&review_hook_path(&repo_path, &status)?)
        }
    }
}
