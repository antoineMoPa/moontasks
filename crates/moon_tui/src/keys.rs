//! This module names every key the TUI answers, once: the key handlers match on these, and the
//! palette and the `?` page show them - so neither can say a key the handlers do not.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// On the board.
pub const QUIT: char = 'q';
pub const HELP: char = '?';
pub const PALETTE: char = ':';
pub const FILTER: char = '/';
pub const NEW_TASK: char = 'n';
pub const NEW_TASK_AT_THE_BOTTOM: char = 'N';
pub const RENAME: char = 'r';
pub const TAGS: char = 't';
pub const NOTES: char = 'e';
pub const START: char = 's';
pub const FOLD: char = 'c';
pub const MARK: char = ' ';
pub const COLUMN_LEFT: char = '<';
pub const COLUMN_RIGHT: char = '>';

// On an agent or a shell picked on its task.
pub const STOP: char = 'x';
pub const DELETE: char = 'D';
pub const RESUME: char = 'R';
pub const TELL: char = 'T';

// After `ctrl-a`, from anywhere.
pub const NEXT_FRAME: char = 'o';
pub const NEXT_TAB: char = 'n';
pub const PREVIOUS_TAB: char = 'p';
pub const SHOW_BOARD: char = '0';
pub const CLOSE_TAB: char = 'x';
pub const TELL_THE_TAB: char = 't';

/// Says how a key is written for a person: `space`, `enter`, `ctrl-p`.
pub fn name_of_the_key(key: &KeyEvent) -> String {
    let name = match key.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(letter) => letter.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        other => format!("{other:?}").to_lowercase(),
    };
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        format!("ctrl-{name}")
    } else {
        name
    }
}

/// Says how a key pressed after `ctrl-a` is written: `ctrl-a x`.
pub fn name_of_the_prefixed_key(key: char) -> String {
    format!(
        "ctrl-a {}",
        name_of_the_key(&KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
    )
}

/// Returns what `?` shows: the few keys worth knowing - the palette names the rest.
pub fn write_the_keys_page() -> String {
    let key =
        |letter: char| name_of_the_key(&KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
    [
        format!("ctrl-p  {PALETTE}  alt-x    the command palette: every action, by name"),
        String::new(),
        "arrows              pick a task, or step through its agents and shells".to_string(),
        "shift+arrows        move the task".to_string(),
        format!(
            "enter  {}  {}         details · start · filter",
            key(START),
            key(FILTER)
        ),
        format!(
            "{}  {}  {}  {}          new task · rename · tags · notes",
            key(NEW_TASK),
            key(RENAME),
            key(TAGS),
            key(NOTES)
        ),
        String::new(),
        format!(
            "on an agent/shell   enter open · {} stop · {} delete · {} resume",
            key(STOP),
            key(DELETE),
            key(RESUME)
        ),
        format!(
            "ctrl-a then         {} board · {} next frame · {} {} tabs · {} close tab",
            key(SHOW_BOARD),
            key(NEXT_FRAME),
            key(NEXT_TAB),
            key(PREVIOUS_TAB),
            key(CLOSE_TAB)
        ),
        String::new(),
        "the mouse drags tasks, columns and tabs, and clicks every button".to_string(),
    ]
    .join("\n")
}
