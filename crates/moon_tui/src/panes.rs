//! This module lists what the frames hold: the board, this moon's shells drawn live, and agents
//! other moons hold, watched. A run opened from the board goes into a frame at the board's
//! right - made the first time - and its tab can be dragged from there to a strip, an edge, or
//! another split, as in the window.

use std::sync::Arc;

use moon_core::instances;
use moon_tui_frames::{DropSide, PaneId, PaneView, Tab};
use ratatui::{Frame, crossterm::event::KeyEvent, layout::Rect};

use moon_tui_terminal::LiveTerminal;

use crate::{
    app::App,
    shell_links::{Attached, CoreShell},
    theme::{GOING, HANDLE},
    watched_run::{WatchedKey, WatchedRun},
};

/// Names a pane of the arrangement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoonPane {
    Board,
    /// Shows one of this moon's shells: the [`LiveTerminal`] in [`App::shells`] with this id.
    Shell {
        terminal_id: String,
    },
    /// Shows an agent another moon holds: the [`WatchedRun`] in [`App::watched`] with this id.
    Elsewhere {
        terminal_id: String,
    },
}

impl MoonPane {
    fn terminal_id(&self) -> Option<&str> {
        match self {
            Self::Board => None,
            Self::Shell { terminal_id } | Self::Elsewhere { terminal_id } => Some(terminal_id),
        }
    }
}

impl App {
    /// Says whether the keyboard is on the board.
    pub fn board_is_focused(&self) -> bool {
        matches!(self.layout.active_pane(), Some((_, MoonPane::Board)) | None)
    }

    /// Returns the shell the keyboard is in, if it is in one of this moon's.
    pub fn focused_shell(&mut self) -> Option<&mut LiveTerminal> {
        let terminal_id = self.focused_terminal_id()?;
        self.shells.get_mut(&terminal_id)
    }

    pub fn focused_terminal_id(&self) -> Option<String> {
        match self.layout.active_pane() {
            Some((_, MoonPane::Shell { terminal_id })) => Some(terminal_id.clone()),
            _ => None,
        }
    }

    /// Returns the agent another moon holds that the keyboard is on, if it is on one.
    pub fn focused_watched_run(&mut self) -> Option<&mut WatchedRun> {
        let terminal_id = match self.layout.active_pane() {
            Some((_, MoonPane::Elsewhere { terminal_id })) => terminal_id.clone(),
            _ => return None,
        };
        self.watched.get_mut(&terminal_id)
    }

    fn pane_on_terminal(&self, terminal_id: &str) -> Option<PaneId> {
        self.layout
            .find_pane(|pane| pane.terminal_id() == Some(terminal_id))
            .map(|(pane, _)| pane)
    }

    /// Shows one of this moon's shells: brought to the front where it is already, or put with
    /// the others beside the board.
    pub fn show_the_shell(&mut self, terminal_id: &str, title: &str) {
        if let Some(pane) = self.pane_on_terminal(terminal_id) {
            self.layout.focus_pane(pane);
            return;
        }
        let attached = self
            .session
            .attach(terminal_id)
            .and_then(|(output, session)| {
                LiveTerminal::new(
                    terminal_id.to_string(),
                    title.to_string(),
                    output,
                    Arc::new(CoreShell(session)),
                )
            });
        let Some(shell) = self.say_if_it_failed(attached) else {
            return;
        };
        self.shells.insert(terminal_id.to_string(), shell);
        self.put_beside_the_board(MoonPane::Shell {
            terminal_id: terminal_id.to_string(),
        });
    }

    /// Opens an agent another moon holds, beside the board where the shells go: attached to,
    /// and drawn and typed into as one of this moon's shells is - or, from a moon too old to
    /// share its shells, watched, and told lines.
    pub fn open_the_run_elsewhere(&mut self, moon: u32, terminal_id: &str, title: &str) {
        if let Some(pane) = self.pane_on_terminal(terminal_id) {
            self.layout.focus_pane(pane);
            return;
        }
        // It attaches at the size it will be drawn at, near enough: the pane fits it on its
        // first draw.
        let (cols, rows) = self.frames_area_beside_the_board();
        let attached =
            instances::attach::attach(moon, terminal_id, cols, rows).and_then(|(output, shell)| {
                LiveTerminal::new(
                    terminal_id.to_string(),
                    title.to_string(),
                    output,
                    Arc::new(Attached(shell)),
                )
            });
        match attached {
            Ok(shell) => {
                self.shells.insert(terminal_id.to_string(), shell);
                self.put_beside_the_board(MoonPane::Shell {
                    terminal_id: terminal_id.to_string(),
                });
            }
            Err(error) => {
                self.flash(format!(
                    "watching: moon {moon} holds this agent ({error:#})"
                ));
                self.watch_the_run_elsewhere(moon, terminal_id, title);
            }
        }
    }

    /// Returns the size a run opened beside the board is drawn at, about: half the screen's
    /// width.
    fn frames_area_beside_the_board(&self) -> (u16, u16) {
        let (width, height) = ratatui::crossterm::terminal::size().unwrap_or((160, 48));
        (
            (width / 2).saturating_sub(1).max(20),
            height.saturating_sub(3).max(5),
        )
    }

    /// Watches an agent another moon holds, beside the board where the shells go.
    fn watch_the_run_elsewhere(&mut self, moon: u32, terminal_id: &str, title: &str) {
        self.watched.insert(
            terminal_id.to_string(),
            WatchedRun::watch(moon, terminal_id, title),
        );
        self.put_beside_the_board(MoonPane::Elsewhere {
            terminal_id: terminal_id.to_string(),
        });
    }

    /// Puts a pane with the other runs - in a frame of their own at the board's right, the
    /// first time.
    fn put_beside_the_board(&mut self, pane: MoonPane) {
        let with_runs = self
            .layout
            .frame_holding(self.layout.active_frame(), |pane| {
                !matches!(pane, MoonPane::Board)
            });
        match with_runs {
            Some(frame) => {
                self.layout.add_pane(frame, pane, None);
            }
            None => {
                let board = self
                    .layout
                    .frame_of(self.board_pane)
                    .unwrap_or(self.layout.active_frame());
                self.layout.add_pane_beside(board, DropSide::Right, pane);
            }
        }
    }

    /// Types a key into a watched agent's pane: its line, which `enter` tells the agent.
    pub fn type_into_the_watched_run(&mut self, key: &KeyEvent) {
        let Some(watched) = self.focused_watched_run() else {
            return;
        };
        let WatchedKey::Tell(line) = watched.type_key(key) else {
            return;
        };
        let (moon, terminal_id, title) = (
            watched.moon,
            watched.terminal_id.clone(),
            watched.title.clone(),
        );
        let told = self.session.tell(&terminal_id, Some(moon), &line);
        if self.say_if_it_failed(told).is_some() {
            self.flash(format!("told {title}"));
        }
    }

    pub fn focus_the_board(&mut self) {
        self.layout.focus_pane(self.board_pane);
    }

    /// Lists the runs open, frame by frame and tab by tab - what `ctrl-a 1` to `9` count.
    pub fn shell_panes_in_order(&self) -> Vec<PaneId> {
        self.layout
            .frame_ids()
            .into_iter()
            .filter_map(|frame| self.layout.frame(frame))
            .flat_map(|frame| frame.panes().to_vec())
            .filter(|pane| !matches!(self.layout.pane(*pane), Some(MoonPane::Board) | None))
            .collect()
    }

    pub fn focus_the_nth_shell(&mut self, index: usize) {
        if let Some(pane) = self.shell_panes_in_order().get(index) {
            self.layout.focus_pane(*pane);
        }
    }

    pub fn close_the_focused_pane(&mut self) {
        if let Some((pane, _)) = self.layout.active_pane() {
            self.close_the_pane(pane);
        }
    }

    /// Closes the tab showing a terminal, if one does - its run has ended.
    pub fn close_the_tab_on(&mut self, terminal_id: &str) {
        if let Some(pane) = self.pane_on_terminal(terminal_id) {
            self.layout.close_pane(pane);
            self.shells.remove(terminal_id);
            self.watched.remove(terminal_id);
        }
        if self.layout.active_pane().is_none() {
            self.focus_the_board();
        }
    }

    /// Closes a run's pane. A plain shell ends with it; an agent goes on running on its card,
    /// where it can be opened again, and one another moon holds goes on there. The board does
    /// not close.
    pub fn close_the_pane(&mut self, pane: PaneId) {
        match self.layout.pane(pane).cloned() {
            Some(MoonPane::Shell { terminal_id }) => {
                self.layout.close_pane(pane);
                let title = self
                    .shells
                    .remove(&terminal_id)
                    .map(|shell| shell.title)
                    .unwrap_or_default();
                let is_an_agent = self.board.tasks.iter().any(|task| {
                    task.resources
                        .iter()
                        .any(|run| run.terminal_id.as_deref() == Some(terminal_id.as_str()))
                });
                if is_an_agent {
                    self.flash(format!("{title} keeps going on its task"));
                } else {
                    self.session.state.terminals.remove(&terminal_id);
                }
            }
            Some(MoonPane::Elsewhere { terminal_id }) => {
                self.layout.close_pane(pane);
                self.watched.remove(&terminal_id);
            }
            Some(MoonPane::Board) | None => return,
        }
        if self.layout.active_pane().is_none() {
            self.focus_the_board();
        }
    }

    pub fn close_the_panes_of_ended_shells(&mut self) {
        let ended: Vec<(String, String)> = self
            .shells
            .values()
            .filter(|shell| shell.has_exited())
            .map(|shell| (shell.terminal_id.clone(), shell.title.clone()))
            .collect();
        if ended.is_empty() {
            return;
        }
        for (terminal_id, _) in &ended {
            if let Some(pane) = self.pane_on_terminal(terminal_id) {
                self.layout.close_pane(pane);
            }
            self.shells.remove(terminal_id);
        }
        if self.layout.active_pane().is_none() {
            self.focus_the_board();
        }
        let titles: Vec<&str> = ended.iter().map(|(_, title)| title.as_str()).collect();
        self.flash(format!("{} ended", titles.join(", ")));
        self.read_the_board_again();
    }

    /// Says whether an agent's run in this shell is going, for the dot on its tab.
    fn shell_runs_an_agent(&self, terminal_id: &str) -> bool {
        self.board.tasks.iter().any(|task| {
            task.resources.iter().any(|run| {
                crate::run_menu::is_going(run) && run.terminal_id.as_deref() == Some(terminal_id)
            })
        })
    }
}

impl PaneView<MoonPane> for App {
    fn tab(&mut self, _id: PaneId, pane: &MoonPane) -> Tab {
        match pane {
            // The board's tab is named for its repo, which no other line says.
            MoonPane::Board => Tab::new(format!(
                "☾ {}",
                self.session
                    .repo_path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            ))
            .closable(false),
            MoonPane::Shell { terminal_id } => {
                let title = self
                    .shells
                    .get(terminal_id)
                    .map_or_else(|| terminal_id.clone(), |shell| shell.title.clone());
                let tab = Tab::new(title);
                if self.shell_runs_an_agent(terminal_id) {
                    tab.with_marker("●", GOING)
                } else {
                    tab
                }
            }
            MoonPane::Elsewhere { terminal_id } => Tab::new(
                self.watched
                    .get(terminal_id)
                    .map_or_else(|| terminal_id.clone(), |watched| watched.title.clone()),
            )
            .with_marker("◌", HANDLE),
        }
    }

    fn draw_pane(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _id: PaneId,
        pane: &MoonPane,
        focused: bool,
    ) {
        let no_overlay = self.overlay.is_none();
        let cursor = match pane {
            MoonPane::Board => return crate::draw::draw_the_board(frame, self, area, focused),
            MoonPane::Shell { terminal_id } => self.shells.get_mut(terminal_id).and_then(|shell| {
                let cursor = shell.draw_into(area, frame.buffer_mut());
                // A view scrolled back says so, and how far, at its top right.
                let back = shell.rows_scrolled_back();
                if back == 0 {
                    return cursor;
                }
                let label = format!(" ↑ {back} lines back ");
                let width = (label.chars().count() as u16).min(area.width);
                frame.render_widget(
                    ratatui::text::Line::styled(
                        label,
                        ratatui::style::Style::new()
                            .fg(crate::theme::ON_ACCENT)
                            .bg(HANDLE),
                    ),
                    Rect::new(area.right() - width, area.y, width, 1),
                );
                None
            }),
            MoonPane::Elsewhere { terminal_id } => self
                .watched
                .get(terminal_id)
                .and_then(|watched| watched.draw(frame, area)),
        };
        if focused
            && no_overlay
            && let Some(cursor) = cursor
        {
            frame.set_cursor_position(cursor);
        }
    }
}
