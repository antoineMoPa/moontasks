//! This module records what was drawn where that a click acts on, for the mouse and for the
//! light under the pointer. Spots are recorded as they are drawn, so what is drawn on top -
//! a button on its task, a menu over the board - is recorded after what it covers, and the
//! last spot under the pointer is the one a click reaches.

use moon_core::moontasks::ColumnEnd;
use ratatui::layout::{Position, Rect};

/// Says what a spot on screen does when clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Picks a task, and carries it when dragged.
    Task {
        column: usize,
        task_id: String,
    },
    /// Carries a column when dragged.
    Heading(usize),
    /// Opens an agent or a shell of a task.
    Run {
        task_id: String,
        run_id: String,
    },
    /// Opens a task's details, which list everything it holds.
    Details(String),
    /// Runs a row of the menu that is open: a palette command, or what to start.
    MenuRow(usize),
    /// Folds a column, or unfolds it.
    Fold(usize),
    /// Asks a new task's name for one end of a column.
    NewTask {
        column: usize,
        joins: ColumnEnd,
    },
    /// Offers what to start on a task.
    StartOn(String),
    /// Ends an agent or a shell, keeping it on its task to resume.
    StopRun {
        task_id: String,
        run_id: String,
    },
    /// Starts a stopped agent or shell again.
    ResumeRun {
        task_id: String,
        run_id: String,
    },
    /// Ends an agent or a shell and takes it off its task - on a second click.
    DeleteRun {
        task_id: String,
        run_id: String,
    },
    /// Opens a task's notes in `$EDITOR`.
    EditNotes(String),
    EditFilter,
    ClearFilter,
    Quit,
    /// Closes the quitting question, staying.
    Stay,
    /// Closes the open menu or dialog: everywhere outside it.
    Close,
    /// Marks the inside of the open menu or dialog, which a click leaves open.
    Inside,
}

impl Hit {
    /// Says whether it is drawn as a button, which lights in the accent - the rest take the
    /// hover shade.
    pub fn is_a_button(&self) -> bool {
        matches!(
            self,
            Self::Fold(_)
                | Self::NewTask { .. }
                | Self::StartOn(_)
                | Self::StopRun { .. }
                | Self::ResumeRun { .. }
                | Self::DeleteRun { .. }
                | Self::EditNotes(_)
                | Self::ClearFilter
                | Self::Quit
                | Self::Stay
        )
    }
}

/// Records where things were drawn last, which is what a click is measured against.
#[derive(Default)]
pub struct HitMap {
    /// Holds what a click acts on, in the order it was drawn.
    pub spots: Vec<(Rect, Hit)>,
    /// Holds each column's area, which a dragged task lands in.
    pub columns: Vec<(Rect, usize)>,
    /// Holds the places columns are drawn in, by place on the board - which, while a column is
    /// carried, is not the column drawn there.
    pub column_slots: Vec<(Rect, usize)>,
}

impl HitMap {
    pub fn record(&mut self, rect: Rect, hit: Hit) {
        self.spots.push((rect, hit));
    }

    /// Returns what a click at `at` reaches: the spot drawn last there.
    pub fn find_the_spot_at(&self, at: Position) -> Option<&(Rect, Hit)> {
        self.spots.iter().rev().find(|(rect, _)| rect.contains(at))
    }

    /// Returns the tasks drawn, with their column, top to bottom.
    pub fn tasks_drawn(&self) -> impl Iterator<Item = (Rect, usize, &str)> {
        self.spots.iter().filter_map(|(rect, hit)| match hit {
            Hit::Task { column, task_id } => Some((*rect, *column, task_id.as_str())),
            _ => None,
        })
    }
}
