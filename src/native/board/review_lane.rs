//! The lane of review files, in a tab beside the board's: a column per review status, a card per
//! pull request - see [`crate::moontasks::reviews`].
//!
//! A card is dragged to another status to move its file there, which is what fires that
//! status's hook; the pill at the right of a status's heading is its hook. The cards of the
//! marked task are lit and the others step back, so a task's pull requests can be found across
//! the lane at a glance.

use egui::{
    Align, CornerRadius, Layout as UiLayout, RichText, ScrollArea, Sense, Stroke, Ui, vec2,
};

use crate::{
    moontasks::{
        ReviewRequestView,
        hooks::start_word,
        reviews::{DEFAULT_REVIEW_STATUSES, ReviewFileView, describe_pull_request},
    },
    native::{
        app::App,
        board::{
            BoardAction, COLUMN_WIDTH,
            hook_editor::{HookEditorTarget, open_hook_editor},
            hooks::{PillKind, draw_hook_pill},
            resources::{ReviewLink, choose_review_link},
            selection,
        },
        theme::{Palette, SMALL_SIZE},
        widgets,
    },
};

/// How opaque the cards of other tasks are while a task is marked.
const OTHER_TASKS_OPACITY: f32 = 0.35;

/// A review card being carried to another status.
#[derive(Clone)]
struct DraggedReview {
    id: String,
    status: String,
    label: String,
}

/// The rows of the board's review files.
fn review_rows(app: &App) -> Vec<(ReviewRequestView, ReviewFileView)> {
    let focus = app.model.board.review_focus.as_deref();
    app.model
        .review_requests
        .iter()
        .filter(|request| focus.is_none_or(|task_id| request.task_id == task_id))
        .map(|request| (request.clone(), request.file.clone()))
        .collect()
}

/// The statuses the lane draws, in order: the board's, or the defaults until it has been read.
fn lane_statuses(app: &App) -> Vec<String> {
    let mut statuses = match app.model.review_statuses.is_empty() {
        true => DEFAULT_REVIEW_STATUSES
            .iter()
            .map(|status| status.to_string())
            .collect(),
        false => app.model.review_statuses.clone(),
    };
    // A file moved into a folder since the last read of the statuses still has a column.
    for (_, file) in review_rows(app) {
        if !statuses.contains(&file.status) {
            statuses.push(file.status);
        }
    }
    statuses
}

/// The lane, filling its frame: a column per status, side by side.
pub(super) fn draw(app: &mut App, ui: &mut Ui, palette: &Palette, actions: &mut Vec<BoardAction>) {
    let_go_of_the_focus_on_escape(app, ui);
    let rows = review_rows(app);
    let height = ui.available_height();
    ScrollArea::horizontal()
        .id_salt("moontasks-review-lane")
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for status in lane_statuses(app) {
                    draw_status_column(app, ui, &status, &rows, height, palette, actions);
                    ui.add_space(6.0);
                }
            });
        });
    draw_carried_review(ui);
}

/// Escape over the tab shows every task's reviews again, after a task's reviews row put it on
/// that task's alone. Nothing is drawn for it: the task's row on the board is what got here.
fn let_go_of_the_focus_on_escape(app: &mut App, ui: &mut Ui) {
    if app.model.board.review_focus.is_none() || ui.ctx().text_edit_focused() {
        return;
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.model.board.review_focus = None;
    }
}

/// One status: its heading and hook, and its cards - a place a carried card can be dropped.
fn draw_status_column(
    app: &mut App,
    ui: &mut Ui,
    status: &str,
    rows: &[(ReviewRequestView, ReviewFileView)],
    height: f32,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let in_status: Vec<&(ReviewRequestView, ReviewFileView)> = rows
        .iter()
        .filter(|(_, file)| file.status == status)
        .collect();
    let carried = egui::DragAndDrop::payload::<DraggedReview>(ui.ctx());

    let column = ui.allocate_ui_with_layout(
        vec2(COLUMN_WIDTH, height),
        UiLayout::top_down(Align::Min),
        |ui| {
            ui.set_width(COLUMN_WIDTH);
            ui.set_min_height(height);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(status.to_uppercase())
                        .size(SMALL_SIZE)
                        .strong()
                        .color(palette.ink),
                );
                ui.label(
                    RichText::new(in_status.len().to_string())
                        .size(SMALL_SIZE - 1.0)
                        .color(palette.muted),
                );
                ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                    draw_review_hook_pill(app, ui, status, palette);
                });
            });
            ui.add_space(3.0);
            ScrollArea::vertical()
                .id_salt(("moontasks-review-status", status))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 5.0;
                    for (request, file) in &in_status {
                        draw_review_card(app, ui, request, file, palette, actions);
                    }
                });
        },
    );

    let rect = column.response.rect;
    let pointer = ui.ctx().pointer_interact_pos();
    let over = carried.is_some() && pointer.is_some_and(|pointer| rect.contains(pointer));
    if over {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(8),
            Stroke::new(1.0, palette.accent),
            egui::StrokeKind::Outside,
        );
        if ui.input(|input| input.pointer.any_released())
            && let Some(dropped) = egui::DragAndDrop::take_payload::<DraggedReview>(ui.ctx())
        {
            if dropped.status != status {
                actions.push(BoardAction::MoveReview {
                    review_id: dropped.id.clone(),
                    from_status: dropped.status.clone(),
                    to_status: Some(status.to_string()),
                });
            }
        }
    }
}

/// The status's hook, or the faint `+` that makes one.
fn draw_review_hook_pill(app: &mut App, ui: &mut Ui, status: &str, palette: &Palette) {
    let target = HookEditorTarget::Review(status.to_string());
    let editing = app
        .model
        .board
        .hook_editor
        .as_ref()
        .is_some_and(|editor| editor.target == target);
    let id = review_hook_pill_id(status);
    let pill = match app.model.board.hooks.of_review_status(status).cloned() {
        Some(hook) => draw_hook_pill(
            ui,
            id,
            PillKind::Column,
            start_word(hook.start),
            editing,
            palette,
        )
        .on_hover_text(format!(
            "When a review arrives in {status}, start {} on its card. Click to change",
            start_word(hook.start)
        )),
        None => draw_hook_pill(ui, id, PillKind::Ghost, "+", editing, palette).on_hover_text(
            format!("Start an agent on every review that arrives in {status}"),
        ),
    };
    if pill.clicked() {
        open_hook_editor(app, target, pill.rect);
    }
}

/// What a status's pill answers to - which is how a test finds it.
pub(crate) fn review_hook_pill_id(status: &str) -> egui::Id {
    egui::Id::new(("moontask-review-hook-pill", status))
}

/// What a review's card answers to - which is how a test finds it. Two files can share a name
/// in two status folders, so the status is part of it: one id per card, never two cards on one.
pub(crate) fn review_card_id(status: &str, review_id: &str) -> egui::Id {
    egui::Id::new(("moontask-review-card", status, review_id))
}

/// One pull request: its repo and number, where its code stands, and the task it is for.
fn draw_review_card(
    app: &App,
    ui: &mut Ui,
    request: &ReviewRequestView,
    file: &ReviewFileView,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let board = &app.model.board;
    let marked = !request.task_id.is_empty() && selection::is_marked(board, &request.task_id);
    // Only the other tasks' reviews say a task is marked, by stepping back.
    let focused = board.review_focus.is_some();
    let another_is_marked = !focused && !board.marked.is_empty() && !marked;
    let task_title = board
        .tasks
        .iter()
        .find(|task| task.id == request.task_id)
        .map(|task| task.title.clone());
    let pull_request = match &file.commit {
        Some(commit) => format!(
            "{} @{}",
            describe_pull_request(file.pr.as_deref(), None),
            &commit[..commit.len().min(7)]
        )
        .trim()
        .to_string(),
        None => describe_pull_request(file.pr.as_deref(), request.branch.as_deref()),
    };

    let card_id = review_card_id(&file.status, &file.id);
    // egui gives the drag half of a press on a button to the card under it, so the card would
    // read a click on [pr] or × as its own click too: a button under the pointer keeps it.
    let mut on_a_button = false;
    // The card is the press, sensed under what is drawn in it, so a run's row inside it is
    // still a button of its own.
    let drawn = ui.scope_builder(
        egui::UiBuilder::new()
            .id(card_id)
            .sense(Sense::click_and_drag()),
        |ui| {
        // Its words are the card's to be dragged by, not text to select.
        ui.style_mut().interaction.selectable_labels = false;
        if another_is_marked {
            ui.multiply_opacity(OTHER_TASKS_OPACITY);
        }
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.line))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin {
                left: 8,
                right: 8,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    // The board's own repo goes without saying; a submodule is named. A review
                    // of the board's repo naming nothing else is named after it all the same.
                    if !request.path_under_repo.is_empty() || pull_request.is_empty() {
                        ui.label(
                            RichText::new(&request.name)
                                .size(SMALL_SIZE + 1.0)
                                .color(palette.ink),
                        );
                    }
                    if !pull_request.is_empty() {
                        ui.label(
                            RichText::new(&pull_request)
                                .size(SMALL_SIZE)
                                .color(palette.accent_2),
                        );
                    }
                    // A click on the card opens what the row on its task's card opens; these
                    // open each one whatever the review's status.
                    ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                        let dismiss = draw_dismiss_mark(ui, file, palette);
                        on_a_button |= dismiss.contains_pointer();
                        if dismiss.clicked() {
                            actions.push(dismiss_review(file));
                        }
                        if let Some(url) = &file.pr_url {
                            let pr = draw_card_button(ui, "[pr]")
                                .on_hover_text(format!("Open {url}"));
                            on_a_button |= pr.contains_pointer();
                            if pr.clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            }
                        }
                        let review = draw_card_button(ui, "[moonreview]").on_hover_text(
                            match &file.commit {
                                Some(_) => "Open the diff of this commit",
                                None => "Open the changes waiting where this is reviewed",
                            },
                        );
                        on_a_button |= review.contains_pointer();
                        if review.clicked() {
                            actions.push(BoardAction::OpenReviewOf(request.clone()));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                        if let Some(deploy) = &file.deploy {
                            ui.label(RichText::new(deploy).size(SMALL_SIZE - 1.0).color(
                                match deploy.as_str() {
                                    "deployed" => palette.added,
                                    "merged" => palette.accent_2,
                                    _ => palette.muted,
                                },
                            ));
                        }
                        // What is left to commit where it is reviewed - a commit review has
                        // nothing of the kind.
                        if file.commit.is_none() && request.has_work_to_commit() {
                            ui.label(
                                RichText::new(crate::native::submodules::changes_label(
                                    request.changed_files,
                                ))
                                .size(SMALL_SIZE - 1.0)
                                .color(palette.muted),
                            );
                        }
                        if let Some(subject) = request.suggestion.as_ref().map(|s| &s.subject) {
                            ui.with_layout(UiLayout::left_to_right(Align::Center), |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(subject)
                                            .size(SMALL_SIZE)
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            });
                        }
                    });
                });
                // While one task's reviews show alone, whose they are goes without saying.
                let task_line = match (&task_title, request.task_id.is_empty()) {
                    (Some(title), _) => format!("↳ {}", widgets::elide_end(title, 44)),
                    (None, true) => "↳ no task".to_string(),
                    (None, false) => format!("↳ {}", request.task_id),
                };
                if !focused {
                    ui.label(
                        RichText::new(task_line)
                            .size(SMALL_SIZE - 1.0)
                            .color(palette.muted),
                    );
                }
                draw_hook_runs_of_review(app, ui, request, file, palette, actions);
            });
        },
    );
    let card = drawn
        .response
        .on_hover_text(hover_of_review(request, file));
    if was_card_clicked_in_place(ui, &card) && !on_a_button {
        // A press that came back where it started is a click, even after it began a drag.
        egui::DragAndDrop::clear_payload(ui.ctx());
        match choose_review_link(request) {
            ReviewLink::PullRequest(url) => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
            ReviewLink::MoonReview => actions.push(BoardAction::OpenReviewOf(request.clone())),
        }
    } else if card.drag_started() {
        egui::DragAndDrop::set_payload(
            ui.ctx(),
            DraggedReview {
                id: file.id.clone(),
                status: file.status.clone(),
                label: format!("{} {pull_request}", request.name),
            },
        );
    }

    egui::Popup::context_menu(&card)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            for status in lane_statuses(app) {
                if status == file.status {
                    continue;
                }
                if widgets::clickable(ui.button(format!("move to {status}"))).clicked() {
                    actions.push(BoardAction::MoveReview {
                        review_id: file.id.clone(),
                        from_status: file.status.clone(),
                        to_status: Some(status),
                    });
                    ui.close();
                }
            }
            ui.separator();
            if widgets::clickable(ui.button("dismiss"))
                .on_hover_text(dismiss_hover_of(file))
                .clicked()
            {
                actions.push(dismiss_review(file));
                ui.close();
            }
        });
}

/// How far the pointer may go between pressing a card and letting go for that to be a click
/// that opens its review rather than a drag.
const CARD_CLICK_SLOP: f32 = 3.0;

/// Whether the card was let go of no further than [`CARD_CLICK_SLOP`] from where it was
/// pressed. egui forgets where a press began on the frame it ends, so the card keeps it.
fn was_card_clicked_in_place(ui: &Ui, card: &egui::Response) -> bool {
    let origin_id = card.id.with("press-origin");
    if card.is_pointer_button_down_on() {
        if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
            ui.data_mut(|data| data.insert_temp(origin_id, origin));
        }
        return false;
    }
    if !(card.clicked() || card.drag_stopped()) {
        return false;
    }
    let origin = ui.data_mut(|data| data.remove_temp::<egui::Pos2>(origin_id));
    let released = ui.input(|input| input.pointer.interact_pos());
    match (origin, released) {
        (Some(origin), Some(released)) => origin.distance(released) <= CARD_CLICK_SLOP,
        _ => false,
    }
}

/// The runs a review status's hook started for this review, inside its card - the same rows a
/// task's card has, with the same marks to stop, resume and remove them.
fn draw_hook_runs_of_review(
    app: &App,
    ui: &mut Ui,
    request: &ReviewRequestView,
    file: &ReviewFileView,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let Some(task) = app
        .model
        .board
        .tasks
        .iter()
        .find(|task| task.id == request.task_id)
    else {
        return;
    };
    if !task
        .resources
        .iter()
        .any(|run| run.for_review.as_deref() == Some(file.id.as_str()))
    {
        return;
    }
    ui.add_space(3.0);
    let mut card = super::gesture::Controls::new(ui);
    super::resources::draw_runs_of_review(
        app,
        ui,
        task.runs(),
        &file.id,
        &mut card,
        palette,
        actions,
    );
}

/// One of a review card's bracketed buttons, in the card's small type.
/// Dismiss a review: take its file away, from the folder it is in - not a namesake's.
pub(crate) fn dismiss_review(file: &ReviewFileView) -> BoardAction {
    BoardAction::MoveReview {
        review_id: file.id.clone(),
        from_status: file.status.clone(),
        to_status: None,
    }
}

/// The × that dismisses a review, on its card in the lane and its row on its task's card.
pub(crate) fn draw_dismiss_mark(
    ui: &mut Ui,
    file: &ReviewFileView,
    palette: &Palette,
) -> egui::Response {
    let mark = widgets::close_button(ui, palette);
    mark.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, DISMISS_MARK_LABEL)
    });
    mark.on_hover_text(dismiss_hover_of(file))
}

/// What the dismiss mark is called to a screen reader - and to a test looking for it.
pub(crate) const DISMISS_MARK_LABEL: &str = "dismiss review";

/// What the pointer on a dismiss mark is told: that the review goes, and which file with it.
fn dismiss_hover_of(file: &ReviewFileView) -> String {
    format!("Dismiss this review - deletes {}", file.repo_path_of_file)
}

fn draw_card_button(ui: &mut Ui, text: &str) -> egui::Response {
    widgets::clickable(ui.add(egui::Button::new(RichText::new(text).size(SMALL_SIZE)).frame(false)))
}

/// What the pointer on a card is told: the commit it asks for, and where its file is.
fn hover_of_review(request: &ReviewRequestView, file: &ReviewFileView) -> String {
    // The subject is on the card already: only the paragraph under it, when there is one.
    let mut hover = String::new();
    if let Some(suggestion) = request
        .suggestion
        .as_ref()
        .filter(|suggestion| !suggestion.paragraph.is_empty())
    {
        hover.push_str(&suggestion.paragraph);
        hover.push_str("\n\n");
    }
    if let Some(branch) = &request.branch {
        hover.push_str(&format!("#{branch}\n"));
    }
    hover.push_str(&format!(
        "{}\ndrag to another status",
        file.repo_path_of_file
    ));
    hover
}

/// The card being carried, under the pointer, until it is let go of.
fn draw_carried_review(ui: &Ui) {
    let Some(carried) = egui::DragAndDrop::payload::<DraggedReview>(ui.ctx()) else {
        return;
    };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    let Some(pointer) = ui.ctx().pointer_interact_pos() else {
        return;
    };
    egui::Area::new(egui::Id::new("moontask-carried-review"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + vec2(12.0, 8.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(&carried.label).size(SMALL_SIZE + 1.0));
            });
        });
}
