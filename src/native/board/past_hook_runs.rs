//! A hook's past runs: off the board once they end, and listed in the hook's editor instead.
//!
//! A schedule that fires every hour would otherwise leave twenty-four ended runs a day in the
//! board's row, and a column hook one more on every card it ever ran on. So a run a hook
//! started is drawn where it was started only while it is going - see
//! [`is_ended_hook_run`] - and the hook's editor lists all of its runs, newest first, in the
//! same rows a card draws its runs in.
//!
//! Nothing is taken off a task: a run kept in a card's `metadata.json` stays there, and a run
//! started by hand is drawn as it always was.

use egui::{RichText, ScrollArea, Ui};

use crate::{
    moontasks::{BoardTaskView, RunsOf, TaskResourceKind, TaskResourceView, TaskView, store},
    native::{
        app::App,
        board::{
            BoardAction,
            day_lines::{LocalDay, minutes_into_local_day},
            gesture::Controls,
            marks::Activity,
            resources::{activity_of, draw_resource},
        },
        theme::{Palette, SMALL_SIZE},
    },
};

/// How tall the list in the editor grows before it scrolls.
const LIST_MAX_HEIGHT: f32 = 190.0;

/// Whether a run is one a hook started that has ended - which the board leaves to the hook's
/// editor rather than drawing where it was started.
pub(crate) fn is_ended_hook_run(resource: &TaskResourceView) -> bool {
    resource.started_by_hook.is_some()
        && matches!(
            resource.kind,
            TaskResourceKind::Agent | TaskResourceKind::Shell
        )
        && activity_of(resource) == Activity::Ended
}

/// One run of a hook, as its editor lists it: the run, and the task it is kept on.
#[derive(Clone)]
pub(crate) struct PastHookRun {
    task_id: String,
    task_title: String,
    repo_path: String,
    run: TaskResourceView,
}

/// Every run the hook called `hook_label` started - a column's heading or a schedule's name,
/// as [`TaskResourceView::started_by_hook`] holds it - on any card or on the board, newest
/// first.
pub(crate) fn collect_runs_of_hook(app: &App, hook_label: &str) -> Vec<PastHookRun> {
    let board = &app.model.board;
    let mut runs: Vec<PastHookRun> = board
        .tasks
        .iter()
        .map(TaskView::runs)
        .chain(board.board_task.iter().map(BoardTaskView::runs))
        .flat_map(|task| {
            task.resources
                .iter()
                .filter(|run| run.started_by_hook.as_deref() == Some(hook_label))
                .map(move |run| PastHookRun {
                    task_id: task.id.to_string(),
                    task_title: task.title.to_string(),
                    repo_path: task.repo_path.to_string(),
                    run: run.clone(),
                })
        })
        .collect();
    runs.sort_by_key(|past| std::cmp::Reverse(past.run.started_at_unix));
    runs
}

/// Whether any run of the hook called `hook_label` is going, which its pill shows with a dot.
pub(crate) fn is_hook_running(app: &App, hook_label: &str) -> bool {
    let board = &app.model.board;
    board
        .tasks
        .iter()
        .flat_map(|task| task.resources.iter())
        .chain(
            board
                .board_task
                .iter()
                .flat_map(|task| task.resources.iter()),
        )
        .any(|run| {
            run.started_by_hook.as_deref() == Some(hook_label)
                && activity_of(run) != Activity::Ended
        })
}

/// `09:15` today, `yesterday 17:15`, `November 12 08:30` before that.
fn describe_when_run_started(unix: u64, today: LocalDay) -> String {
    let minutes = minutes_into_local_day(unix);
    let clock = format!("{:02}:{:02}", minutes / 60, minutes % 60);
    let day = LocalDay::of(unix);
    if day == today {
        clock
    } else {
        format!("{} {clock}", day.label(today))
    }
}

/// The editor's `past sessions`: each run drawn as the card and the board draw a run - its
/// dot, its name, stop or resume, the mark that takes it off - with when it started in the
/// hook's mark. `removing` is the run whose mark has been pressed once.
pub(crate) fn draw_past_sessions(
    ui: &mut Ui,
    runs: &[PastHookRun],
    removing: Option<&str>,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    ui.label(
        RichText::new("PAST SESSIONS")
            .size(SMALL_SIZE - 2.0)
            .color(palette.muted),
    );
    if runs.is_empty() {
        ui.label(
            RichText::new("none yet")
                .size(SMALL_SIZE - 1.0)
                .color(palette.muted),
        );
        return;
    }
    let today = LocalDay::of(store::now_unix());
    // Nothing here is a card, so there is no card for a press to be told apart from.
    let mut controls = Controls::elsewhere();
    ScrollArea::vertical()
        .id_salt("moontask-hook-past-sessions")
        .max_height(LIST_MAX_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for past in runs {
                let task = RunsOf {
                    id: &past.task_id,
                    title: &past.task_title,
                    repo_path: &past.repo_path,
                    resources: std::slice::from_ref(&past.run),
                };
                let when = describe_when_run_started(past.run.started_at_unix, today);
                draw_resource(
                    ui,
                    task,
                    &past.run,
                    &mut controls,
                    removing == Some(past.run.id.as_str()),
                    Some(&when),
                    palette,
                    actions,
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::AgentKind;

    fn run(started_by_hook: Option<&str>, running: bool) -> TaskResourceView {
        TaskResourceView {
            id: "r".to_string(),
            kind: TaskResourceKind::Agent,
            agent: AgentKind::Claude,
            label: "claude".to_string(),
            file_path: None,
            going_elsewhere_in: None,
            terminal_id: running.then(|| "t".to_string()),
            running,
            quiet_for_secs: None,
            attention: None,
            resumable: true,
            started_at_unix: 0,
            started_by: None,
            work_tree: None,
            started_by_hook: started_by_hook.map(str::to_string),
            for_review: None,
        }
    }

    #[test]
    fn only_a_hooks_ended_run_leaves_the_board() {
        assert!(is_ended_hook_run(&run(Some("QA"), false)));
        assert!(!is_ended_hook_run(&run(Some("QA"), true)));
        assert!(!is_ended_hook_run(&run(None, false)));
    }
}
