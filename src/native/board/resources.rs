//! The runs and files a task has, as they are listed under it.
//!
//! The list is drawn twice - down a card, and down the task's own pane - and it is the same
//! list in both, from here: a row is a way back to what it names, and the marks that stop it,
//! resume it, or take it off the task.

use egui::{
    Align, CornerRadius, Layout as UiLayout, Rect, Response, RichText, Sense, Ui, UiBuilder, vec2,
};

use crate::{
    api::AgentKind,
    moontasks::{ReviewRequestView, RunsOf, TaskResourceKind, TaskResourceView},
    native::{
        app::App,
        board::{
            BoardAction, close_button,
            gesture::Controls,
            marks::{Activity, activity_dot, chart_mark, file_mark},
        },
        theme::{Palette, SMALL_SIZE},
        widgets,
    },
};

/// How a task's rows are laid out.
#[derive(Clone, Copy)]
pub(crate) enum Rows {
    /// One under the other, each as wide as what they are listed in: a card, or a task's pane.
    Down,
    /// Side by side, each this wide, in a `Ui` that wraps: the board task, over the columns.
    Across { width: f32 },
}

impl Rows {
    /// Give one row its place, and draw it there.
    fn place(self, ui: &mut Ui, row: impl FnOnce(&mut Ui)) {
        match self {
            Rows::Down => row(ui),
            Rows::Across { width } => {
                ui.allocate_ui(vec2(width, ui.spacing().interact_size.y), row);
            }
        }
    }
}

/// Every run and file the task has, each on its row.
pub(crate) fn draw_list(
    app: &App,
    ui: &mut Ui,
    task: RunsOf<'_>,
    card: &mut Controls,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
    rows: Rows,
) {
    // Read once for the whole list: the mark that takes a run off the task asks first, and the
    // row that asked is the one drawn holding the question.
    let removing = app.model.board.pending_resource_delete.clone();
    // A run a review hook started is drawn on the review's card in the lane instead.
    for resource in task
        .resources
        .iter()
        .filter(|resource| resource.for_review.is_none())
    {
        let pending = removing.as_deref() == Some(resource.id.as_str());
        rows.place(ui, |ui| {
            draw_resource(ui, task, resource, card, pending, palette, actions)
        });
    }

    // Under the runs and the files: one row for all of the task's reviews, which the reviews
    // tab shows - see [`super::review_lane`].
    let requests: Vec<&ReviewRequestView> = app
        .model
        .review_requests
        .iter()
        .filter(|request| request.task_id == task.id)
        .collect();
    if !requests.is_empty() {
        rows.place(ui, |ui| {
            draw_reviews_summary(ui, task, &requests, card, palette, actions)
        });
    }
}

/// The runs a review status's hook started for one review, drawn the way a card draws its
/// runs - so they stop, resume and come off the task the same way. They are the task's runs,
/// listed on the review's card in the lane instead of on the task's.
pub(crate) fn draw_runs_of_review(
    app: &App,
    ui: &mut Ui,
    task: RunsOf<'_>,
    review_id: &str,
    card: &mut Controls,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let removing = app.model.board.pending_resource_delete.clone();
    for resource in task
        .resources
        .iter()
        .filter(|resource| resource.for_review.as_deref() == Some(review_id))
    {
        let pending = removing.as_deref() == Some(resource.id.as_str());
        draw_resource(ui, task, resource, card, pending, palette, actions);
    }
}

/// One row for the task's reviews: how many, and how many are still open. A click shows them in
/// the reviews tab, alone; its menu opens every repo with work still to commit as one diff.
fn draw_reviews_summary(
    ui: &mut Ui,
    task: RunsOf<'_>,
    requests: &[&ReviewRequestView],
    card: &mut Controls,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let open = requests.iter().filter(|request| request.pending()).count();
    let with_work = requests
        .iter()
        .filter(|request| request.has_work_to_commit())
        .count();
    let reviews = match requests.len() {
        1 => "1 review".to_string(),
        count => format!("{count} reviews"),
    };
    // How many are open is the dot's to say: yellow while any is, green once none is.
    let label = reviews;
    let row = draw_row(ui, palette, true, "Show this task's reviews in the reviews tab");
    draw_in_row(ui, row.rect, |ui| {
        // Yellow while one still wants looking at, green once every one is done.
        activity_dot(
            ui,
            match open {
                0 => Activity::Running,
                _ => Activity::Quiet,
            },
            palette,
        );
        ui.add(
            egui::Label::new(RichText::new(&label).color(match open {
                0 => palette.muted,
                _ => palette.ink,
            }))
            .selectable(false),
        );
    });
    if card.pressed(&row) {
        actions.push(BoardAction::ShowReviewsOf(task.id.to_string()));
    }
    egui::Popup::context_menu(&row)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let review_all = match with_work {
                0 => None,
                1 => Some("open its diff".to_string()),
                count => Some(format!("review all {count} repos")),
            };
            match review_all {
                Some(label) => {
                    if widgets::clickable(ui.button(label))
                        .on_hover_text(
                            "Open the changes of every repo with work to commit, one after the \
                             other in one diff",
                        )
                        .clicked()
                    {
                        actions.push(BoardAction::OpenTaskReview {
                            task_id: task.id.to_string(),
                            title: task.title.to_string(),
                        });
                        ui.close();
                    }
                }
                None => {
                    ui.label(
                        RichText::new("nothing left to commit")
                            .size(SMALL_SIZE)
                            .color(palette.muted),
                    );
                }
            }
        });
}

/// How many characters of a linked file's path the card shows before the middle is cut out.
/// The end of a path is what tells files apart, so that is the part that is kept.
const FILE_PATH_CHARS: usize = 34;

/// How long a running agent has to have printed nothing before its dot turns amber. An agent
/// at work redraws its spinner many times a second and a long tool call still ticks; one
/// that has stopped is waiting - on a question it asked, or on the person - or is stuck.
/// Long enough that a pause between two tool calls does not flicker the dot.
const AGENT_QUIET_AFTER_SECS: u64 = 10;

/// What a run's dot says - see [`Activity`]. A shell asking for a person says so over
/// anything else; only an agent run reads as quiet, and only once it has been so for
/// [`AGENT_QUIET_AFTER_SECS`].
pub(crate) fn activity_of(resource: &TaskResourceView) -> Activity {
    // Going in a process that is no shell of this moon: that it is going is all there is to
    // say of it from here.
    if resource.going_elsewhere_in.is_some() {
        return Activity::Running;
    }
    if !resource.running {
        return Activity::Ended;
    }
    if resource.attention.is_some() {
        return Activity::Attention;
    }
    match resource.quiet_for_secs {
        Some(quiet) if quiet >= AGENT_QUIET_AFTER_SECS => Activity::Quiet,
        _ => Activity::Running,
    }
}

/// `2m 05s` for a hover, `45s` under a minute.
fn quiet_text(secs: u64) -> String {
    match (secs / 3600, (secs % 3600) / 60, secs % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m {s:02}s"),
        (h, m, _) => format!("{h}h {m:02}m"),
    }
}

/// One shell, agent run or linked file of a task: what it is, whether it is still going, and
/// the way back to it.
fn draw_resource(
    ui: &mut Ui,
    task: RunsOf<'_>,
    resource: &TaskResourceView,
    card: &mut Controls,
    pending_delete: bool,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    match resource.kind {
        TaskResourceKind::File => {
            draw_file_resource(ui, task, resource, card, palette, actions);
            return;
        }
        TaskResourceKind::Visualization => {
            draw_visualization_resource(ui, task, resource, card, palette, actions);
            return;
        }
        TaskResourceKind::Shell | TaskResourceKind::Agent => {}
    }
    // A running shell is the way back to its tab; a run that has ended opens nothing, and
    // its row says so by staying unlit.
    let opens = resource.running && resource.terminal_id.is_some();
    let row = draw_row(ui, palette, opens, hover_of(resource.kind));
    let row_pressed = card.pressed(&row);
    draw_in_row(ui, row.rect, |ui| {
        let activity = activity_of(resource);
        activity_dot(ui, activity, palette);

        match (&resource.terminal_id, resource.running) {
            (Some(terminal_id), true) => {
                let hover = match (activity, resource.quiet_for_secs, &resource.attention) {
                    (Activity::Attention, _, Some(asking)) => {
                        format!("Open this shell in a tab\n{}", asking.asked.message())
                    }
                    (Activity::Quiet, Some(quiet), _) => format!(
                        "Open this shell in a tab\nnothing printed for {} - waiting on you?",
                        quiet_text(quiet)
                    ),
                    _ => "Open this shell in a tab".to_string(),
                };
                let name = widgets::quiet_button(ui, &resource.label).on_hover_text(hover);
                if card.pressed(&name) || row_pressed {
                    actions.push(BoardAction::OpenShell {
                        terminal_id: terminal_id.clone(),
                        command: (resource.agent != AgentKind::None).then_some(resource.agent),
                        task_id: task.id.to_string(),
                    });
                }
            }
            _ => {
                let name = ui.label(
                    RichText::new(&resource.label)
                        .size(SMALL_SIZE)
                        .color(palette.muted),
                );
                if let Some(pid) = resource.going_elsewhere_in {
                    name.on_hover_text(format!(
                        "Going in process {pid}, which is no shell of this window: another \
                         moon has it, or its agent was started outside one. It is opened \
                         where it was started"
                    ));
                }
            }
        }

        ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
            // Furthest right, so the two that keep the run are never the one you mean to
            // press and miss. Removing a run is not undoable either, so it asks first.
            // A shell has nothing to keep - closing it is the end of it - so it is offered the
            // close mark alone, while an agent run can be stopped and come back to.
            let is_shell = resource.kind == TaskResourceKind::Shell;

            if pending_delete {
                match widgets::confirm(
                    ui,
                    palette,
                    "[really close]",
                    match (is_shell, resource.going_elsewhere_in) {
                        (true, _) => "this ends the shell, and its scrollback goes with it",
                        (false, None) => "this ends the run and takes it off the task for good",
                        (false, Some(_)) => {
                            "this takes the run off the task, and leaves it going where it is"
                        }
                    },
                ) {
                    widgets::Confirmed::Yes => actions.push(BoardAction::DeleteResource(
                        task.id.to_string(),
                        resource.id.clone(),
                    )),
                    widgets::Confirmed::No => actions.push(BoardAction::CancelResourceDelete),
                    widgets::Confirmed::Waiting => {}
                }
                return;
            }
            let close = close_button(ui, palette).on_hover_text(
                match (is_shell, resource.running, resource.going_elsewhere_in) {
                    (true, ..) => "Close this shell",
                    (false, _, Some(_)) => "Take this run off the task, where it goes on going",
                    (false, true, None) => "End this run and take it off the task",
                    (false, false, None) => "Take this run off the task",
                },
            );
            if card.pressed(&close) {
                actions.push(BoardAction::ArmResourceDelete(resource.id.clone()));
            }
            if resource.running && !is_shell {
                let stop = widgets::quiet_button_colored(ui, "stop", palette.muted)
                    .on_hover_text("End this shell, keeping the run to come back to");
                if card.pressed(&stop) {
                    actions.push(BoardAction::Stop(task.id.to_string(), resource.id.clone()));
                }
            } else if resource.resumable {
                let resume = widgets::quiet_button_colored(ui, "resume", palette.accent)
                    .on_hover_text("Start this agent again where it left off");
                if card.pressed(&resume) {
                    actions.push(BoardAction::Resume(
                        task.id.to_string(),
                        resource.id.clone(),
                    ));
                }
            }
            // Last, in whatever room the name and the marks have left between them - and
            // never while the row holds the question of closing it, which returned above.
            super::hooks::draw_hook_mark_on_run(ui, resource, palette);
            draw_started_by(ui, resource, palette);
        });
    });
}

/// The least room a run's row has to have left for it to say whose the run is: under this
/// there is the cut-off mark and nothing of the login before it.
const ROOM_TO_SAY_WHOSE: f32 = 36.0;

/// Whose a run is and the work tree it went in, as its row says them: the login, and the
/// folder's own name when the run did not go in the checkout the board is in. `None` on a run
/// that says neither, which is every run of a server where nobody has a Unix user.
fn started_by_mark(resource: &TaskResourceView) -> Option<String> {
    let whose = resource
        .started_by
        .as_deref()
        .map(|login| format!("@{login}"));
    let where_it_went = resource.work_tree.as_deref().map(|work_tree| {
        let folder = std::path::Path::new(work_tree)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| work_tree.to_string());
        format!("in {folder}")
    });
    let said: Vec<String> = whose.into_iter().chain(where_it_went).collect();
    (!said.is_empty()).then(|| said.join(" "))
}

/// What a run's row says between its name and its marks, in the quiet ink: who started it,
/// and the work tree it went in. Cut off to the room the row has left, with the whole of it
/// on the hover; a row with no room says it on the hover of nothing, and the task's pane has
/// the room.
fn draw_started_by(ui: &mut Ui, resource: &TaskResourceView, palette: &Palette) {
    let Some(mark) = started_by_mark(resource) else {
        return;
    };
    if ui.available_width() < ROOM_TO_SAY_WHOSE {
        return;
    }
    let mut hover = Vec::new();
    if let Some(started_by) = resource.started_by.as_deref() {
        hover.push(format!(
            "Started by {started_by}: it runs as them, and resumes as them whoever asks"
        ));
    }
    if let Some(work_tree) = resource.work_tree.as_deref() {
        hover.push(format!("It ran in the work tree {work_tree}"));
    }
    ui.add(egui::Label::new(RichText::new(mark).size(SMALL_SIZE).color(palette.muted)).truncate())
        .on_hover_text(hover.join("\n"));
}

/// How far a row's fill reaches past its contents on either side.
const ROW_INSET: f32 = 3.0;

/// The row a run, a file or a pending review is listed on, taken before its contents are drawn:
/// lit while the pointer is over it, when a click on it would open something, so what the click
/// will do is plain before it is made. A row with nothing to open stays as it is. The row is a
/// click of its own, on everything the marks at its right do not cover - the marks are drawn
/// after it, so a click on one of them is the mark's and not the row's.
pub(super) fn draw_row(ui: &mut Ui, palette: &Palette, opens: bool, hover: &str) -> Response {
    let (rect, row) = ui.allocate_exact_size(
        vec2(ui.available_width(), ui.spacing().interact_size.y),
        if opens {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if opens && row.hovered() && ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), palette.row_hover_bg);
    }
    if opens {
        row.on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(hover)
    } else {
        row
    }
}

/// What a run's or a file's row says when the pointer rests on it.
fn hover_of(kind: TaskResourceKind) -> &'static str {
    match kind {
        TaskResourceKind::File => "Open this file in a pane",
        TaskResourceKind::Visualization => "Open this visualization in a pane",
        TaskResourceKind::Shell | TaskResourceKind::Agent => "Open this shell in a tab",
    }
}

/// Draw a row's contents inside the space [`draw_row`] took for it, inset from its fill.
pub(super) fn draw_in_row(ui: &mut Ui, rect: Rect, contents: impl FnOnce(&mut Ui)) {
    let inside = rect.shrink2(vec2(ROW_INSET, 0.0));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(inside)
            .layout(UiLayout::left_to_right(Align::Center)),
        contents,
    );
}

/// A file linked to the task: its path, which opens it, and the mark that takes it off the
/// card again.
///
/// Nothing runs here, so there is nothing to stop or resume, and taking the file off the
/// card loses nothing - the file stays where it is - so unlike a run it goes without asking.
fn draw_file_resource(
    ui: &mut Ui,
    task: RunsOf<'_>,
    resource: &TaskResourceView,
    card: &mut Controls,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let Some(file_path) = resource.file_path.as_deref() else {
        panic!("linked file {} has no file path", resource.id);
    };
    let row = draw_row(ui, palette, true, hover_of(resource.kind));
    let row_pressed = card.pressed(&row);
    draw_in_row(ui, row.rect, |ui| {
        file_mark(ui, palette);

        let path = widgets::quiet_button(ui, &widgets::elide_path(file_path, FILE_PATH_CHARS))
            .on_hover_text(format!("Open {file_path} in a pane"));
        if card.pressed(&path) || row_pressed {
            actions.push(BoardAction::OpenFile {
                task_id: task.id.to_string(),
                file_path: file_path.to_string(),
            });
        }

        ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
            let unlink = close_button(ui, palette).on_hover_text("Take this file off the task");
            if card.pressed(&unlink) {
                actions.push(BoardAction::DeleteResource(
                    task.id.to_string(),
                    resource.id.clone(),
                ));
            }
        });
    });
}

/// A visualization a run of the task showed: its name, which opens it again, and the mark that
/// takes it off the card.
///
/// Like a linked file, taking it off loses nothing and so goes without asking: its copy stays in
/// the task's folder. An agent still running that rewrites it puts it back.
fn draw_visualization_resource(
    ui: &mut Ui,
    task: RunsOf<'_>,
    resource: &TaskResourceView,
    card: &mut Controls,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let Some(file_path) = resource.file_path.as_deref() else {
        panic!("visualization {} has no file path", resource.id);
    };
    let row = draw_row(ui, palette, true, hover_of(resource.kind));
    let row_pressed = card.pressed(&row);
    draw_in_row(ui, row.rect, |ui| {
        chart_mark(ui, palette);

        let name = widgets::quiet_button(ui, &resource.label)
            .on_hover_text(format!("Open {file_path} in a pane"));
        if card.pressed(&name) || row_pressed {
            actions.push(BoardAction::OpenVisualization {
                fragment_path: std::path::Path::new(&task.repo_path)
                    .join(file_path)
                    .display()
                    .to_string(),
            });
        }

        ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
            let unlink =
                close_button(ui, palette).on_hover_text("Take this visualization off the task");
            if card.pressed(&unlink) {
                actions.push(BoardAction::DeleteResource(
                    task.id.to_string(),
                    resource.id.clone(),
                ));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(running: bool, quiet_for_secs: Option<u64>) -> TaskResourceView {
        TaskResourceView {
            id: "run".to_string(),
            kind: TaskResourceKind::Agent,
            agent: AgentKind::Claude,
            label: "claude - 1".to_string(),
            file_path: None,
            terminal_id: running.then(|| "terminal-1".to_string()),
            running,
            going_elsewhere_in: None,
            quiet_for_secs,
            attention: None,
            resumable: true,
            started_at_unix: 0,
            started_by: None,
            work_tree: None,
            started_by_hook: None,
            for_review: None,
        }
    }

    /// The dot turns amber only for a run that is going and has been quiet long enough: a
    /// run that ended is hollow however long ago it last printed, and one that only just went
    /// quiet is still green.
    #[test]
    fn a_run_reads_as_quiet_once_it_has_printed_nothing_for_long_enough() {
        assert_eq!(activity_of(&run(true, None)), Activity::Running);
        assert_eq!(activity_of(&run(true, Some(3))), Activity::Running);
        assert_eq!(
            activity_of(&run(true, Some(AGENT_QUIET_AFTER_SECS))),
            Activity::Quiet
        );
        assert_eq!(activity_of(&run(false, Some(600))), Activity::Ended);
        // Asking for a person outranks being quiet, which it usually also is.
        let mut asking = run(true, Some(600));
        asking.attention = Some(crate::api::TerminalAttentionView {
            terminal_id: "terminal-1".to_string(),
            name: None,
            asked: crate::attention::Asked::Notification("Permission needs input".to_string()),
            at_unix: 1,
        });
        assert_eq!(activity_of(&asking), Activity::Attention);
        asking.running = false;
        assert_eq!(activity_of(&asking), Activity::Ended);
    }

    /// Whose a run is and where it went are said together, and not at all by a run of a
    /// server where nobody has a Unix user.
    #[test]
    fn a_run_says_who_started_it_and_the_work_tree_it_went_in() {
        let run = |started_by: Option<&str>, work_tree: Option<&str>| TaskResourceView {
            started_by: started_by.map(str::to_string),
            work_tree: work_tree.map(str::to_string),
            ..run(false, None)
        };

        assert_eq!(started_by_mark(&run(None, None)), None);
        assert_eq!(
            started_by_mark(&run(Some("alice"), None)).as_deref(),
            Some("@alice")
        );
        assert_eq!(
            started_by_mark(&run(Some("alice"), Some("/home/moon-alice/wt"))).as_deref(),
            Some("@alice in wt")
        );
    }

    #[test]
    fn a_quiet_spell_reads_in_the_largest_unit_that_fits() {
        assert_eq!(quiet_text(45), "45s");
        assert_eq!(quiet_text(125), "2m 05s");
        assert_eq!(quiet_text(3900), "1h 05m");
    }

    /// A run going in another moon, or in an agent started outside one, has no shell here
    /// and is going all the same: its dot says so, rather than reading as a run that ended.
    #[test]
    fn a_run_going_in_another_process_reads_as_running() {
        let elsewhere = TaskResourceView {
            going_elsewhere_in: Some(4242),
            ..run(false, None)
        };

        assert_eq!(activity_of(&elsewhere), Activity::Running);
        assert_eq!(activity_of(&run(false, None)), Activity::Ended);
    }
}
