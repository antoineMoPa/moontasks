//! This module does what the board is asked: it moves cards and columns, makes and renames
//! cards, and starts, stops, resumes and tells agents and shells - each through the same service
//! the window calls, so the board on disk is the same whichever moon changed it.

use moon_core::{
    api::AgentKind,
    moontasks::{StartFolder, TaskResourceView, store},
};

use crate::app::{App, Overlay, PromptAsks, StartItem};

/// Moves the cards to the column `step` away, at the place the picked one has in its own.
pub fn move_cards_to_the_column_beside(app: &mut App, step: isize) {
    let target = app.column as isize + step;
    if target < 0 || target as usize >= app.board.columns.len() {
        return;
    }
    let cards = app.cards_to_move();
    let position = app.card_in_column[app.column];
    drop_cards_at(app, cards, target as usize, position);
}

/// Moves the picked card one place down (`step` 1) or up (-1) its column.
pub fn move_card_along_its_column(app: &mut App, step: isize) {
    let Some(task_id) = app.picked_task_id() else {
        return;
    };
    let column = app.board.columns[app.column].id.clone();
    let in_order: Vec<String> = app
        .board
        .cards_in(&column)
        .iter()
        .map(|task| task.id.clone())
        .collect();
    let Some(at) = in_order.iter().position(|id| *id == task_id) else {
        return;
    };
    let to = at as isize + step;
    if to < 0 || to as usize >= in_order.len() {
        return;
    }
    drop_cards_at(app, vec![task_id], app.column, to as usize);
}

/// Puts cards at `position` among the other cards of a column.
pub fn drop_cards_at(app: &mut App, cards: Vec<String>, column: usize, position: usize) {
    let Some(column_id) = app
        .board
        .columns
        .get(column)
        .map(|column| column.id.clone())
    else {
        return;
    };
    if cards.is_empty() {
        return;
    }
    let placed = app.session.place_cards(&cards, &column_id, position);
    if app.say_if_it_failed(placed).is_some() {
        app.read_the_board_again();
        app.pick_task(&cards[0]);
    }
}

/// Moves the picked column `step` places along the board.
pub fn move_the_column(app: &mut App, step: isize) {
    let to = app.column as isize + step;
    if to < 0 || to as usize >= app.board.columns.len() {
        return;
    }
    drop_column_at(app, app.column, to as usize);
}

pub fn drop_column_at(app: &mut App, column: usize, position: usize) {
    let Some(column_id) = app
        .board
        .columns
        .get(column)
        .map(|column| column.id.clone())
    else {
        return;
    };
    let placed = app.session.place_column(&column_id, position);
    if app.say_if_it_failed(placed).is_some() {
        let picks = app.card_in_column.remove(column);
        let scroll = app.scroll_in_column.remove(column);
        let position = position.min(app.card_in_column.len());
        app.card_in_column.insert(position, picks);
        app.scroll_in_column.insert(position, scroll);
        app.column = position;
        app.read_the_board_again();
    }
}

/// Offers, on `s`, what can be started on the picked task.
pub fn offer_what_to_start(app: &mut App) {
    let Some(task_id) = app.picked_task_id() else {
        return;
    };
    let available = &app.session.state.agent_availability;
    let mut items: Vec<StartItem> = [
        (AgentKind::Claude, available.claude),
        (AgentKind::Codex, available.codex),
        (AgentKind::Pi, available.pi),
        (AgentKind::OpenCode, available.opencode),
        (AgentKind::Minerve, available.minerve && app.offers_minerve),
    ]
    .into_iter()
    .filter(|(_, installed)| *installed)
    .map(|(agent, _)| StartItem::Agent(agent))
    .collect();
    items.push(StartItem::Shell(StartFolder::Repo));
    items.push(StartItem::Shell(StartFolder::TaskFolder));
    app.overlay = Some(Overlay::StartMenu {
        task_id,
        items,
        index: 0,
    });
}

/// Says what a start menu's line is called.
pub fn label_of(item: StartItem) -> String {
    match item {
        StartItem::Agent(agent) => agent.label().to_lowercase(),
        StartItem::Shell(StartFolder::Repo) => "shell, in the repo".to_string(),
        StartItem::Shell(StartFolder::TaskFolder) => "shell, in the task's folder".to_string(),
    }
}

pub fn start_on_the_task(app: &mut App, task_id: &str, item: StartItem) {
    let title = app
        .board
        .task(task_id)
        .map(|task| task.title.clone())
        .unwrap_or_default();
    let started = match item {
        StartItem::Agent(agent) => app.session.start_agent_on(task_id, agent),
        StartItem::Shell(opens_in) => app.session.start_shell_on(task_id, opens_in),
    };
    if let Some(terminal_id) = app.say_if_it_failed(started) {
        let name = format!("{} · {title}", label_of(item));
        open_a_tab_on(app, &terminal_id, &name);
        app.read_the_board_again();
    }
}

/// Shows a shell of this moon - see [`App::show_the_shell`].
pub fn open_a_tab_on(app: &mut App, terminal_id: &str, title: &str) {
    app.show_the_shell(terminal_id, title);
}

/// Opens a run on `enter`: its tab when this moon holds it, what it shows when another moon
/// does, and starts the run again when it is not going.
pub fn open_the_run(app: &mut App, task_id: &str, run: &TaskResourceView) {
    let title = crate::run_menu::name_of_the_run(
        run,
        app.board
            .task(task_id)
            .map(|task| task.title.as_str())
            .unwrap_or(""),
    );
    // A run another moon holds is not `running` here - that is only for this moon's shells.
    match (run.going_elsewhere_in, run.running, &run.terminal_id) {
        (Some(moon), _, Some(terminal_id)) if moon != std::process::id() => {
            let terminal_id = terminal_id.clone();
            app.open_the_run_elsewhere(moon, &terminal_id, &title);
        }
        (_, true, Some(terminal_id)) => {
            let terminal_id = terminal_id.clone();
            open_a_tab_on(app, &terminal_id, &title);
        }
        _ if run.resumable => resume_the_run(app, task_id, &run.id, &title),
        _ => app.flash(format!("{title} is not going, and cannot be resumed")),
    }
}

pub fn stop_the_run(app: &mut App, task_id: &str, run_id: &str) {
    let stopped = app.session.stop_run(task_id, run_id);
    if app.say_if_it_failed(stopped).is_some() {
        app.read_the_board_again();
    }
}

pub fn resume_the_run(app: &mut App, task_id: &str, run_id: &str, title: &str) {
    let resumed = app.session.resume_run(task_id, run_id);
    if let Some(terminal_id) = app.say_if_it_failed(resumed) {
        open_a_tab_on(app, &terminal_id, title);
        app.read_the_board_again();
    }
}

/// Acts on a line typed in a prompt, once it is done.
pub fn submit_the_line(app: &mut App, asks: PromptAsks, text: String) {
    let text = text.trim().to_string();
    match asks {
        PromptAsks::Filter => app.filter = text,
        PromptAsks::NewCard { column, joins } if !text.is_empty() => {
            let created = app.session.create_card(&text, &column, joins);
            if let Some(task_id) = app.say_if_it_failed(created) {
                app.read_the_board_again();
                app.pick_task(&task_id);
            }
        }
        PromptAsks::Rename { task_id } if !text.is_empty() => {
            let renamed = app.session.rename_card(&task_id, &text);
            if app.say_if_it_failed(renamed).is_some() {
                app.read_the_board_again();
            }
        }
        PromptAsks::Tags { task_id } => {
            let tags = store::tags_of(text.split(','));
            let set = app.session.set_card_tags(&task_id, &tags);
            if app.say_if_it_failed(set).is_some() {
                app.read_the_board_again();
            }
        }
        PromptAsks::Tell {
            terminal_id,
            held_by,
            title,
        } if !text.is_empty() => {
            let told = app.session.tell(&terminal_id, held_by, &text);
            if app.say_if_it_failed(told).is_some() {
                app.flash(format!("told {title}"));
            }
        }
        _ => {}
    }
}

impl App {
    /// Says whether a column, by its place on the board, is folded to a strip.
    pub fn column_is_folded(&self, column: usize) -> bool {
        self.board
            .columns
            .get(column)
            .is_some_and(|it| self.folded_columns.contains(it.id.as_str()))
    }

    /// Folds a column to a narrow strip, or unfolds one that is folded.
    pub fn fold_or_unfold_the_column(&mut self, column: usize) {
        let Some(id) = self.board.columns.get(column).map(|it| it.id.to_string()) else {
            return;
        };
        if !self.folded_columns.remove(&id) {
            self.folded_columns.insert(id);
        }
    }

    /// Shows every card on one line, its runs as marks after its title - or as they were.
    pub fn switch_one_line_cards(&mut self) {
        self.one_line_cards = !self.one_line_cards;
        self.picked_run = None;
        self.flash(if self.one_line_cards {
            "cards show one line each · C shows them whole".to_string()
        } else {
            "cards show whole again".to_string()
        });
    }
}
