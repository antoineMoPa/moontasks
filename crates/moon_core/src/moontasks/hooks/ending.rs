//! A hook's run closing once it is done: an agent that has finished its turn sits at its box
//! for as long as its shell is open, and a run nobody is at would stay on the board forever.
//!
//! So an agent a hook started is closed once it says it is waiting to be typed at - see
//! [`crate::attention::WAITING_TO_BE_TYPED_AT`] - the way closing its tab would, which leaves
//! its session to be resumed. The run then leaves the board for its hook's `past sessions`.
//!
//! Three runs are left open:
//!
//! - one asking a question or a permission: that is not done, and its dot says so;
//! - one somebody has typed into: it is theirs now, and closes when they close it;
//! - one whose agent never says it is waiting - Codex, or a harness's command that is no
//!   agent of moon's - which ends when its program does.
//!
//! A hook's shell is left open too: nothing of a shell outlives its pty, so closing it would
//! take what its commands printed with it.

use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

use super::runner::write_log_line;
use crate::api::AppState;

/// One agent run a hook started in this moon, for as long as it is open.
struct OpenHookRun {
    repo_path: PathBuf,
    hook_label: String,
    task_id: String,
}

/// The agent runs hooks started in this moon that have not been closed yet, by terminal. A
/// run started again from `past sessions` is not one: somebody asked for it.
static OPEN_HOOK_RUNS: Mutex<BTreeMap<String, OpenHookRun>> = Mutex::new(BTreeMap::new());

/// Take note of an agent run a hook has just started, to close it once it is done.
pub(crate) fn note_hook_agent_run(
    terminal_id: &str,
    repo_path: PathBuf,
    hook_label: &str,
    task_id: &str,
) {
    if let Ok(mut runs) = OPEN_HOOK_RUNS.lock() {
        runs.insert(
            terminal_id.to_string(),
            OpenHookRun {
                repo_path,
                hook_label: hook_label.to_string(),
                task_id: task_id.to_string(),
            },
        );
    }
}

/// Close each hook's run whose agent has finished and that nobody typed into, and forget the
/// ones that ended by themselves.
pub(super) fn close_hook_runs_that_are_done(state: &AppState) {
    let done: Vec<(String, OpenHookRun)> = {
        let Ok(mut runs) = OPEN_HOOK_RUNS.lock() else {
            return;
        };
        runs.retain(|terminal_id, _| state.terminals.is_live(terminal_id));
        let done: Vec<String> = runs
            .keys()
            .filter(|terminal_id| state.terminals.has_agent_finished_untouched(terminal_id))
            .cloned()
            .collect();
        done.into_iter()
            .filter_map(|terminal_id| runs.remove(&terminal_id).map(|run| (terminal_id, run)))
            .collect()
    };
    // Outside the lock: hanging up on an agent waits for it to save its session.
    for (terminal_id, run) in done {
        state.terminals.remove(&terminal_id);
        write_log_line(
            &run.repo_path,
            &format!(
                "{}: done on {}, its run is in past sessions",
                run.hook_label, run.task_id
            ),
        );
    }
}
