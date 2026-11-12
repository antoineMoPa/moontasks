//! This module handles the mouse: an open menu's first, then the frames' own - tabs, splits,
//! the lines between them - then the board's: a click picks, a drag carries tasks or a column,
//! a button does what it says. It is all measured against where things were drawn last - see
//! [`crate::hits`].

use std::time::{Duration, Instant};

use ratatui::{
    crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    layout::{Position, Rect},
};

use moon_tui_frames::{FramesEvent, PointerOutcome};

use crate::{
    app::{App, Drag, Dragged, Overlay},
    board_actions,
    hits::Hit,
};

/// Sets how soon a second click on a card has to follow the first to be a double click.
const DOUBLE_CLICK_WITHIN: Duration = Duration::from_millis(400);

/// Sets how far, in cells, a press has to travel to be a drag rather than a click.
const DRAG_STARTS_AFTER: u16 = 1;

/// Says where a drag would land cards: a column, and the place among its cards.
#[derive(Debug, PartialEq, Eq)]
pub struct CardLanding {
    pub column: usize,
    /// Holds the place among the column's cards that are not being carried.
    pub position: usize,
    /// Holds the row the line marking it is drawn on.
    pub row: u16,
}

pub fn handle_the_mouse(app: &mut App, mouse: MouseEvent) {
    let at = Position::new(mouse.column, mouse.row);
    app.pointer_at = Some(at);
    if app.overlay.is_some() {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            click_on_the_overlay(app, at);
        }
        return;
    }
    match app.frames.handle_the_mouse(mouse, &mut app.layout) {
        PointerOutcome::Taken(events) => {
            for event in events {
                match event {
                    FramesEvent::PaneCloseRequested(pane) => app.close_the_pane(pane),
                    FramesEvent::NewTabRequested(frame) => {
                        board_actions::start_a_shell_in_the_frame(app, frame)
                    }
                    FramesEvent::PaneFocused(_) | FramesEvent::PaneMoved(_) => {}
                }
            }
        }
        PointerOutcome::InPane { pane, .. } if pane == app.board_pane => {
            handle_the_mouse_on_the_board(app, mouse)
        }
        PointerOutcome::InPane { pane, area } => {
            if let Some(crate::panes::MoonPane::Shell { terminal_id }) = app.layout.pane(pane)
                && let Some(shell) = app.shells.get_mut(terminal_id)
            {
                shell.handle_the_mouse(&mouse, area);
            }
        }
        PointerOutcome::Missed => {}
    }
}

fn handle_the_mouse_on_the_board(app: &mut App, mouse: MouseEvent) {
    let at = Position::new(mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => press(app, at, mouse.modifiers),
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(drag) = &mut app.drag {
                drag.at = at;
                if at.x.abs_diff(drag.from.x) > DRAG_STARTS_AFTER || at.y.abs_diff(drag.from.y) > 0
                {
                    drag.moving = true;
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => release(app, at),
        MouseEventKind::ScrollDown => scroll_column_under(app, at, 1),
        MouseEventKind::ScrollUp => scroll_column_under(app, at, -1),
        _ => {}
    }
}

/// Takes a click while a menu or a dialog is open: one of its rows or buttons, or - anywhere
/// else - closing it.
fn click_on_the_overlay(app: &mut App, at: Position) {
    let hit = app.hits.find_the_spot_at(at).map(|(_, hit)| hit.clone());
    let Some(overlay) = app.overlay.take() else {
        return;
    };
    match (hit, overlay) {
        (Some(Hit::MenuRow(index)), Overlay::Palette { line, .. }) => {
            let picked = crate::palette::list_the_matching_commands(app, &line.text)
                .into_iter()
                .nth(index);
            if let Some(picked) = picked {
                crate::palette::run_the_command(app, picked.action);
            }
        }
        (Some(Hit::MenuRow(index)), Overlay::StartMenu { task_id, items, .. }) => {
            if let Some(item) = items.get(index) {
                board_actions::start_on_the_task(app, &task_id, *item);
            }
        }
        (Some(Hit::Quit), _) => app.quit = true,
        // A click inside a box that is not one of its rows leaves it open.
        (Some(Hit::Inside), overlay) => app.overlay = Some(overlay),
        // Everything else closes it. A filter being typed is kept, as it was shown.
        (_, Overlay::Prompt { .. } | Overlay::Text { .. } | Overlay::ConfirmQuit) => {}
        (_, Overlay::Palette { .. } | Overlay::StartMenu { .. }) => {}
    }
}

fn press(app: &mut App, at: Position, modifiers: KeyModifiers) {
    let Some((_, hit)) = app.hits.find_the_spot_at(at).cloned() else {
        return;
    };
    if !matches!(hit, Hit::DeleteRun { .. }) {
        app.armed_delete = None;
    }
    match hit {
        Hit::DeleteRun { task_id, run_id } => {
            if app.delete_is_armed_for(&run_id) {
                app.armed_delete = None;
                board_actions::delete_the_run(app, &task_id, &run_id);
            } else {
                app.armed_delete = Some((run_id, Instant::now()));
            }
        }
        Hit::StopRun { task_id, run_id } => board_actions::stop_the_run(app, &task_id, &run_id),
        Hit::ResumeRun { task_id, run_id } => {
            let title = app
                .board
                .task(&task_id)
                .and_then(|task| {
                    let run = task.resources.iter().find(|run| run.id == run_id)?;
                    Some(crate::run_menu::name_of_the_run(run, &task.title))
                })
                .unwrap_or_default();
            board_actions::resume_the_run(app, &task_id, &run_id, &title);
        }
        Hit::Run { task_id, run_id } => open_the_run_clicked(app, &task_id, &run_id),
        Hit::Details(task_id) => {
            app.pick_task(&task_id);
            app.task_pane_open = true;
        }
        Hit::StartOn(task_id) => {
            app.pick_task(&task_id);
            board_actions::offer_what_to_start(app);
        }
        Hit::EditNotes(task_id) => {
            let path = app.session.notes_file_of(&task_id);
            app.edit_notes_at = app.say_if_it_failed(path);
        }
        Hit::NewTask { column, joins } => {
            app.column = column;
            crate::board_keys::ask_for_a_new_card(app, joins);
        }
        Hit::Fold(column) => {
            app.column = column;
            app.fold_or_unfold_the_column(column);
        }
        Hit::EditFilter => {
            let filter = app.filter.clone();
            app.ask_for_a_line(crate::app::PromptAsks::Filter, &filter);
        }
        Hit::ClearFilter => app.filter.clear(),
        Hit::Heading(column) => {
            app.column = column;
            app.drag = Some(Drag {
                what: Dragged::Column(column),
                from: at,
                at,
                moving: false,
            });
        }
        Hit::Task { column, task_id } => press_on_a_task(app, at, modifiers, column, task_id),
        Hit::MenuRow(_) | Hit::Quit | Hit::Stay | Hit::Close | Hit::Inside => {}
    }
}

/// Picks a task - marks it too, with a modifier held - opens its details on a double click,
/// and picks it up to be carried.
fn press_on_a_task(
    app: &mut App,
    at: Position,
    modifiers: KeyModifiers,
    column: usize,
    task_id: String,
) {
    if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::ALT)
        && !app.marked.remove(&task_id)
    {
        app.marked.insert(task_id.clone());
    }
    app.column = column;
    app.pick_task(&task_id);
    if clicked_twice(app, &task_id) {
        app.task_pane_open = true;
    }
    // A task picked up that is marked carries every marked task with it; one that is not
    // goes alone.
    let cards = if app.marked.contains(&task_id) {
        app.cards_to_move()
    } else {
        vec![task_id]
    };
    app.drag = Some(Drag {
        what: Dragged::Cards(cards),
        from: at,
        at,
        moving: false,
    });
}

/// Says whether this click on a card follows one on the same card soon enough to be a double
/// click - which opens its details beside the board.
fn clicked_twice(app: &mut App, task_id: &str) -> bool {
    let now = Instant::now();
    let twice = app.last_click.as_ref().is_some_and(|(last, at)| {
        last == task_id && now.duration_since(*at) <= DOUBLE_CLICK_WITHIN
    });
    // A double click is not the first of another.
    app.last_click = (!twice).then(|| (task_id.to_string(), now));
    twice
}

fn open_the_run_clicked(app: &mut App, task_id: &str, run_id: &str) {
    app.pick_task(task_id);
    app.picked_run = app.board.task(task_id).and_then(|task| {
        crate::run_menu::runs_pickable_on(task)
            .iter()
            .position(|run| run.id == run_id)
    });
    let run = app
        .board
        .task(task_id)
        .and_then(|task| task.resources.iter().find(|run| run.id == run_id))
        .cloned();
    if let Some(run) = run {
        board_actions::open_the_run(app, task_id, &run);
    }
}

fn release(app: &mut App, at: Position) {
    let Some(drag) = app.drag.take() else {
        return;
    };
    if !drag.moving {
        return;
    }
    match drag.what {
        Dragged::Cards(cards) => {
            if let Some(landing) = where_cards_would_land(app, &cards, at) {
                board_actions::drop_cards_at(app, cards, landing.column, landing.position);
            }
        }
        Dragged::Column(column) => {
            if let Some(position) = where_a_column_would_land(app, at) {
                board_actions::drop_column_at(app, column, position);
            }
        }
    }
}

/// Finds the column under the pointer and the place among its cards the carried ones would
/// take.
pub fn where_cards_would_land(app: &App, carried: &[String], at: Position) -> Option<CardLanding> {
    let (area, column) = app
        .hits
        .columns
        .iter()
        .find(|(rect, _)| at.x >= rect.x && at.x < rect.right())
        .copied()?;
    // These are the cards drawn in that column, top to bottom, leaving out the carried ones.
    let drawn: Vec<(Rect, usize, &str)> = app
        .hits
        .tasks_drawn()
        .filter(|(_, card_column, task_id)| {
            *card_column == column && !carried.iter().any(|carried| carried == task_id)
        })
        .collect();
    let in_order: Vec<String> = app
        .board
        .columns
        .get(column)
        .map(|column| {
            app.board
                .cards_in(&column.id)
                .iter()
                .map(|task| task.id.clone())
                .filter(|task_id| !carried.contains(task_id))
                .collect()
        })
        .unwrap_or_default();
    // The first card drawn whose middle is below the pointer is the one they go above.
    let above = drawn
        .iter()
        .find(|(rect, _, _)| at.y < rect.y + rect.height / 2);
    Some(match above {
        Some((rect, _, task_id)) => CardLanding {
            column,
            position: in_order.iter().position(|id| id == task_id).unwrap_or(0),
            row: rect.y.saturating_sub(1).max(area.y),
        },
        None => CardLanding {
            column,
            // Below the last card drawn, they go after it, which is where the next one hidden by
            // the screen's end would be.
            position: drawn
                .last()
                .and_then(|(_, _, task_id)| in_order.iter().position(|id| id == task_id))
                .map_or(in_order.len(), |last| last + 1),
            row: drawn
                .last()
                .map_or(area.y + 1, |(rect, _, _)| rect.bottom())
                .min(area.bottom().saturating_sub(2)),
        },
    })
}

/// Finds the place among the columns a carried column would take: the place under the
/// pointer, or the nearest one when the pointer is past the board's ends.
pub fn where_a_column_would_land(app: &App, at: Position) -> Option<usize> {
    let slots = &app.hits.column_slots;
    slots
        .iter()
        .find(|(rect, _)| at.x >= rect.x && at.x < rect.right())
        .or_else(|| {
            if at.x < slots.first()?.0.x {
                slots.first()
            } else {
                slots.last()
            }
        })
        .map(|(_, position)| *position)
}

fn scroll_column_under(app: &mut App, at: Position, step: isize) {
    if let Some((_, column)) = app.hits.columns.iter().find(|(rect, _)| rect.contains(at)) {
        let column = *column;
        let shown = app.cards_shown_in(column).len();
        let index = &mut app.card_in_column[column];
        *index = (*index as isize + step).clamp(0, shown.saturating_sub(1) as isize) as usize;
        app.column = column;
    }
}
