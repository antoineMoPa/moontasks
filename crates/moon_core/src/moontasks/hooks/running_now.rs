//! `[run now]`: a hook fired from its editor, as it is typed - saved or not - so its prompt can
//! be tried before it is left to fire by itself.
//!
//! A column's hook runs on the card it is asked to run on, or else the top card of its column;
//! a schedule on the board's own task, with today's date. Nothing is claimed and nothing is
//! counted toward [`super::HOOK_RUNS_IN_A_ROW_CAP`]: a person pressing a button is not a loop.

use std::path::Path;

use anyhow::{Result, anyhow, bail};

use super::{
    HookRunStarted, RunHookNowRequest,
    clock::LocalNow,
    firing::{Arrival, fire_schedule, start_column_hook_run},
    runner::write_log_line,
};
use crate::{
    api::AppState,
    moontasks::{ColumnId, service::repo_of as repo_of_session, store},
};

/// Fire a hook now, once, and answer the run it started so the window can open it.
pub fn run_hook_now(
    state: &AppState,
    session_id: &str,
    request: RunHookNowRequest,
) -> Result<HookRunStarted> {
    let repo_path = repo_of_session(state, session_id)?;
    let (started, said) = match request {
        RunHookNowRequest::Column { hook, task } => {
            let board = store::read_board(&repo_path);
            let label_of = |column: &ColumnId| {
                board
                    .columns
                    .iter()
                    .find(|known| known.id == *column)
                    .map_or_else(|| column.to_string(), |known| known.label.clone())
            };
            let column_label = label_of(&hook.column);
            let task_id = match task {
                Some(task_id) => task_id,
                None => find_top_card_of_column(&repo_path, &hook.column)?
                    .ok_or_else(|| anyhow!("{column_label} has no card to run it on"))?,
            };
            let from_label = store::read_task(&repo_path, &task_id)?
                .status
                .map_or_else(String::new, |column| label_of(&column));
            let arrival = Arrival {
                task_id: &task_id,
                from_label: &from_label,
                column_label: &column_label,
            };
            let terminal_id =
                start_column_hook_run(state, session_id, &repo_path, &hook, &arrival)?;
            let said = format!("{column_label}: run now on {task_id}");
            (
                HookRunStarted {
                    task_id,
                    terminal_id,
                },
                said,
            )
        }
        RunHookNowRequest::Schedule { schedule } => {
            if schedule.name.trim().is_empty() {
                bail!("a schedule needs a title");
            }
            let date = LocalNow::read().date;
            let terminal_id = fire_schedule(state, session_id, &repo_path, &schedule, &date)?;
            let said = format!("{}: run now", schedule.name);
            let task_id = store::BOARD_TASK_ID.to_string();
            (
                HookRunStarted {
                    task_id,
                    terminal_id,
                },
                said,
            )
        }
    };
    write_log_line(&repo_path, &said);
    Ok(started)
}

/// The card at the top of a column: the lowest position, then the oldest. `None` for an empty
/// column.
pub(super) fn find_top_card_of_column(
    repo_path: &Path,
    column: &ColumnId,
) -> Result<Option<String>> {
    let mut top: Option<(u32, u64, String)> = None;
    for task_id in store::list_task_ids(repo_path)? {
        if task_id == store::BOARD_TASK_ID {
            continue;
        }
        let Ok(metadata) = store::read_task(repo_path, &task_id) else {
            continue;
        };
        if metadata.status.as_ref() != Some(column) {
            continue;
        }
        let place = (metadata.position, metadata.created_at_unix, task_id);
        if top.as_ref().is_none_or(|best| place < *best) {
            top = Some(place);
        }
    }
    Ok(top.map(|(_, _, task_id)| task_id))
}
