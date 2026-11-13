//! The board's hooks, as the board shows them: a pill at the right of a column's heading for
//! the agent its hook starts, a pill beside `board` for each of the board's schedules, a mark
//! on every run a hook started, and a line on a card the hooks have stopped on.
//!
//! A pill is pressed to open its hook's editor - see [`super::hook_editor`]. A column without a
//! hook shows a faint `+` pill while the pointer is over its heading, which is how one is made.
//! The icons are Lucide's `fishing-hook` and `clock`, drawn from their paths.

use egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, vec2,
};

use crate::{
    moontasks::{
        BoardColumn, TaskResourceView, TaskView,
        hooks::{HOOK_RUNS_IN_A_ROW_CAP, Schedule, start_word},
    },
    native::{
        app::App,
        board::hook_editor::{HookEditorTarget, open_hook_editor},
        theme::{Palette, SMALL_SIZE, ThemeMode},
    },
};

/// What a pill is for, which is its color and its icon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PillKind {
    /// A column's hook.
    Column,
    /// One of the board's schedules.
    Schedule,
    /// A column with no hook yet: the pill that makes one.
    Ghost,
}

#[derive(Clone, Copy)]
enum Icon {
    FishingHook,
    Clock,
}

const PILL_PADDING: egui::Vec2 = egui::vec2(7.0, 2.0);
const ICON_GAP: f32 = 4.0;

/// The color schedules are drawn in: amber, apart from the teal of a column's hook.
fn schedule_ink(palette: &Palette) -> Color32 {
    match palette.mode {
        ThemeMode::Dark => Color32::from_rgb(0xe3, 0xc2, 0x7a),
        ThemeMode::Light => Color32::from_rgb(0x8a, 0x6a, 0x1f),
    }
}

fn ink_of(kind: PillKind, palette: &Palette) -> Color32 {
    match kind {
        PillKind::Column => palette.accent_2,
        PillKind::Schedule => schedule_ink(palette),
        PillKind::Ghost => palette.muted,
    }
}

/// A pill: an icon and a word on a rounded ground of the pill's own color, ringed in the
/// accent while its editor is open. Answers the press.
pub(super) fn draw_hook_pill(
    ui: &mut Ui,
    id: egui::Id,
    kind: PillKind,
    text: &str,
    editing: bool,
    palette: &Palette,
) -> Response {
    let ink = ink_of(kind, palette);
    let font = egui::FontId::proportional(SMALL_SIZE - 1.0);
    let word = ui.painter().layout_no_wrap(text.to_string(), font, ink);
    let icon_size = word.size().y.max(10.0);
    let size = vec2(icon_size + ICON_GAP + word.size().x, icon_size) + PILL_PADDING * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let lit = response.hovered() || editing;
    let painter = ui.painter();
    let radius = CornerRadius::same((rect.height() / 2.0) as u8);
    let (fill, ring) = match kind {
        PillKind::Ghost => (
            Color32::TRANSPARENT,
            palette.line.gamma_multiply(if lit { 1.0 } else { 0.7 }),
        ),
        _ => (
            ink.gamma_multiply(if lit { 0.22 } else { 0.13 }),
            ink.gamma_multiply(0.45),
        ),
    };
    let ring = if editing { palette.accent } else { ring };
    painter.rect(
        rect,
        radius,
        fill,
        Stroke::new(1.0, ring),
        egui::StrokeKind::Inside,
    );
    let icon_rect = Rect::from_min_size(rect.min + PILL_PADDING, vec2(icon_size, icon_size));
    let icon = match kind {
        PillKind::Schedule => Icon::Clock,
        PillKind::Column | PillKind::Ghost => Icon::FishingHook,
    };
    paint_icon(painter, icon, icon_rect, ink);
    painter.galley(
        egui::pos2(icon_rect.right() + ICON_GAP, rect.top() + PILL_PADDING.y),
        word,
        ink,
    );
    response
}

/// What a column's pill answers to - which is how a test finds it.
pub(crate) fn column_pill_id(column: &crate::moontasks::ColumnId) -> egui::Id {
    egui::Id::new(("moontask-hook-pill", column.as_str()))
}

pub(crate) fn schedule_pill_id(name: &str) -> egui::Id {
    egui::Id::new(("moontask-schedule-pill", name))
}

/// The pill at the right of a column's heading, drawn in a right-to-left row: the agent the
/// column's hook starts, or - on a column without one, while the pointer is over the heading -
/// the faint pill that makes one. Space is kept for the faint one either way, so the heading
/// does not shift as the pointer crosses it.
pub(super) fn draw_column_hook_pill(
    app: &mut App,
    ui: &mut Ui,
    column: &BoardColumn,
    heading_hovered: bool,
    palette: &Palette,
) {
    let hook = app.model.board.hooks.of_column(&column.id).cloned();
    let editing = app
        .model
        .board
        .hook_editor
        .as_ref()
        .is_some_and(|editor| editor.target == HookEditorTarget::Column(column.id.clone()));
    let id = column_pill_id(&column.id);
    let pressed = match &hook {
        Some(hook) => draw_hook_pill(
            ui,
            id,
            PillKind::Column,
            start_word(hook.start),
            editing,
            palette,
        )
        .on_hover_text(format!(
            "When a card arrives in {}, start {} on it. Click to change",
            column.label,
            start_word(hook.start)
        )),
        None if heading_hovered || editing => {
            draw_hook_pill(ui, id, PillKind::Ghost, "+", editing, palette).on_hover_text(format!(
                "Start an agent on every card that arrives in {}",
                column.label
            ))
        }
        None => {
            // The faint pill's room, drawn as nothing.
            let room = ui.painter().layout_no_wrap(
                "+".to_string(),
                egui::FontId::proportional(SMALL_SIZE - 1.0),
                palette.muted,
            );
            let height = room.size().y.max(10.0);
            ui.allocate_exact_size(
                vec2(height + ICON_GAP + room.size().x, height) + PILL_PADDING * 2.0,
                Sense::hover(),
            )
            .1
        }
    };
    if pressed.clicked() {
        open_hook_editor(
            app,
            HookEditorTarget::Column(column.id.clone()),
            pressed.rect,
        );
    }
}

/// The board's schedules beside `board`, a pill each, and the `[schedule]` that adds one.
pub(super) fn draw_schedule_pills(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let schedules: Vec<Schedule> = app.model.board.hooks.schedules.clone();
    for schedule in &schedules {
        let target = HookEditorTarget::Schedule(Some(schedule.name.clone()));
        let editing = app
            .model
            .board
            .hook_editor
            .as_ref()
            .is_some_and(|editor| editor.target == target);
        let pill = draw_hook_pill(
            ui,
            schedule_pill_id(&schedule.name),
            PillKind::Schedule,
            &schedule.pill_text(),
            editing,
            palette,
        )
        .on_hover_text(format!(
            "Starts {} on the board {}. Click to change",
            start_word(schedule.start),
            match schedule.every {
                crate::moontasks::hooks::ScheduleEvery::Minute => "every minute",
                crate::moontasks::hooks::ScheduleEvery::Hour => "every hour",
                crate::moontasks::hooks::ScheduleEvery::Day => "every day",
                crate::moontasks::hooks::ScheduleEvery::Weekdays => "every weekday",
            }
        ));
        if pill.clicked() {
            open_hook_editor(app, target, pill.rect);
        }
    }
}

/// On a run's row, when a hook started it: the hook's icon and the column or schedule it was.
pub(super) fn draw_hook_mark_on_run(ui: &mut Ui, resource: &TaskResourceView, palette: &Palette) {
    let Some(label) = resource.started_by_hook.as_deref() else {
        return;
    };
    // A review's run is on that review's card, in that status's column: the mark would say
    // again what both already say.
    if resource.for_review.is_some() {
        return;
    }
    let font = egui::FontId::proportional(SMALL_SIZE - 1.0);
    let word = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, palette.accent_2);
    let icon_size = word.size().y.max(10.0);
    let size = vec2(icon_size + 3.0 + word.size().x, icon_size);
    // Only in the room the run's name has left: drawn anyway, it would lie over the name.
    if ui.available_width() < size.x {
        return;
    }
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.on_hover_text(format!("Started by the {label} hook"));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let icon_rect = Rect::from_min_size(rect.min, vec2(icon_size, icon_size));
    paint_icon(ui.painter(), Icon::FishingHook, icon_rect, palette.accent_2);
    ui.painter().galley(
        egui::pos2(icon_rect.right() + 3.0, rect.top()),
        word,
        palette.accent_2,
    );
}

/// The line a card says when the hooks have stopped on it.
pub(super) fn draw_hooks_stopped_note(ui: &mut Ui, task: &TaskView, palette: &Palette) {
    if !task.hooks_stopped {
        return;
    }
    ui.add(
        egui::Label::new(
            RichText::new(format!(
                "Hooks stopped: {HOOK_RUNS_IN_A_ROW_CAP} hook runs in a row on this card"
            ))
            .size(SMALL_SIZE - 1.0)
            .color(palette.accent),
        )
        .wrap(),
    )
    .on_hover_text("Move the card yourself and its hooks start again");
}

/// Lucide's icon, on its 24-unit grid, scaled into `rect` and stroked in `ink`.
fn paint_icon(painter: &Painter, icon: Icon, rect: Rect, ink: Color32) {
    let at = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x / 24.0 * rect.width(),
            rect.top() + y / 24.0 * rect.height(),
        )
    };
    let stroke = Stroke::new((rect.width() / 12.0).max(1.1), ink);
    let circle = |cx: f32, cy: f32, r: f32| {
        painter.circle_stroke(at(cx, cy), r / 24.0 * rect.width(), stroke);
    };
    match icon {
        Icon::Clock => {
            circle(12.0, 12.0, 10.0);
            painter.add(egui::Shape::line(
                vec![at(12.0, 6.0), at(12.0, 12.0), at(16.0, 14.0)],
                stroke,
            ));
        }
        Icon::FishingHook => {
            // m17.586 11.414 -5.93 5.93, then the bend of the hook: half a circle round
            // (7.656, 13.344) from its lower right to its upper left, the long way down.
            let mut shank = vec![at(17.586, 11.414), at(11.656, 17.344)];
            let (cx, cy, r) = (7.656_f32, 13.344_f32, 5.657_f32);
            for step in 1..=12 {
                let angle = (45.0 + 180.0 * step as f32 / 12.0).to_radians();
                shank.push(at(cx + r * angle.cos(), cy + r * angle.sin()));
            }
            // l3.137-3.137, the barb's turn, and down to its point.
            shank.extend([
                at(6.793, 6.207),
                at(7.55, 6.05),
                at(8.0, 6.707),
                at(8.0, 10.0),
            ]);
            painter.add(egui::Shape::line(shank, stroke));
            painter.line_segment([at(20.414, 8.586), at(22.0, 7.0)], stroke);
            circle(19.0, 10.0, 2.0);
        }
    }
}
