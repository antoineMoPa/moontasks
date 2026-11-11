//! This module takes the mouse over a shell's tab, the way the window's terminals do: the wheel
//! scrolls back through what the shell printed, unless the program asked for the mouse - then
//! the wheel and the buttons go to it - or it is a full-screen program, which gets the wheel as
//! arrow keys. Shift and the wheel always scrolls back.

use libghostty_vt::{
    mouse,
    screen::Screen,
    terminal::{Mode, ScrollViewport},
};
use ratatui::{
    crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
};

use super::LiveTerminal;

/// Sets how many lines a notch of the wheel scrolls back, or how many arrow keys it sends.
const LINES_PER_NOTCH: usize = 3;
/// Sets the size, in the encoder's pixels, given to a cell - the terminal has no pixels, only
/// cells, so any size does that is the same both ways.
const CELL_WIDTH: u32 = 10;
const CELL_HEIGHT: u32 = 20;

impl LiveTerminal {
    /// Takes a mouse event over the shell, drawn at `area`.
    pub fn handle_the_mouse(&mut self, event: &MouseEvent, area: Rect) {
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let to_program = !shift && self.emulator.is_mouse_tracking().unwrap_or(false);
        let (action, button) = match event.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = event.kind == MouseEventKind::ScrollUp;
                if to_program {
                    let button = if up {
                        mouse::Button::Four
                    } else {
                        mouse::Button::Five
                    };
                    return self.send_the_mouse(event, area, mouse::Action::Press, Some(button));
                }
                if !shift && self.wheel_is_arrow_keys() {
                    return self.send_the_wheel_as_arrows(up);
                }
                let lines = LINES_PER_NOTCH as isize;
                self.emulator.scroll_viewport(ScrollViewport::Delta(if up {
                    -lines
                } else {
                    lines
                }));
                return;
            }
            _ if !to_program => return,
            MouseEventKind::Down(button) => (mouse::Action::Press, Some(button_of(button))),
            MouseEventKind::Up(button) => (mouse::Action::Release, Some(button_of(button))),
            MouseEventKind::Drag(button) => (mouse::Action::Motion, Some(button_of(button))),
            MouseEventKind::Moved => (mouse::Action::Motion, None),
            _ => return,
        };
        self.send_the_mouse(event, area, action, button);
    }

    /// Says whether the view is scrolled back from the bottom, and by how many rows.
    pub fn rows_scrolled_back(&self) -> u64 {
        self.emulator
            .scrollbar()
            .map_or(0, |bar| bar.total.saturating_sub(bar.offset + bar.len))
    }

    /// Brings the view back to the bottom, where the shell is writing.
    pub fn scroll_to_the_bottom(&mut self) {
        if self.rows_scrolled_back() > 0 {
            self.emulator.scroll_viewport(ScrollViewport::Bottom);
        }
    }

    /// Says whether the wheel goes to the program as arrow keys: the alternate screen has no
    /// scrollback, and a full-screen program that did not ask for the mouse still expects the
    /// wheel to move something.
    fn wheel_is_arrow_keys(&self) -> bool {
        self.emulator
            .active_screen()
            .is_ok_and(|screen| screen == Screen::Alternate)
            && self.emulator.mode(Mode::ALT_SCROLL).unwrap_or(false)
    }

    fn send_the_wheel_as_arrows(&mut self, up: bool) {
        let application = self.emulator.mode(Mode::DECCKM).unwrap_or(false);
        let arrow: &[u8] = match (up, application) {
            (true, true) => b"\x1bOA",
            (true, false) => b"\x1b[A",
            (false, true) => b"\x1bOB",
            (false, false) => b"\x1b[B",
        };
        let _ = self.session.write_input(&arrow.repeat(LINES_PER_NOTCH));
    }

    /// Sends a mouse event to a program that asked for the mouse, in the encoding it asked for.
    fn send_the_mouse(
        &mut self,
        event: &MouseEvent,
        area: Rect,
        action: mouse::Action,
        button: Option<mouse::Button>,
    ) {
        if matches!(action, mouse::Action::Press | mouse::Action::Release)
            && !matches!(button, Some(mouse::Button::Four | mouse::Button::Five))
        {
            self.mouse_button_down = action == mouse::Action::Press;
        }
        self.mouse_encoder
            .set_options_from_terminal(&self.emulator)
            .set_size(mouse::EncoderSize {
                screen_width: u32::from(area.width) * CELL_WIDTH,
                screen_height: u32::from(area.height) * CELL_HEIGHT,
                cell_width: CELL_WIDTH,
                cell_height: CELL_HEIGHT,
                padding_top: 0,
                padding_bottom: 0,
                padding_right: 0,
                padding_left: 0,
            })
            .set_any_button_pressed(self.mouse_button_down)
            .set_track_last_cell(true);
        let mut mods = libghostty_vt::key::Mods::empty();
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            mods |= libghostty_vt::key::Mods::SHIFT;
        }
        if event.modifiers.contains(KeyModifiers::ALT) {
            mods |= libghostty_vt::key::Mods::ALT;
        }
        if event.modifiers.contains(KeyModifiers::CONTROL) {
            mods |= libghostty_vt::key::Mods::CTRL;
        }
        // The middle of the cell, so rounding puts it in the cell meant.
        let column = event.column.saturating_sub(area.x) as f32;
        let row = event.row.saturating_sub(area.y) as f32;
        self.mouse_event
            .set_action(action)
            .set_button(button)
            .set_mods(mods)
            .set_position(mouse::Position {
                x: (column + 0.5) * CELL_WIDTH as f32,
                y: (row + 0.5) * CELL_HEIGHT as f32,
            });
        let mut bytes = Vec::new();
        if self
            .mouse_encoder
            .encode_to_vec(&self.mouse_event, &mut bytes)
            .is_ok()
            && !bytes.is_empty()
        {
            let _ = self.session.write_input(&bytes);
        }
    }
}

fn button_of(button: MouseButton) -> mouse::Button {
    match button {
        MouseButton::Left => mouse::Button::Left,
        MouseButton::Right => mouse::Button::Right,
        MouseButton::Middle => mouse::Button::Middle,
    }
}
