//! This module runs the board in a terminal: it keeps what is on screen and what is picked, and
//! holds the loop that reads keys, mouse and the shells' output and draws it all again.

use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

use anyhow::Result;
use moon_core::{
    api::AgentKind,
    moontasks::{
        ColumnEnd, ReviewRequestView, StartFolder, TaskResourceView, TaskView,
        store::{ColumnId, TaskResourceKind},
    },
};
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Position, Rect},
};

use moon_tui_frames::{Frames, Layout, PaneId};

use moon_tui_terminal::{
    LiveTerminal,
    screen::{self, Screen},
};

use crate::{
    board_session::{BoardSession, BoardSnapshot},
    line_input::{LineEdit, LineInput},
    panes::MoonPane,
    watched_run::WatchedRun,
};

/// Sets how often the board is read off disk again, for what agents and other moons changed.
const BOARD_READ_EVERY: Duration = Duration::from_millis(1500);
/// Sets how long the review requests' thread rests between two readings.
const REVIEWS_READ_EVERY: Duration = Duration::from_secs(5);
/// Sets how long a key or the shells are waited on before the screen is looked at again.
const TICK: Duration = Duration::from_millis(30);
/// Sets how long a message in the status bar stays.
const FLASH_FOR: Duration = Duration::from_secs(4);

/// Says what a line being typed is for.
pub enum PromptAsks {
    NewCard {
        column: ColumnId,
        joins: ColumnEnd,
    },
    Rename {
        task_id: String,
    },
    Tags {
        task_id: String,
    },
    Tell {
        terminal_id: String,
        held_by: Option<u32>,
        title: String,
    },
    Filter,
}

/// Lists what `s` offers to start on a task.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StartItem {
    Agent(AgentKind),
    Shell(StartFolder),
}

/// Says what is drawn over the board, taking the keys while it is.
pub enum Overlay {
    Prompt {
        asks: PromptAsks,
        line: LineInput,
    },
    StartMenu {
        task_id: String,
        items: Vec<StartItem>,
        index: usize,
    },
    Text {
        title: String,
        body: String,
    },
    /// Shows a card's runs, to open, stop, resume or tell.
    RunMenu {
        task_id: String,
        index: usize,
    },
    ConfirmQuit,
}

/// Holds what a drag carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dragged {
    Cards(Vec<String>),
    Column(usize),
}

pub struct Drag {
    pub what: Dragged,
    pub from: Position,
    pub at: Position,
    /// Says whether it has moved far enough to be a drag rather than a click.
    pub moving: bool,
}

/// Records where things were drawn last, which is what a click is measured against.
#[derive(Default)]
pub struct HitMap {
    pub cards: Vec<(Rect, usize, String)>,
    pub headings: Vec<(Rect, usize)>,
    pub columns: Vec<(Rect, usize)>,
    /// Holds the places columns are drawn in, by place on the board - which, while a column is
    /// carried, is not the column drawn there.
    pub column_slots: Vec<(Rect, usize)>,
    /// Holds the runs listed on cards: the task, and the run.
    pub card_runs: Vec<(Rect, String, String)>,
    /// Holds the `▾` on each heading and each folded column's strip, which a click folds or
    /// unfolds.
    pub folds: Vec<(Rect, usize)>,
}

pub struct App {
    pub session: BoardSession,
    pub board: BoardSnapshot,
    last_read: Instant,
    review_requests: Receiver<Vec<ReviewRequestView>>,
    /// Holds the board and the shells, in frames.
    pub layout: Layout<MoonPane>,
    pub frames: Frames,
    pub board_pane: PaneId,
    /// Holds this moon's shells drawn live, by terminal id.
    pub shells: HashMap<String, LiveTerminal>,
    /// Keeps the agents other moons hold, watched, by terminal id.
    pub watched: HashMap<String, WatchedRun>,
    /// Holds the picked column, by its place on the board.
    pub column: usize,
    /// Holds the card picked in each column, by its place among the cards the filter shows.
    pub card_in_column: Vec<usize>,
    /// Holds the first card drawn in each column, for columns longer than the screen.
    pub scroll_in_column: Vec<usize>,
    pub marked: BTreeSet<String>,
    pub filter: String,
    pub task_pane_open: bool,
    /// Holds the columns folded to a narrow strip, by column id.
    pub folded_columns: BTreeSet<String>,
    /// Says whether cards show their title alone, on one line, with their runs as marks.
    pub one_line_cards: bool,
    /// Holds a run picked on the picked card, by its place among the runs the card lists -
    /// `None` when it is the card itself that is picked.
    pub picked_run: Option<usize>,
    pub overlay: Option<Overlay>,
    flash: Option<(String, Instant)>,
    /// `ctrl-a` was pressed: the next key is moon's rather than the shell's.
    pub prefix_armed: bool,
    pub drag: Option<Drag>,
    /// Holds the card last clicked, and when - a second click on it soon after is a double click.
    pub last_click: Option<(String, Instant)>,
    pub hits: HitMap,
    pub edit_notes_at: Option<PathBuf>,
    /// Says whether Minerve can be started here: it runs as `moon __minerve`, which only the
    /// full moon is.
    pub offers_minerve: bool,
    pub quit: bool,
}

impl App {
    pub fn new(session: BoardSession, offers_minerve: bool) -> Result<Self> {
        let board = session.read_the_board()?;
        let column = board
            .columns
            .iter()
            .position(|column| column.id == ColumnId::new("in_progress"))
            .unwrap_or(0);
        let layout = Layout::with_pane(MoonPane::Board);
        let board_pane = layout
            .active_pane()
            .map(|(pane, _)| pane)
            .expect("a layout made with a pane has it in front");
        Ok(Self {
            review_requests: session.watch_the_review_requests(REVIEWS_READ_EVERY),
            card_in_column: vec![0; board.columns.len()],
            scroll_in_column: vec![0; board.columns.len()],
            session,
            board,
            last_read: Instant::now(),
            layout,
            frames: Frames::new(),
            board_pane,
            shells: HashMap::new(),
            watched: HashMap::new(),
            column,
            marked: BTreeSet::new(),
            filter: String::new(),
            task_pane_open: false,
            folded_columns: BTreeSet::new(),
            one_line_cards: false,
            picked_run: None,
            overlay: None,
            flash: None,
            prefix_armed: false,
            drag: None,
            last_click: None,
            hits: HitMap::default(),
            edit_notes_at: None,
            offers_minerve,
            quit: false,
        })
    }

    /// Runs the loop: reads the shells, draws, waits a tick for keys and the mouse, again -
    /// until `q`.
    pub fn run(&mut self, screen: &mut Screen) -> Result<()> {
        while !self.quit {
            if self.last_read.elapsed() >= BOARD_READ_EVERY {
                self.read_the_board_again();
            }
            if let Some(requests) = self.review_requests.try_iter().last() {
                self.board.review_requests = requests;
            }
            for shell in self.shells.values_mut() {
                shell.read_what_was_printed();
            }
            self.close_the_panes_of_ended_shells();
            screen.draw(|frame| crate::draw::draw_the_screen(frame, self))?;
            if event::poll(TICK)? {
                loop {
                    let happened = event::read()?;
                    self.handle(happened);
                    if self.quit || !event::poll(Duration::ZERO)? {
                        break;
                    }
                }
            }
            if let Some(path) = self.edit_notes_at.take() {
                let edited = screen::edit_in_the_editor(screen, &path);
                self.say_if_it_failed(edited);
                self.read_the_board_again();
            }
        }
        Ok(())
    }

    fn handle(&mut self, happened: Event) {
        match happened {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.press(key),
            Event::Mouse(mouse) => crate::pointer::handle_the_mouse(self, mouse),
            Event::Paste(text) => self.paste(&text),
            _ => {}
        }
    }

    pub fn read_the_board_again(&mut self) {
        match self.session.read_the_board() {
            Ok(board) => {
                let review_requests = std::mem::take(&mut self.board.review_requests);
                self.board = BoardSnapshot {
                    review_requests,
                    ..board
                };
                let columns = self.board.columns.len();
                self.card_in_column.resize(columns, 0);
                self.scroll_in_column.resize(columns, 0);
                self.column = self.column.min(columns.saturating_sub(1));
                let tasks: BTreeSet<String> = self
                    .board
                    .tasks
                    .iter()
                    .map(|task| task.id.clone())
                    .collect();
                self.marked.retain(|task_id| tasks.contains(task_id));
            }
            Err(error) => self.flash(format!("could not read the board: {error:#}")),
        }
        self.last_read = Instant::now();
    }

    pub fn flash(&mut self, message: String) {
        self.flash = Some((message, Instant::now()));
    }

    pub fn flashed(&self) -> Option<&str> {
        self.flash
            .as_ref()
            .filter(|(_, at)| at.elapsed() < FLASH_FOR)
            .map(|(message, _)| message.as_str())
    }

    pub fn say_if_it_failed<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(done) => Some(done),
            Err(error) => {
                self.flash(format!("{error:#}"));
                None
            }
        }
    }

    // ---- what is picked ----

    /// Returns the cards of a column the filter shows, in the board's order.
    pub fn cards_shown_in(&self, column: usize) -> Vec<&TaskView> {
        let Some(column) = self.board.columns.get(column) else {
            return Vec::new();
        };
        let wanted = self.filter.to_lowercase();
        self.board
            .cards_in(&column.id)
            .into_iter()
            .filter(|task| wanted.is_empty() || card_matches(task, &wanted))
            .collect()
    }

    pub fn picked_card(&self) -> Option<&TaskView> {
        let cards = self.cards_shown_in(self.column);
        let index = self.card_in_column.get(self.column).copied().unwrap_or(0);
        cards.get(index.min(cards.len().saturating_sub(1))).copied()
    }

    pub fn picked_task_id(&self) -> Option<String> {
        self.picked_card().map(|task| task.id.clone())
    }

    /// Returns the cards a move takes: the marked ones, or the picked one when none is marked.
    pub fn cards_to_move(&self) -> Vec<String> {
        if self.marked.is_empty() {
            self.picked_task_id().into_iter().collect()
        } else {
            // They come in the board's order, which is the order they keep.
            self.board
                .tasks
                .iter()
                .filter(|task| self.marked.contains(&task.id))
                .map(|task| task.id.clone())
                .collect()
        }
    }

    /// Picks a card by its task, wherever it is.
    pub fn pick_task(&mut self, task_id: &str) {
        for column in 0..self.board.columns.len() {
            if let Some(index) = self
                .cards_shown_in(column)
                .iter()
                .position(|task| task.id == task_id)
            {
                self.column = column;
                self.card_in_column[column] = index;
                self.picked_run = None;
                return;
            }
        }
    }

    /// Returns every agent's run on the board that is going.
    pub fn agents_going(&self) -> Vec<&TaskResourceView> {
        self.board
            .tasks
            .iter()
            .flat_map(|task| &task.resources)
            .filter(|run| run.kind == TaskResourceKind::Agent && crate::run_menu::is_going(run))
            .collect()
    }

    // ---- keys ----

    fn press(&mut self, key: KeyEvent) {
        if self.overlay.is_some() {
            return self.press_on_the_overlay(key);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.prefix_armed {
            self.prefix_armed = false;
            return self.press_after_the_prefix(key);
        }
        if ctrl && key.code == KeyCode::Char('a') {
            self.prefix_armed = true;
            return;
        }
        if self.board_is_focused() {
            crate::board_keys::press_on_the_board(self, key);
        } else if let Some(shell) = self.focused_shell() {
            shell.type_key(&key);
        } else {
            self.type_into_the_watched_run(&key);
        }
    }

    /// Handles the key after `ctrl-a`: the frames and their tabs, from anywhere.
    fn press_after_the_prefix(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(shell) = self.focused_shell() {
                    shell.type_key(&key);
                }
            }
            KeyCode::Char('o') | KeyCode::Tab => {
                self.layout.focus_next_frame();
            }
            KeyCode::Char('n') | KeyCode::Right => {
                self.layout.focus_tab_after(1);
            }
            KeyCode::Char('p') | KeyCode::Left => {
                self.layout.focus_tab_after(-1);
            }
            KeyCode::Char('b') | KeyCode::Char('0') => self.focus_the_board(),
            KeyCode::Char(digit @ '1'..='9') => {
                self.focus_the_nth_shell(digit as usize - '1' as usize)
            }
            KeyCode::Char('x') => self.close_the_focused_pane(),
            KeyCode::Char('t') => {
                if let Some(terminal_id) = self.focused_terminal_id() {
                    let title = self
                        .shells
                        .get(&terminal_id)
                        .map(|shell| shell.title.clone())
                        .unwrap_or_default();
                    let asks = PromptAsks::Tell {
                        terminal_id,
                        held_by: None,
                        title,
                    };
                    self.ask_for_a_line(asks, "");
                }
            }
            KeyCode::Char('?') => self.show_the_keys(),
            KeyCode::Char('q') => self.ask_to_quit(),
            _ => {}
        }
    }

    fn paste(&mut self, text: &str) {
        if let Some(Overlay::Prompt { line, .. }) = &mut self.overlay {
            line.type_text(text);
        } else if self.overlay.is_none()
            && let Some(shell) = self.focused_shell()
        {
            shell.paste(text);
        } else if self.overlay.is_none()
            && let Some(watched) = self.focused_watched_run()
        {
            watched.paste(text);
        }
    }

    pub fn ask_for_a_line(&mut self, asks: PromptAsks, holding: &str) {
        self.overlay = Some(Overlay::Prompt {
            asks,
            line: LineInput::holding(holding),
        });
    }

    pub fn show_the_keys(&mut self) {
        self.overlay = Some(Overlay::Text {
            title: "keys".to_string(),
            body: crate::board_keys::KEYS_HELP.to_string(),
        });
    }

    pub fn ask_to_quit(&mut self) {
        let own = std::process::id();
        let going_here = self
            .agents_going()
            .iter()
            .filter(|run| run.going_elsewhere_in.is_none_or(|pid| pid == own))
            .count();
        if going_here == 0 && self.shells.is_empty() {
            self.quit = true;
        } else {
            self.overlay = Some(Overlay::ConfirmQuit);
        }
    }

    fn press_on_the_overlay(&mut self, key: KeyEvent) {
        let Some(overlay) = self.overlay.take() else {
            return;
        };
        match overlay {
            Overlay::Prompt { asks, mut line } => match line.press(&key) {
                LineEdit::Editing => {
                    if matches!(asks, PromptAsks::Filter) {
                        self.filter = line.text.clone();
                        self.card_in_column.iter_mut().for_each(|index| *index = 0);
                    }
                    self.overlay = Some(Overlay::Prompt { asks, line });
                }
                LineEdit::Cancelled => {
                    if matches!(asks, PromptAsks::Filter) {
                        self.filter.clear();
                    }
                }
                LineEdit::Submitted => crate::board_actions::submit_the_line(self, asks, line.text),
            },
            Overlay::StartMenu {
                task_id,
                items,
                mut index,
            } => match key.code {
                KeyCode::Down => {
                    index = (index + 1) % items.len();
                    self.overlay = Some(Overlay::StartMenu {
                        task_id,
                        items,
                        index,
                    });
                }
                KeyCode::Up => {
                    index = (index + items.len() - 1) % items.len();
                    self.overlay = Some(Overlay::StartMenu {
                        task_id,
                        items,
                        index,
                    });
                }
                KeyCode::Enter => {
                    crate::board_actions::start_on_the_task(self, &task_id, items[index])
                }
                KeyCode::Esc | KeyCode::Char('q') => {}
                _ => {
                    self.overlay = Some(Overlay::StartMenu {
                        task_id,
                        items,
                        index,
                    })
                }
            },
            Overlay::RunMenu { task_id, index } => {
                crate::run_menu::press_on_the_run_menu(self, task_id, index, key)
            }
            Overlay::Text { .. } => {}
            Overlay::ConfirmQuit => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => self.quit = true,
                _ => {}
            },
        }
    }
}

/// Says whether a card holds what the filter says, in its title, tags or notes.
fn card_matches(task: &TaskView, wanted: &str) -> bool {
    task.title.to_lowercase().contains(wanted)
        || task
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(wanted))
        || task.notes.to_lowercase().contains(wanted)
}
