//! This module is the TUI's command palette: everything the board and the frames do from
//! the keyboard, named the way a person would search for it, run by typing a few words. It
//! opens on `ctrl-p` or `:` on the board, and on `alt-x` or `ctrl-a :` from anywhere.
//!
//! A command replays the key that does the same thing, so the palette runs the code the keys
//! run and cannot drift from them.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    app::{App, Overlay, StartItem},
    board_actions, keys,
    line_input::{LineEdit, LineInput},
};

/// Says what running a command does.
#[derive(Clone)]
pub enum PaletteAction {
    /// Presses a key on the board, on the picked task and column.
    BoardKey(KeyEvent),
    /// Presses a key after `ctrl-a`: the frames and their tabs.
    PrefixKey(char),
    /// Starts an agent or a shell on the picked task.
    Start(StartItem),
    /// Moves the picked task (or the marked ones) to the top of a column, by its place.
    MoveToColumn(usize),
    /// Ends an agent or a shell, keeping it on its task to resume.
    StopRun { task_id: String, run_id: String },
    /// Starts a stopped agent or shell again, where it left off.
    ResumeRun { task_id: String, run_id: String },
    /// Ends an agent or a shell and takes it off its task.
    DeleteRun { task_id: String, run_id: String },
}

impl PaletteAction {
    /// Says which keys do the same as the command, read off the key table - nothing for a
    /// command no key does.
    fn keys(&self) -> String {
        let key = |letter: char| {
            keys::name_of_the_key(&KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE))
        };
        match self {
            Self::BoardKey(pressed) => keys::name_of_the_key(pressed),
            Self::PrefixKey(letter) => keys::name_of_the_prefixed_key(*letter),
            Self::Start(_) => key(keys::START),
            Self::MoveToColumn(_) => String::new(),
            Self::StopRun { .. } => key(keys::STOP),
            Self::ResumeRun { .. } => key(keys::RESUME),
            Self::DeleteRun { .. } => key(keys::DELETE),
        }
    }
}

pub struct PaletteCommand {
    pub title: String,
    pub description: String,
    /// Holds the keys that do the same, shown beside the title.
    pub keys: String,
    pub action: PaletteAction,
}

fn command(title: &str, description: &str, action: PaletteAction) -> PaletteCommand {
    PaletteCommand {
        title: title.to_string(),
        description: description.to_string(),
        keys: action.keys(),
        action,
    }
}

fn board_key(letter: char) -> PaletteAction {
    PaletteAction::BoardKey(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE))
}

/// Lists the commands on the agent or shell the keyboard is on: in the tab in front, or picked
/// on its task.
fn list_the_commands_on_the_run_in_focus(app: &App) -> Vec<PaletteCommand> {
    let Some((task_id, run)) = board_actions::find_the_run_in_focus(app) else {
        return Vec::new();
    };
    let task_title = app
        .board
        .task(&task_id)
        .map(|task| task.title.clone())
        .unwrap_or_default();
    let name = crate::run_menu::name_of_the_run(&run, &task_title);
    let what = crate::run_menu::kind_of_the_run(&run);
    let ids = || (task_id.clone(), run.id.clone());
    let mut commands = Vec::new();
    // A shell stopped is gone - nothing resumes it - so a shell is only deleted.
    let an_agent = what == "agent";
    if an_agent && crate::run_menu::is_going(&run) {
        let (task_id, run_id) = ids();
        commands.push(command(
            &format!("Stop this {what}"),
            &format!("end {name}; it stays on its task, to resume"),
            PaletteAction::StopRun { task_id, run_id },
        ));
    } else if an_agent && run.resumable {
        let (task_id, run_id) = ids();
        commands.push(command(
            &format!("Resume this {what}"),
            &format!("start {name} again where it left off"),
            PaletteAction::ResumeRun { task_id, run_id },
        ));
    }
    let (task_id, run_id) = ids();
    commands.push(command(
        &format!("Delete this {what}"),
        &format!("end {name} and take it off its task"),
        PaletteAction::DeleteRun { task_id, run_id },
    ));
    commands
}

/// Lists every command the board offers from where it is: the agent or shell in focus first,
/// then the picked task's, its column's, the board's and the frames'.
pub fn list_the_commands(app: &App) -> Vec<PaletteCommand> {
    let mut commands = list_the_commands_on_the_run_in_focus(app);
    if let Some(task) = app.picked_card() {
        for item in board_actions::list_what_can_start(app) {
            let title = match item {
                StartItem::Agent(agent) => format!("Start {}", agent.label()),
                StartItem::Shell(moon_core::moontasks::StartFolder::Repo) => {
                    "Start a shell".to_string()
                }
                StartItem::Shell(_) => "Start a shell in the task's folder".to_string(),
            };
            commands.push(command(
                &title,
                &format!("on {}", task.title),
                PaletteAction::Start(item),
            ));
        }
        commands.extend([
            command(
                "Rename task",
                "change the picked task's title",
                board_key(keys::RENAME),
            ),
            command("Edit tags", "tag the picked task", board_key(keys::TAGS)),
            command(
                "Edit notes",
                "open the task's notes.md in $EDITOR",
                board_key(keys::NOTES),
            ),
            command(
                "Show task details",
                "notes, agents, shells, files and review beside the board",
                PaletteAction::BoardKey(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            ),
            command(
                "Mark task",
                "mark or unmark it, so several move together",
                board_key(keys::MARK),
            ),
        ]);
        for (place, column) in app.board.columns.iter().enumerate() {
            if place != app.column {
                commands.push(command(
                    &format!("Move task to {}", column.label),
                    "to the top of the column - the marked tasks too",
                    PaletteAction::MoveToColumn(place),
                ));
            }
        }
    }
    commands.extend([
        command(
            "New task",
            "at the top of the picked column",
            board_key(keys::NEW_TASK),
        ),
        command(
            "New task at the bottom",
            "at the bottom of the picked column",
            board_key(keys::NEW_TASK_AT_THE_BOTTOM),
        ),
        command(
            "Fold column",
            "fold the picked column to a narrow strip, or unfold it",
            board_key(keys::FOLD),
        ),
        command("Move column left", "", board_key(keys::COLUMN_LEFT)),
        command("Move column right", "", board_key(keys::COLUMN_RIGHT)),
        command(
            "Filter tasks",
            "show the tasks whose title, tags or notes hold a word",
            board_key(keys::FILTER),
        ),
    ]);
    if !app.filter.is_empty() {
        commands.push(command(
            "Clear filter",
            "show every task again",
            PaletteAction::BoardKey(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        ));
    }
    commands.extend([
        command(
            "Show board",
            "bring the board's tab to the front",
            PaletteAction::PrefixKey(keys::SHOW_BOARD),
        ),
        command(
            "Next frame",
            "move the keyboard to the next frame",
            PaletteAction::PrefixKey(keys::NEXT_FRAME),
        ),
        command(
            "Next tab",
            "show the next tab of the frame",
            PaletteAction::PrefixKey(keys::NEXT_TAB),
        ),
        command(
            "Previous tab",
            "show the previous tab of the frame",
            PaletteAction::PrefixKey(keys::PREVIOUS_TAB),
        ),
        command(
            "Close tab",
            "a shell ends with it; an agent keeps going on its task",
            PaletteAction::PrefixKey(keys::CLOSE_TAB),
        ),
        command(
            "Send a line to the agent",
            "type a line into the agent of the tab in front, as if typed there",
            PaletteAction::PrefixKey(keys::TELL_THE_TAB),
        ),
        command(
            "Show keys",
            "the few keys worth knowing",
            board_key(keys::HELP),
        ),
        command("Quit", "ends what this moon started", board_key(keys::QUIT)),
    ]);
    commands
}

/// Returns the commands whose title or description holds every typed word, the ones whose
/// title holds them all first - the GUI palette's order.
pub fn rank_the_commands(commands: Vec<PaletteCommand>, query: &str) -> Vec<PaletteCommand> {
    let terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let mut matching: Vec<PaletteCommand> = commands
        .into_iter()
        .filter(|command| {
            let searched = format!("{} {}", command.title, command.description).to_lowercase();
            terms.iter().all(|term| searched.contains(term))
        })
        .collect();
    // A stable sort keeps the list's own order within each rank.
    matching.sort_by_cached_key(|command| {
        let title = command.title.to_lowercase();
        if title
            .split_whitespace()
            .eq(terms.iter().map(String::as_str))
        {
            0
        } else if terms.iter().all(|term| title.contains(term.as_str())) {
            1
        } else {
            2
        }
    });
    matching
}

/// Returns the commands the palette shows for what is typed in it.
pub fn list_the_matching_commands(app: &App, query: &str) -> Vec<PaletteCommand> {
    rank_the_commands(list_the_commands(app), query)
}

pub fn open_the_palette(app: &mut App) {
    app.overlay = Some(Overlay::Palette {
        line: LineInput::default(),
        index: 0,
    });
}

/// Takes a key while the palette is open: up and down pick, enter runs, escape closes.
pub fn press_on_the_palette(app: &mut App, mut line: LineInput, index: usize, key: KeyEvent) {
    let count = list_the_matching_commands(app, &line.text).len();
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let index = match key.code {
        KeyCode::Down => (index + 1) % count.max(1),
        KeyCode::Char('n') if ctrl => (index + 1) % count.max(1),
        KeyCode::Up => (index + count.max(1) - 1) % count.max(1),
        KeyCode::Char('p') if ctrl => (index + count.max(1) - 1) % count.max(1),
        _ => match line.press(&key) {
            LineEdit::Cancelled => return,
            LineEdit::Submitted => {
                let picked = list_the_matching_commands(app, &line.text)
                    .into_iter()
                    .nth(index);
                if let Some(picked) = picked {
                    run_the_command(app, picked.action);
                }
                return;
            }
            // What is typed changed, so the list did: its first row is picked again.
            LineEdit::Editing => 0,
        },
    };
    app.overlay = Some(Overlay::Palette { line, index });
}

pub fn run_the_command(app: &mut App, action: PaletteAction) {
    match action {
        PaletteAction::BoardKey(key) => crate::board_keys::press_on_the_board(app, key),
        PaletteAction::PrefixKey(key) => {
            app.press_after_the_prefix(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
        }
        PaletteAction::Start(item) => {
            if let Some(task_id) = app.picked_task_id() {
                board_actions::start_on_the_task(app, &task_id, item);
            }
        }
        PaletteAction::MoveToColumn(column) => {
            let cards = app.cards_to_move();
            board_actions::drop_cards_at(app, cards, column, 0);
        }
        PaletteAction::StopRun { task_id, run_id } => {
            board_actions::stop_the_run(app, &task_id, &run_id)
        }
        PaletteAction::ResumeRun { task_id, run_id } => {
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
        PaletteAction::DeleteRun { task_id, run_id } => {
            board_actions::delete_the_run(app, &task_id, &run_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(titles: &[&str]) -> Vec<PaletteCommand> {
        titles
            .iter()
            .map(|title| command(title, "", PaletteAction::PrefixKey('x')))
            .collect()
    }

    fn titles_of(commands: &[PaletteCommand]) -> Vec<&str> {
        commands
            .iter()
            .map(|command| command.title.as_str())
            .collect()
    }

    #[test]
    fn every_typed_word_has_to_match_somewhere() {
        let ranked = rank_the_commands(named(&["Rename task", "Edit notes", "New task"]), "ta re");
        assert_eq!(titles_of(&ranked), ["Rename task"]);
    }

    #[test]
    fn a_title_holding_every_word_comes_before_a_description_that_does() {
        let mut commands = named(&["Edit tags", "Edit notes"]);
        commands[0].description = "notes".to_string();
        let ranked = rank_the_commands(commands, "notes");
        assert_eq!(titles_of(&ranked), ["Edit notes", "Edit tags"]);
    }
}
