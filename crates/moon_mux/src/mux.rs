//! This module runs the multiplexer: its frames of shells, the keys after `ctrl-a`, the mouse,
//! and the loop that reads the shells and draws them.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use moon_tui_frames::{
    DropSide, Frames, FramesEvent, Layout, PaneId, PaneView, PointerOutcome, Tab,
};
use moon_tui_terminal::{LiveTerminal, screen::Screen};
use ratatui::{
    Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::pty_shell::PtyShell;

/// Sets how long the loop waits for a key before reading the shells again.
const TICK: Duration = Duration::from_millis(16);
/// Sets how long a second `ctrl-a q` has to quit.
const QUIT_AGAIN_WITHIN: Duration = Duration::from_secs(2);
/// Sets how long a message stays on the bottom line.
const FLASH_FOR: Duration = Duration::from_secs(3);
const ACCENT: Color = Color::Indexed(75);
const DIM: Color = Color::Indexed(244);

const KEYS_HELP: &str = "\
ctrl-a and then
  c                  open a new tab in this frame
  |  \\               split the frame, the new shell on the right
  -                  split the frame, the new shell below
  o  tab             move to the next frame
  n  p  → ←          show the next / previous tab of the frame
  1-9                show a tab, by its place
  x                  close the tab, ending its shell
  q                  quit, twice in a row - every shell ends
  ctrl-a             send a ctrl-a to the shell itself
  ?                  show or hide these keys

the mouse
  click a tab        shows it; its × closes it, and so does a middle click
  drag a tab         moves it along its strip, onto another frame, or against an edge to
                     split the frame
  drag a line        between two frames resizes them
  the wheel          scrolls back through a shell; shift and the wheel always does";

/// Names a pane: the shell it shows, by its number in [`Mux::shells`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MuxPane {
    shell: u64,
}

pub struct Mux {
    layout: Layout<MuxPane>,
    frames: Frames,
    /// Holds the shells drawn, by their number.
    shells: HashMap<u64, LiveTerminal>,
    next_shell: u64,
    folder: PathBuf,
    /// Says whether `ctrl-a` was pressed: the next key is the multiplexer's.
    prefix_armed: bool,
    help_open: bool,
    /// Holds when `ctrl-a q` was last pressed, which a second one soon after confirms.
    quit_asked_at: Option<Instant>,
    flash: Option<(String, Instant)>,
    quit: bool,
}

impl Mux {
    /// Starts the multiplexer with one tab, running `program` or the user's shell.
    pub fn start(program: Option<Vec<String>>, folder: PathBuf, screen: &Screen) -> Result<Self> {
        let size = screen.size()?;
        let mut mux = Self {
            layout: Layout::new(),
            frames: Frames::new(),
            shells: HashMap::new(),
            next_shell: 1,
            folder,
            prefix_armed: false,
            help_open: false,
            quit_asked_at: None,
            flash: None,
            quit: false,
        };
        let pane = mux.start_a_shell(
            program.as_deref(),
            size.width,
            size.height.saturating_sub(2),
        )?;
        mux.layout = Layout::with_pane(pane);
        Ok(mux)
    }

    /// Runs the loop until the last shell ends or `ctrl-a q` is pressed twice.
    pub fn run(&mut self, screen: &mut Screen) -> Result<()> {
        while !self.quit {
            for shell in self.shells.values_mut() {
                shell.read_what_was_printed();
            }
            self.close_the_panes_of_ended_shells();
            if self.quit {
                break;
            }
            screen.draw(|frame| self.draw_the_screen(frame))?;
            if event::poll(TICK)? {
                loop {
                    match event::read()? {
                        Event::Key(key) if key.kind != KeyEventKind::Release => self.press(key),
                        Event::Mouse(mouse) => self.handle_the_mouse(mouse),
                        Event::Paste(text) => {
                            if let Some(shell) = self.focused_shell() {
                                shell.paste(&text);
                            }
                        }
                        _ => {}
                    }
                    if self.quit || !event::poll(Duration::ZERO)? {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Starts a shell, keeps it, and returns the pane that shows it - not yet placed.
    fn start_a_shell(
        &mut self,
        program: Option<&[String]>,
        cols: u16,
        rows: u16,
    ) -> Result<MuxPane> {
        let (output, shell) = PtyShell::start(program, &self.folder, cols.max(2), rows.max(1))?;
        let number = self.next_shell;
        self.next_shell += 1;
        let title = program
            .and_then(|words| words.first())
            .map_or_else(|| format!("shell {number}"), |first| first.clone());
        let live = LiveTerminal::new(number.to_string(), title, output, shell as Arc<_>)?;
        self.shells.insert(number, live);
        Ok(MuxPane { shell: number })
    }

    /// Opens a new shell in the frame the keyboard is in, as a tab, or beside it.
    fn open_a_shell(&mut self, beside: Option<DropSide>) {
        let frame = self.layout.active_frame();
        let (cols, rows) = self
            .layout
            .active_pane()
            .and_then(|(pane, _)| self.frames.pane_area(pane))
            .map_or((80, 24), |area| (area.width, area.height));
        let (cols, rows) = match beside {
            Some(DropSide::Right) => (cols / 2, rows),
            Some(DropSide::Bottom) => (cols, rows / 2),
            _ => (cols, rows),
        };
        let started = self.start_a_shell(None, cols, rows);
        let pane = match started {
            Ok(pane) => pane,
            Err(error) => return self.flash(format!("could not start a shell: {error:#}")),
        };
        let id = match beside {
            Some(side) => self.layout.add_pane_beside(frame, side, pane),
            None => self.layout.add_pane(frame, pane, None),
        };
        self.layout.focus_pane(id);
    }

    fn focused_shell(&mut self) -> Option<&mut LiveTerminal> {
        let shell = self.layout.active_pane()?.1.shell;
        self.shells.get_mut(&shell)
    }

    /// Closes a tab, ending its shell.
    fn close_the_pane(&mut self, pane: PaneId) {
        if let Some(closed) = self.layout.close_pane(pane) {
            self.shells.remove(&closed.shell);
        }
        self.quit = self.layout.is_empty();
    }

    fn close_the_panes_of_ended_shells(&mut self) {
        let ended: Vec<u64> = self
            .shells
            .iter()
            .filter(|(_, shell)| shell.has_exited())
            .map(|(number, _)| *number)
            .collect();
        for number in ended {
            if let Some((pane, _)) = self.layout.find_pane(|pane| pane.shell == number) {
                self.close_the_pane(pane);
            } else {
                self.shells.remove(&number);
            }
        }
    }

    fn flash(&mut self, message: String) {
        self.flash = Some((message, Instant::now()));
    }

    // ---- keys ----

    fn press(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.prefix_armed {
            self.prefix_armed = false;
            return self.press_after_the_prefix(key);
        }
        if ctrl && key.code == KeyCode::Char('a') {
            self.prefix_armed = true;
            return;
        }
        if self.help_open && key.code == KeyCode::Esc {
            self.help_open = false;
            return;
        }
        if let Some(shell) = self.focused_shell() {
            shell.type_key(&key);
        }
    }

    fn press_after_the_prefix(&mut self, key: KeyEvent) {
        if key.code != KeyCode::Char('q') {
            self.quit_asked_at = None;
        }
        match key.code {
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(shell) = self.focused_shell() {
                    shell.type_key(&key);
                }
            }
            KeyCode::Char('c') => self.open_a_shell(None),
            KeyCode::Char('|' | '\\' | '%') => self.open_a_shell(Some(DropSide::Right)),
            KeyCode::Char('-' | '"') => self.open_a_shell(Some(DropSide::Bottom)),
            KeyCode::Char('o') | KeyCode::Tab | KeyCode::Up | KeyCode::Down => {
                self.layout.focus_next_frame();
            }
            KeyCode::Char('n') | KeyCode::Right => {
                self.layout.focus_tab_after(1);
            }
            KeyCode::Char('p') | KeyCode::Left => {
                self.layout.focus_tab_after(-1);
            }
            KeyCode::Char(digit @ '1'..='9') => {
                if let Some(pane) = self.panes_in_order().get(digit as usize - '1' as usize) {
                    self.layout.focus_pane(*pane);
                }
            }
            KeyCode::Char('x') => {
                if let Some((pane, _)) = self.layout.active_pane() {
                    self.close_the_pane(pane);
                }
            }
            KeyCode::Char('q') => self.ask_to_quit(),
            KeyCode::Char('?') => self.help_open = !self.help_open,
            _ => {}
        }
    }

    /// Quits on a second `ctrl-a q` soon after the first, which says what quitting ends.
    fn ask_to_quit(&mut self) {
        if self
            .quit_asked_at
            .is_some_and(|at| at.elapsed() <= QUIT_AGAIN_WITHIN)
        {
            self.quit = true;
            return;
        }
        self.quit_asked_at = Some(Instant::now());
        let count = self.shells.len();
        self.flash(format!(
            "ctrl-a q again quits, ending {count} shell{}",
            if count == 1 { "" } else { "s" }
        ));
    }

    /// Lists the panes frame by frame and tab by tab - what `ctrl-a 1` to `9` count.
    fn panes_in_order(&self) -> Vec<PaneId> {
        self.layout
            .frame_ids()
            .into_iter()
            .filter_map(|frame| self.layout.frame(frame))
            .flat_map(|frame| frame.panes().to_vec())
            .collect()
    }

    // ---- the mouse ----

    fn handle_the_mouse(&mut self, mouse: event::MouseEvent) {
        match self.frames.handle_the_mouse(mouse, &mut self.layout) {
            PointerOutcome::Taken(events) => {
                for happened in events {
                    if let FramesEvent::PaneCloseRequested(pane) = happened {
                        self.close_the_pane(pane);
                    }
                }
            }
            PointerOutcome::InPane { pane, area } => {
                let shell = self.layout.pane(pane).map(|pane| pane.shell);
                if let Some(shell) = shell.and_then(|shell| self.shells.get_mut(&shell)) {
                    shell.handle_the_mouse(&mouse, area);
                }
            }
            PointerOutcome::Missed => {}
        }
    }

    // ---- drawing ----

    fn draw_the_screen(&mut self, frame: &mut Frame) {
        let [body, foot] =
            ratatui::layout::Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
                .areas(frame.area());
        // The frames draw the panes, and the multiplexer draws the shells in them: both are
        // lent out.
        let layout = std::mem::take(&mut self.layout);
        let mut frames = std::mem::take(&mut self.frames);
        frames.draw(frame, body, &layout, self);
        self.layout = layout;
        self.frames = frames;
        self.draw_the_foot(frame, foot);
        if self.help_open {
            draw_the_keys_help(frame, body);
        }
    }

    fn draw_the_foot(&mut self, frame: &mut Frame, area: Rect) {
        if let Some((message, at)) = &self.flash {
            if at.elapsed() < FLASH_FOR {
                frame.render_widget(
                    Line::styled(format!(" {message}"), Style::new().fg(Color::Yellow)),
                    area,
                );
                return;
            }
            self.flash = None;
        }
        let keys: &[(&str, &str)] = if self.prefix_armed {
            &[
                ("c", "new tab"),
                ("|", "split right"),
                ("-", "split down"),
                ("o", "next frame"),
                ("n p", "next/prev tab"),
                ("x", "close tab"),
                ("q", "quit"),
                ("?", "keys"),
            ]
        } else {
            &[(
                "ctrl-a",
                "then c opens a tab · | splits right · - splits down · ? shows keys",
            )]
        };
        let mut spans = vec![Span::styled(" moon-mux ", Style::new().fg(ACCENT))];
        for (key, says) in keys {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::new().fg(Color::Black).bg(if self.prefix_armed {
                    ACCENT
                } else {
                    Color::Gray
                }),
            ));
            spans.push(Span::styled(format!(" {says}  "), Style::new().fg(DIM)));
        }
        frame.render_widget(Line::from(spans), area);
    }
}

fn draw_the_keys_help(frame: &mut Frame, area: Rect) {
    let lines: Vec<Line> = KEYS_HELP.lines().map(Line::raw).collect();
    let width = (KEYS_HELP
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0) as u16
        + 4)
    .min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let at = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, at);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(ACCENT))
                .title(" keys · ctrl-a ? or esc hides them "),
        ),
        at,
    );
}

impl PaneView<MuxPane> for Mux {
    fn tab(&mut self, _id: PaneId, pane: &MuxPane) -> Tab {
        let title = self.shells.get(&pane.shell).map_or_else(
            || format!("shell {}", pane.shell),
            |shell| {
                shell
                    .title_set_by_the_program()
                    .unwrap_or_else(|| shell.title.clone())
            },
        );
        Tab::new(title)
    }

    fn draw_pane(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _id: PaneId,
        pane: &MuxPane,
        focused: bool,
    ) {
        let help_open = self.help_open;
        let Some(shell) = self.shells.get_mut(&pane.shell) else {
            return;
        };
        let cursor = shell.draw_into(area, frame.buffer_mut());
        let back = shell.rows_scrolled_back();
        if back > 0 {
            let label = format!(" ↑ {back} lines back · type to return ");
            let width = (label.chars().count() as u16).min(area.width);
            frame.render_widget(
                Line::styled(label, Style::new().fg(Color::Black).bg(ACCENT)),
                Rect::new(area.right() - width, area.y, width, 1),
            );
        } else if focused
            && !help_open
            && let Some(cursor) = cursor
        {
            frame.set_cursor_position(cursor);
        }
    }
}
