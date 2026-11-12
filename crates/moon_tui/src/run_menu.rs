//! This module knows a task's agents and shells: which are listed on it, what each is called,
//! and what the keys do to the one picked on its task. (In the code they are its runs; on
//! screen each is named for what it is - an agent or a shell.)

use moon_core::moontasks::{TaskResourceView, TaskView, store::TaskResourceKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::{App, PromptAsks},
    board_actions, keys,
};

/// Sets the most agents and shells a task lists - the ones the up and down arrows step through;
/// its details list the rest.
pub const RUNS_LISTED: usize = 3;

/// Says whether a run is going: in one of this moon's shells, or in another moon - which a run's
/// `running` alone does not say, being only about this moon's shells.
pub fn is_going(run: &TaskResourceView) -> bool {
    run.running || run.going_elsewhere_in.is_some()
}

/// Returns the runs worth listing on a card: the ones going, and the ones that can be
/// resumed - the going first, then the newest.
pub fn runs_listed_on(task: &TaskView) -> Vec<&TaskResourceView> {
    let mut runs: Vec<&TaskResourceView> = task
        .resources
        .iter()
        .filter(|run| run.terminal_id.is_some() || run.resumable)
        .collect();
    runs.sort_by_key(|run| (!is_going(run), std::cmp::Reverse(run.started_at_unix)));
    runs
}

/// Says what a run is called on its card: its own label without the task's title, which the
/// card already says - `claude - 2`.
pub fn label_on_the_card(run: &TaskResourceView, task_title: &str) -> String {
    run.label
        .strip_prefix(task_title)
        .map(str::trim)
        .filter(|rest| !rest.is_empty())
        .map(|rest| rest.to_lowercase().replace(" - ", " "))
        .unwrap_or_else(|| match run.kind {
            TaskResourceKind::Shell => "shell".to_string(),
            _ => run.agent.label().to_lowercase(),
        })
}

/// Says what a run is, in a word a person reads: an agent or a shell.
pub fn kind_of_the_run(run: &TaskResourceView) -> &'static str {
    match run.kind {
        TaskResourceKind::Shell => "shell",
        _ => "agent",
    }
}

/// Says what a run is called on its tab: what runs in it, and the task.
pub fn name_of_the_run(run: &TaskResourceView, task_title: &str) -> String {
    format!("{} · {task_title}", label_on_the_card(run, task_title))
}

/// Returns the runs listed on a card, which the keys can pick.
pub fn runs_pickable_on(task: &TaskView) -> Vec<TaskResourceView> {
    runs_listed_on(task)
        .into_iter()
        .take(RUNS_LISTED)
        .cloned()
        .collect()
}

/// Handles the keys on a run picked on its card: `enter` views it, `x` stops it, `R` resumes
/// it, `T` tells it. Says whether the key was one of those.
pub fn press_on_the_picked_run(app: &mut App, key: &KeyEvent) -> bool {
    let Some((task_id, task_title, run)) = app.picked_card().and_then(|task| {
        let run = runs_pickable_on(task).into_iter().nth(app.picked_run?)?;
        Some((task.id.clone(), task.title.clone(), run))
    }) else {
        return false;
    };
    match key.code {
        KeyCode::Enter => board_actions::open_the_run(app, &task_id, &run),
        KeyCode::Char(keys::STOP) => {
            if is_going(&run) {
                board_actions::stop_the_run(app, &task_id, &run.id);
                app.flash(format!("stopped {}", name_of_the_run(&run, &task_title)));
            } else {
                app.flash("not running".to_string());
            }
        }
        KeyCode::Char(keys::DELETE) => {
            board_actions::delete_the_run(app, &task_id, &run.id);
            app.picked_run = None;
            app.flash(format!("deleted {}", name_of_the_run(&run, &task_title)));
        }
        KeyCode::Char(keys::RESUME) => {
            let title = name_of_the_run(&run, &task_title);
            board_actions::resume_the_run(app, &task_id, &run.id, &title);
        }
        KeyCode::Char(keys::TELL) => match run.terminal_id.clone() {
            Some(terminal_id) if is_going(&run) => {
                let asks = PromptAsks::Tell {
                    terminal_id,
                    held_by: run.going_elsewhere_in,
                    title: name_of_the_run(&run, &task_title),
                };
                app.ask_for_a_line(asks, "");
            }
            _ => app.flash("not running".to_string()),
        },
        _ => return false,
    }
    true
}
