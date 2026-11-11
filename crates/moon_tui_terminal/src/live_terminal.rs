//! This module draws a shell live in a tab: what it prints goes through Ghostty's emulator, and
//! its grid is copied cell by cell into the frame. Keys go back to it encoded the way Ghostty
//! encodes them.

use std::sync::{Arc, mpsc};

use anyhow::{Result, anyhow};
use libghostty_vt::{
    Terminal as Emulator, TerminalOptions,
    key::{self, Action},
    mouse as vt_mouse,
    render::{CellIterator, RenderState, RowIterator},
    style::RgbColor,
    terminal::Mode,
};
use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyEvent,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::Line,
};

use crate::terminal_keys::vt_keystroke_of;

mod mouse;

/// Sets how many rows of what scrolled off the top a tab keeps.
const KEPT_ROWS: usize = 5_000;

/// Leads into a shell drawn here: a PTY this process holds, or a socket to a shell another
/// process holds.
pub trait ShellLink: Send + Sync {
    fn write_input(&self, bytes: &[u8]) -> Result<()>;
    fn write_reply(&self, bytes: &[u8]) -> Result<()>;
    fn resize(&self, cols: u16, rows: u16) -> Result<()>;
    fn has_exited(&self) -> bool;
}

/// Holds a shell drawn in a tab.
pub struct LiveTerminal {
    pub terminal_id: String,
    /// Holds what the tab is called: the run's name, the agent and the task.
    pub title: String,
    emulator: Emulator<'static, 'static>,
    render_state: RenderState<'static>,
    row_iterator: RowIterator<'static>,
    cell_iterator: CellIterator<'static>,
    encoder: key::Encoder<'static>,
    mouse_encoder: vt_mouse::Encoder<'static>,
    mouse_event: vt_mouse::Event<'static>,
    /// Says whether a button is held down over a program reading the mouse, which its drags
    /// are reported with.
    mouse_button_down: bool,
    output: mpsc::Receiver<Vec<u8>>,
    session: Arc<dyn ShellLink>,
    cols: u16,
    rows: u16,
}

impl LiveTerminal {
    /// Makes a tab on a shell, given what it has printed so far and everything it prints next.
    pub fn new(
        terminal_id: String,
        title: String,
        output: mpsc::Receiver<Vec<u8>>,
        session: Arc<dyn ShellLink>,
    ) -> Result<Self> {
        let (cols, rows) = (80, 24);
        let mut emulator = Emulator::new(TerminalOptions {
            cols,
            rows,
            max_scrollback: KEPT_ROWS,
        })
        .map_err(|error| anyhow!("failed to create a terminal: {error}"))?;
        // Programs ask the terminal things as they start - its size, its colors - and wait
        // for the answers, which go back to them as replies rather than as typing.
        let reply_to = Arc::clone(&session);
        emulator
            .on_pty_write(move |_emulator, data| {
                let _ = reply_to.write_reply(data);
            })
            .map_err(|error| anyhow!("failed to install the terminal reply hook: {error}"))?;
        Ok(Self {
            terminal_id,
            title,
            emulator,
            render_state: RenderState::new()
                .map_err(|error| anyhow!("failed to create a render state: {error}"))?,
            row_iterator: RowIterator::new()
                .map_err(|error| anyhow!("failed to create a row iterator: {error}"))?,
            cell_iterator: CellIterator::new()
                .map_err(|error| anyhow!("failed to create a cell iterator: {error}"))?,
            encoder: key::Encoder::new()
                .map_err(|error| anyhow!("failed to create a key encoder: {error}"))?,
            mouse_encoder: vt_mouse::Encoder::new()
                .map_err(|error| anyhow!("failed to create a mouse encoder: {error}"))?,
            mouse_event: vt_mouse::Event::new()
                .map_err(|error| anyhow!("failed to create a mouse event: {error}"))?,
            mouse_button_down: false,
            output,
            session,
            cols,
            rows,
        })
    }

    /// Feeds the emulator what the shell printed since last time. Says whether it printed
    /// anything, which is when the tab needs drawing again.
    pub fn read_what_was_printed(&mut self) -> bool {
        let mut printed = false;
        while let Ok(bytes) = self.output.try_recv() {
            self.emulator.vt_write(&bytes);
            printed = true;
        }
        printed
    }

    pub fn has_exited(&self) -> bool {
        self.session.has_exited()
    }

    /// Makes the shell the size of the area it is drawn in, telling the program as well.
    pub fn fit_to(&mut self, area: Rect) {
        let (cols, rows) = (area.width.max(2), area.height.max(1));
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        let _ = self.emulator.resize(cols, rows, 8, 16);
        let _ = self.session.resize(cols, rows);
    }

    /// Copies the screen into `buffer` at `area`, and says where the cursor is, when it shows.
    pub fn draw_into(&mut self, area: Rect, buffer: &mut Buffer) -> Option<Position> {
        self.fit_to(area);
        let snapshot = self.render_state.update(&self.emulator).ok()?;
        let mut rows = self.row_iterator.update(&snapshot).ok()?;
        let mut text = String::with_capacity(8);
        let mut y = area.y;
        while let Some(row) = rows.next() {
            if y >= area.bottom() {
                break;
            }
            let Ok(mut cells) = self.cell_iterator.update(row) else {
                break;
            };
            let mut x = area.x;
            // This is the column a wide character drawn before it still covers.
            let mut covered_until = area.x;
            while let Some(cell) = cells.next() {
                if x >= area.right() {
                    break;
                }
                text.clear();
                if cell.graphemes_len().unwrap_or(0) > 0 {
                    let _ = cell.graphemes_utf8(&mut text);
                }
                if x >= covered_until {
                    let mut style = Style::default();
                    if let Ok(Some(fg)) = cell.fg_color() {
                        style = style.fg(color_of(fg));
                    }
                    if let Ok(Some(bg)) = cell.bg_color() {
                        style = style.bg(color_of(bg));
                    }
                    if cell.has_styling().unwrap_or(false)
                        && let Ok(cell_style) = cell.style()
                    {
                        style = style.add_modifier(modifiers_of(&cell_style));
                    }
                    let symbol = if text.is_empty() { " " } else { text.as_str() };
                    let width = Line::raw(symbol).width().max(1) as u16;
                    if let Some(drawn) = buffer.cell_mut((x, y)) {
                        drawn.set_symbol(symbol).set_style(style);
                    }
                    covered_until = x + width;
                }
                x += 1;
            }
            y += 1;
        }

        snapshot
            .cursor_visible()
            .unwrap_or(false)
            .then(|| snapshot.cursor_viewport().ok().flatten())
            .flatten()
            .map(|at| Position::new(area.x + at.x, area.y + at.y))
            .filter(|at| area.contains(*at))
    }

    /// Types a key into the shell.
    pub fn type_key(&mut self, event: &KeyEvent) {
        let Some(stroke) = vt_keystroke_of(event) else {
            return;
        };
        // Typing is done at the bottom, where the shell is writing.
        self.scroll_to_the_bottom();
        self.encoder
            .set_options_from_terminal(&self.emulator)
            .set_macos_option_as_alt(key::OptionAsAlt::True);
        let Ok(mut vt_event) = key::Event::new() else {
            return;
        };
        vt_event.set_action(Action::Press);
        vt_event.set_key(stroke.key);
        vt_event.set_mods(stroke.mods);
        if let Some(unshifted) = stroke.unshifted {
            vt_event.set_unshifted_codepoint(unshifted);
        }
        if stroke.shift_consumed {
            vt_event.set_consumed_mods(key::Mods::SHIFT);
        }
        let text = stroke.text.clone();
        vt_event.set_utf8(stroke.text);
        let mut bytes = Vec::new();
        let encoded = self.encoder.encode_to_vec(&vt_event, &mut bytes).is_ok();
        // A key libghostty has nothing for, but text, is the text.
        if (!encoded || bytes.is_empty())
            && let Some(text) = text
        {
            bytes = text.into_bytes();
        }
        if !bytes.is_empty() {
            let _ = self.session.write_input(&bytes);
        }
    }

    /// Returns the title the program in the shell set, if it set one.
    pub fn title_set_by_the_program(&self) -> Option<String> {
        self.emulator
            .title()
            .ok()
            .filter(|title| !title.trim().is_empty())
            .map(str::to_string)
    }

    /// Pastes text into the shell, marked as a paste when the program asked for that.
    pub fn paste(&mut self, text: &str) {
        self.scroll_to_the_bottom();
        let bracketed = self.emulator.mode(Mode::BRACKETED_PASTE).unwrap_or(false);
        let mut bytes = Vec::with_capacity(text.len() + 12);
        if bracketed {
            bytes.extend_from_slice(b"\x1b[200~");
        }
        bytes.extend_from_slice(text.as_bytes());
        if bracketed {
            bytes.extend_from_slice(b"\x1b[201~");
        }
        let _ = self.session.write_input(&bytes);
    }
}

fn color_of(rgb: RgbColor) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

fn modifiers_of(style: &libghostty_vt::style::Style) -> Modifier {
    let mut modifiers = Modifier::empty();
    if style.bold {
        modifiers |= Modifier::BOLD;
    }
    if style.italic {
        modifiers |= Modifier::ITALIC;
    }
    if style.faint {
        modifiers |= Modifier::DIM;
    }
    if style.inverse {
        modifiers |= Modifier::REVERSED;
    }
    if style.strikethrough {
        modifiers |= Modifier::CROSSED_OUT;
    }
    if style.underline != libghostty_vt::style::Underline::None {
        modifiers |= Modifier::UNDERLINED;
    }
    modifiers
}
