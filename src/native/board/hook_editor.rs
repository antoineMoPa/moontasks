//! The editor a hook's pill opens, under the pill: what it starts, its prompt, and for a
//! schedule its name and time. Saving writes the hook's file - see
//! [`crate::moontasks::hooks`] - so what is typed here and what is in the file are one thing.
//!
//! It stands over the board until it is saved, removed, or let go of with Escape or a click
//! anywhere else.

use egui::{Align2, Context, Id, Rect, RichText, TextEdit, Ui, vec2};

use crate::{
    api::AgentKind,
    moontasks::{
        ColumnId,
        hooks::{
            ColumnHook, RemoveHookRequest, SaveHookRequest, Schedule, ScheduleEvery, TimeAsked,
            TimeOfDay, start_word,
        },
    },
    native::{
        app::App,
        board::{BoardAction, available_agents},
        theme::{Palette, SMALL_SIZE},
        widgets,
    },
};

const WIDTH: f32 = 340.0;
/// How far under its pill the editor stands.
const GAP: f32 = 6.0;

/// Which hook an editor is of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HookEditorTarget {
    Column(ColumnId),
    /// A schedule, by the name it was saved under - `None` for one not saved yet.
    Schedule(Option<String>),
}

/// The hook being edited, as it is being typed.
pub(crate) struct HookEditor {
    pub(crate) target: HookEditorTarget,
    name: String,
    every: ScheduleEvery,
    at: String,
    start: AgentKind,
    prompt: String,
    /// Whether the hook is saved already, which is what has a file to remove.
    saved_before: bool,
    /// The pill it was opened from, which it stands under.
    anchor: Rect,
    /// What the last save or removal was refused with.
    pub(crate) error: Option<String>,
    /// The frame it opened on, whose click - the one on the pill - is not a click elsewhere.
    opened_on_frame: u64,
    /// Whether the prompt is to take the keyboard on the next frame.
    focus_prompt: bool,
    /// The same for a new schedule's title, which is the first thing it needs.
    focus_title: bool,
}

/// Open the editor of a hook under the pill it was pressed on - or put it away, if that hook's
/// editor is the one open.
pub(crate) fn open_hook_editor(app: &mut App, target: HookEditorTarget, anchor: Rect) {
    if app
        .model
        .board
        .hook_editor
        .as_ref()
        .is_some_and(|open| open.target == target)
    {
        app.model.board.hook_editor = None;
        return;
    }
    let hooks = &app.model.board.hooks;
    let default_start = available_agents(app)
        .into_iter()
        .find(|agent| *agent != AgentKind::None)
        .unwrap_or(AgentKind::Claude);
    let mut editor = HookEditor {
        target: target.clone(),
        name: String::new(),
        every: ScheduleEvery::Day,
        at: "09:00".to_string(),
        start: default_start,
        prompt: String::new(),
        saved_before: false,
        anchor,
        error: None,
        opened_on_frame: 0,
        focus_prompt: true,
        focus_title: false,
    };
    match &target {
        HookEditorTarget::Column(column) => {
            if let Some(hook) = hooks.of_column(column) {
                editor.saved_before = true;
                editor.start = hook.start;
                editor.prompt = hook.prompt.clone();
            }
        }
        HookEditorTarget::Schedule(Some(name)) => {
            if let Some(schedule) = hooks.schedules.iter().find(|s| s.name == *name) {
                editor.saved_before = true;
                editor.name = schedule.name.clone();
                editor.every = schedule.every;
                editor.at = schedule.at.written_for(schedule.every);
                editor.start = schedule.start;
                editor.prompt = schedule.prompt.clone();
            }
        }
        HookEditorTarget::Schedule(None) => {
            editor.focus_prompt = false;
            editor.focus_title = true;
        }
    }
    app.model.board.hook_editor = Some(editor);
}

/// The editor, if one is open, over the board.
pub(super) fn draw(
    app: &mut App,
    ctx: &Context,
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) {
    let mut agents = offered_starts(app);
    // What the hook starts is offered whether or not this machine has it: it is what the file
    // says, and a hook written elsewhere may name an agent this one lacks.
    if let Some(start) = app
        .model
        .board
        .hook_editor
        .as_ref()
        .map(|editor| editor.start)
        && !agents.contains(&start)
    {
        agents.insert(0, start);
    }
    let column_label = |column: &ColumnId| {
        app.model
            .board
            .columns
            .iter()
            .find(|known| known.id == *column)
            .map_or_else(|| column.to_string(), |known| known.label.clone())
    };
    let title = match app
        .model
        .board
        .hook_editor
        .as_ref()
        .map(|e| e.target.clone())
    {
        Some(HookEditorTarget::Column(column)) => {
            format!("When a card arrives in {}", column_label(&column))
        }
        Some(HookEditorTarget::Schedule(_)) => String::new(),
        None => return,
    };
    let frame_nr = ctx.cumulative_frame_nr();
    let Some(editor) = app.model.board.hook_editor.as_mut() else {
        return;
    };
    if editor.opened_on_frame == 0 {
        editor.opened_on_frame = frame_nr;
    }

    let (pivot, at) = match editor.target {
        HookEditorTarget::Column(_) => (
            Align2::RIGHT_TOP,
            editor.anchor.right_bottom() + vec2(0.0, GAP),
        ),
        HookEditorTarget::Schedule(_) => (
            Align2::LEFT_TOP,
            editor.anchor.left_bottom() + vec2(0.0, GAP),
        ),
    };
    let mut close = false;
    let area = egui::Area::new(Id::new("moontask-hook-editor"))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(palette.panel)
                .stroke(egui::Stroke::new(1.0, palette.accent))
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    close = draw_contents(ui, editor, &title, &agents, palette, actions);
                });
        });

    let clicked_elsewhere = ctx.input(|input| {
        input.pointer.any_click()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| !area.response.rect.contains(pos))
    });
    let escaped = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    if close || escaped || (clicked_elsewhere && frame_nr != editor.opened_on_frame) {
        app.model.board.hook_editor = None;
    }
}

/// What a hook can start here: the agents installed, then a shell.
fn offered_starts(app: &App) -> Vec<AgentKind> {
    let mut agents: Vec<AgentKind> = available_agents(app)
        .into_iter()
        .filter(|agent| *agent != AgentKind::None)
        .collect();
    agents.push(AgentKind::None);
    agents
}

/// Answers whether the editor is to be put away.
fn draw_contents(
    ui: &mut Ui,
    editor: &mut HookEditor,
    title: &str,
    agents: &[AgentKind],
    palette: &Palette,
    actions: &mut Vec<BoardAction>,
) -> bool {
    let is_schedule = matches!(editor.target, HookEditorTarget::Schedule(_));
    ui.spacing_mut().item_spacing.y = 8.0;

    if is_schedule {
        let title_box = ui.add(
            TextEdit::singleline(&mut editor.name)
                .hint_text("Title")
                .frame(egui::Frame::NONE)
                .font(egui::FontId::proportional(SMALL_SIZE + 1.0))
                .desired_width(f32::INFINITY),
        );
        if editor.focus_title {
            title_box.request_focus();
            editor.focus_title = false;
        }
        ui.horizontal(|ui| {
            muted(ui, "every", palette);
            for every in ScheduleEvery::ALL {
                if widgets::clickable(ui.selectable_label(editor.every == every, every.word()))
                    .clicked()
                    && editor.every != every
                {
                    editor.at = time_carried_over(&editor.at, editor.every, every);
                    editor.every = every;
                }
            }
            match editor.every.asks_for() {
                TimeAsked::Nothing => {}
                TimeAsked::MinutePast => {
                    muted(ui, "at", palette);
                    ui.add(
                        TextEdit::singleline(&mut editor.at)
                            .hint_text(":00")
                            .desired_width(30.0),
                    );
                }
                TimeAsked::TimeOfDay => {
                    muted(ui, "at", palette);
                    ui.add(
                        TextEdit::singleline(&mut editor.at)
                            .hint_text("08:30")
                            .desired_width(44.0),
                    );
                }
            }
        });
    } else {
        ui.label(
            RichText::new(title)
                .size(SMALL_SIZE + 0.5)
                .color(palette.ink),
        );
    }

    ui.horizontal_wrapped(|ui| {
        muted(ui, "start", palette);
        for agent in agents {
            if widgets::clickable(ui.selectable_label(editor.start == *agent, start_word(*agent)))
                .clicked()
            {
                editor.start = *agent;
            }
        }
    });

    let hint = match editor.start {
        AgentKind::None => "Commands",
        _ => "Prompt",
    };
    let placeholders = if is_schedule {
        "{date} is filled in with today's date"
    } else {
        "{title} {notes} {from} {folder} {id} are filled in from the card"
    };
    let prompt = ui
        .add(
            TextEdit::multiline(&mut editor.prompt)
                .hint_text(hint)
                .desired_rows(5)
                .desired_width(f32::INFINITY),
        )
        .on_hover_text(placeholders);
    if editor.focus_prompt {
        prompt.request_focus();
        editor.focus_prompt = false;
    }

    if let Some(error) = &editor.error {
        ui.label(
            RichText::new(error)
                .size(SMALL_SIZE - 1.0)
                .color(palette.warn),
        );
    }

    let mut close = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::quiet_button_colored(ui, "[save]", palette.accent).clicked() {
                match request_to_save(editor) {
                    Ok(request) => {
                        editor.error = None;
                        actions.push(BoardAction::SaveHook(request));
                    }
                    Err(error) => editor.error = Some(error),
                }
            }
            if let Some(removal) = request_to_remove(editor)
                && widgets::quiet_button_colored(ui, "[remove]", palette.muted)
                    .on_hover_text("Take this hook off the board, and its file with it")
                    .clicked()
            {
                actions.push(BoardAction::RemoveHook(removal));
                close = true;
            }
        });
    });
    close
}

fn muted(ui: &mut Ui, text: &str, palette: &Palette) {
    ui.label(
        RichText::new(text)
            .size(SMALL_SIZE - 1.0)
            .color(palette.muted),
    );
}

/// The time as typed for one kind of schedule, kept as near as it goes when the schedule is
/// switched to another: `08:30` becomes `:30` for every hour, and `:30` becomes `09:30`.
fn time_carried_over(typed: &str, from: ScheduleEvery, to: ScheduleEvery) -> String {
    let at = TimeOfDay::parse_for(from, typed).unwrap_or(TimeOfDay { hour: 9, minute: 0 });
    let at = match from.asks_for() {
        TimeAsked::TimeOfDay => at,
        _ => TimeOfDay { hour: 9, ..at },
    };
    at.written_for(to)
}

/// The save the editor's fields make, or what is wrong with them.
fn request_to_save(editor: &HookEditor) -> Result<SaveHookRequest, String> {
    match &editor.target {
        HookEditorTarget::Column(column) => Ok(SaveHookRequest::Column {
            hook: ColumnHook {
                column: column.clone(),
                start: editor.start,
                prompt: editor.prompt.clone(),
            },
        }),
        HookEditorTarget::Schedule(renamed_from) => {
            if editor.name.trim().is_empty() {
                return Err("A schedule needs a title".to_string());
            }
            let at = TimeOfDay::parse_for(editor.every, &editor.at).ok_or_else(|| match editor
                .every
                .asks_for()
            {
                TimeAsked::MinutePast => {
                    format!("{:?} is not a minute past the hour, such as :30", editor.at)
                }
                _ => format!("{:?} is not a time such as 08:30", editor.at),
            })?;
            Ok(SaveHookRequest::Schedule {
                schedule: Schedule {
                    name: editor.name.trim().to_string(),
                    every: editor.every,
                    at,
                    start: editor.start,
                    prompt: editor.prompt.clone(),
                },
                renamed_from: renamed_from.clone(),
            })
        }
    }
}

/// The removal of the hook being edited, if it was saved before - a new one has nothing to
/// take away.
fn request_to_remove(editor: &HookEditor) -> Option<RemoveHookRequest> {
    if !editor.saved_before {
        return None;
    }
    match &editor.target {
        HookEditorTarget::Column(column) => Some(RemoveHookRequest::Column {
            column: column.clone(),
        }),
        HookEditorTarget::Schedule(Some(name)) => {
            Some(RemoveHookRequest::Schedule { name: name.clone() })
        }
        HookEditorTarget::Schedule(None) => None,
    }
}
