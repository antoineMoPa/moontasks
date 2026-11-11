//! This module shows a card's runs: listed on the card itself, and - with `o` - in a menu to
//! open one, stop it, resume it or tell it a line. This is where the board's agents are, card by
//! card.

use moon_core::moontasks::{TaskResourceView, TaskView, store::TaskResourceKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::{App, Overlay, PromptAsks},
    board_actions,
};

/// Sets the most runs a card lists - the ones the up and down arrows step through; `o` shows the rest.
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
        .map(str::to_lowercase)
        .unwrap_or_else(|| match run.kind {
            TaskResourceKind::Shell => "shell".to_string(),
            _ => run.agent.label().to_lowercase(),
        })
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
        KeyCode::Char('x') => {
            if is_going(&run) {
                board_actions::stop_the_run(app, &task_id, &run.id);
                app.flash(format!("stopped {}", name_of_the_run(&run, &task_title)));
            } else {
                app.flash("it is not going: R resumes it".to_string());
            }
        }
        KeyCode::Char('R') => {
            let title = name_of_the_run(&run, &task_title);
            board_actions::resume_the_run(app, &task_id, &run.id, &title);
        }
        KeyCode::Char('T') => match run.terminal_id.clone() {
            Some(terminal_id) if is_going(&run) => {
                let asks = PromptAsks::Tell {
                    terminal_id,
                    held_by: run.going_elsewhere_in,
                    title: name_of_the_run(&run, &task_title),
                };
                app.ask_for_a_line(asks, "");
            }
            _ => app.flash("it is not going: R resumes it".to_string()),
        },
        _ => return false,
    }
    true
}

/// Opens, on `o`, a card's one run, or a menu of them.
pub fn open_the_runs_of_the_picked_card(app: &mut App) {
    let Some(task) = app.picked_card() else {
        return;
    };
    let task_id = task.id.clone();
    let runs: Vec<TaskResourceView> = runs_listed_on(task).into_iter().cloned().collect();
    match runs.as_slice() {
        [] => app.flash("nothing has run on this card: s starts something".to_string()),
        [run] => board_actions::open_the_run(app, &task_id, run),
        _ => app.overlay = Some(Overlay::RunMenu { task_id, index: 0 }),
    }
}

pub fn press_on_the_run_menu(app: &mut App, task_id: String, index: usize, key: KeyEvent) {
    let Some(task) = app.board.task(&task_id) else {
        return;
    };
    let task_title = task.title.clone();
    let runs: Vec<TaskResourceView> = runs_listed_on(task).into_iter().cloned().collect();
    if runs.is_empty() {
        return;
    }
    let index = index.min(runs.len() - 1);
    let run = &runs[index];
    let stay = |app: &mut App, index: usize| {
        app.overlay = Some(Overlay::RunMenu {
            task_id: task_id.clone(),
            index,
        })
    };
    match key.code {
        KeyCode::Down => stay(app, (index + 1) % runs.len()),
        KeyCode::Up => stay(app, (index + runs.len() - 1) % runs.len()),
        KeyCode::Enter => board_actions::open_the_run(app, &task_id, run),
        KeyCode::Char('x') => {
            board_actions::stop_the_run(app, &task_id, &run.id);
            stay(app, index);
        }
        KeyCode::Char('R') => {
            let title = name_of_the_run(run, &task_title);
            board_actions::resume_the_run(app, &task_id, &run.id, &title);
        }
        KeyCode::Char('T') => match run.terminal_id.clone() {
            Some(terminal_id) if is_going(run) => {
                let asks = PromptAsks::Tell {
                    terminal_id,
                    held_by: run.going_elsewhere_in,
                    title: name_of_the_run(run, &task_title),
                };
                app.ask_for_a_line(asks, "");
            }
            _ => {
                app.flash("that run is not going: R resumes it".to_string());
                stay(app, index);
            }
        },
        KeyCode::Esc | KeyCode::Char('q') => {}
        _ => stay(app, index),
    }
}
