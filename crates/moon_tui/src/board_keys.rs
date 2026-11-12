//! This module handles the keys pressed on the board itself.

use moon_core::moontasks::ColumnEnd;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    app::{App, PromptAsks},
    board_actions,
    keys::{
        COLUMN_LEFT, COLUMN_RIGHT, FILTER, FOLD, HELP, MARK, NEW_TASK, NEW_TASK_AT_THE_BOTTOM,
        NOTES, PALETTE, QUIT, RENAME, START, TAGS,
    },
    run_menu,
};

pub fn press_on_the_board(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(QUIT) => return app.ask_to_quit(),
        KeyCode::Char('c') if ctrl => return app.ask_to_quit(),
        KeyCode::Char(HELP) => return app.show_the_keys(),
        KeyCode::Char('p') if ctrl => return crate::palette::open_the_palette(app),
        KeyCode::Char(PALETTE) => return crate::palette::open_the_palette(app),
        KeyCode::Tab | KeyCode::BackTab => {
            app.layout.focus_next_frame();
            return;
        }
        KeyCode::Char(digit @ '1'..='9') => {
            return app.focus_the_nth_shell(digit as usize - '1' as usize);
        }
        KeyCode::Char(FILTER) => {
            let filter = app.filter.clone();
            return app.ask_for_a_line(PromptAsks::Filter, &filter);
        }
        _ => {}
    }
    press_on_the_columns(app, key);
}

fn press_on_the_columns(app: &mut App, key: KeyEvent) {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let columns = app.board.columns.len();
    if columns == 0 {
        return;
    }
    let shown = app.cards_shown_in(app.column).len();
    let index = app.card_in_column[app.column];
    if run_menu::press_on_the_picked_run(app, &key) {
        return;
    }
    // Every key but the ones that step through a card's runs puts the card itself back.
    if !matches!(key.code, KeyCode::Down | KeyCode::Up) {
        app.picked_run = None;
    }
    match key.code {
        // Shift and an arrow carries the card (or the marked ones) that way.
        KeyCode::Left if shift => board_actions::move_cards_to_the_column_beside(app, -1),
        KeyCode::Right if shift => board_actions::move_cards_to_the_column_beside(app, 1),
        KeyCode::Down if shift => board_actions::move_card_along_its_column(app, 1),
        KeyCode::Up if shift => board_actions::move_card_along_its_column(app, -1),
        KeyCode::Left => app.column = app.column.saturating_sub(1),
        KeyCode::Right => app.column = (app.column + 1).min(columns - 1),
        KeyCode::Down => step_down(app, index, shown),
        KeyCode::Up => step_up(app, index),
        KeyCode::Home => app.card_in_column[app.column] = 0,
        KeyCode::End => {
            app.card_in_column[app.column] = shown.saturating_sub(1);
        }
        KeyCode::Char(COLUMN_LEFT) => board_actions::move_the_column(app, -1),
        KeyCode::Char(COLUMN_RIGHT) => board_actions::move_the_column(app, 1),
        KeyCode::Char(MARK) => {
            if let Some(task_id) = app.picked_task_id()
                && !app.marked.remove(&task_id)
            {
                app.marked.insert(task_id);
            }
        }
        KeyCode::Esc => {
            if !app.marked.is_empty() {
                app.marked.clear();
            } else if app.task_pane_open {
                app.task_pane_open = false;
            } else {
                app.filter.clear();
            }
        }
        KeyCode::Enter => app.task_pane_open = !app.task_pane_open,
        KeyCode::Char(NEW_TASK) => ask_for_a_new_card(app, ColumnEnd::Top),
        KeyCode::Char(NEW_TASK_AT_THE_BOTTOM) => ask_for_a_new_card(app, ColumnEnd::Bottom),
        KeyCode::Char(RENAME) => {
            if let Some(task) = app.picked_card() {
                let (task_id, title) = (task.id.clone(), task.title.clone());
                app.ask_for_a_line(PromptAsks::Rename { task_id }, &title);
            }
        }
        KeyCode::Char(TAGS) => {
            if let Some(task) = app.picked_card() {
                let (task_id, tags) = (task.id.clone(), task.tags.join(", "));
                app.ask_for_a_line(PromptAsks::Tags { task_id }, &tags);
            }
        }
        KeyCode::Char(NOTES) => {
            if let Some(task_id) = app.picked_task_id() {
                let path = app.session.notes_file_of(&task_id);
                app.edit_notes_at = app.say_if_it_failed(path);
            }
        }
        KeyCode::Char(START) => board_actions::offer_what_to_start(app),
        KeyCode::Char(FOLD) => app.fold_or_unfold_the_column(app.column),
        _ => {}
    }
}

/// Counts the runs the picked card lists, which the up and down arrows step through.
fn runs_on_the_picked_card(app: &App) -> usize {
    app.picked_card()
        .map_or(0, |task| run_menu::runs_pickable_on(task).len())
}

/// Steps down on the down arrow: from a card into its runs, one by one, and on to the next card.
fn step_down(app: &mut App, index: usize, shown: usize) {
    let runs = runs_on_the_picked_card(app);
    app.picked_run = match app.picked_run {
        None if runs > 0 => Some(0),
        Some(run) if run + 1 < runs => Some(run + 1),
        _ if index + 1 < shown => {
            app.card_in_column[app.column] = index + 1;
            None
        }
        kept => kept,
    };
}

/// Steps up on the up arrow: back through the runs to the card, and on to the last run of the card
/// above.
fn step_up(app: &mut App, index: usize) {
    app.picked_run = match app.picked_run {
        Some(0) => None,
        Some(run) => Some(run - 1),
        None if index > 0 => {
            app.card_in_column[app.column] = index - 1;
            runs_on_the_picked_card(app).checked_sub(1)
        }
        None => None,
    };
}

/// Asks the name of a new card for the picked column.
pub fn ask_for_a_new_card(app: &mut App, joins: ColumnEnd) {
    if let Some(column) = app.board.columns.get(app.column) {
        let column = column.id.clone();
        app.ask_for_a_line(PromptAsks::NewCard { column, joins }, "");
    }
}
