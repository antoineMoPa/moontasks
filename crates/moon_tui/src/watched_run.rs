//! This module watches, from here, an agent another moon holds - a window, another terminal:
//! what its screen shows, read over that moon's socket twice a second, and a line at the foot
//! that `enter` tells it. That moon has its keys; this one can only read it and tell it lines.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use moon_core::{instances, terminal::Shown};
use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    draw::{DIM, HANDLE},
    line_input::{LineEdit, LineInput},
};

/// Sets how long the screen is left before it is read again.
const READ_EVERY: Duration = Duration::from_millis(500);

pub struct WatchedRun {
    pub moon: u32,
    pub terminal_id: String,
    pub title: String,
    /// Holds the screen as last read, or why it could not be.
    screen: Arc<Mutex<String>>,
    /// Is set when the pane closes, which ends the thread reading it.
    stopped: Arc<AtomicBool>,
    line: LineInput,
}

/// Says what a key typed into a watched run's pane asks for.
pub enum WatchedKey {
    Nothing,
    /// Carries a line to tell the agent.
    Tell(String),
}

impl WatchedRun {
    /// Starts reading the screen of a run another moon holds.
    pub fn watch(moon: u32, terminal_id: &str, title: &str) -> Self {
        let screen = Arc::new(Mutex::new(String::from("reading its screen…")));
        let stopped = Arc::new(AtomicBool::new(false));
        let (shown_to, stop, id) = (
            Arc::clone(&screen),
            Arc::clone(&stopped),
            terminal_id.to_string(),
        );
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let read = instances::shown(moon, &id, Shown::Screen).unwrap_or_else(|error| {
                    format!("could not read it from moon {moon}: {error:#}")
                });
                if let Ok(mut screen) = shown_to.lock() {
                    *screen = read;
                }
                std::thread::sleep(READ_EVERY);
            }
        });
        Self {
            moon,
            terminal_id: terminal_id.to_string(),
            title: title.to_string(),
            screen,
            stopped,
            line: LineInput::default(),
        }
    }

    pub fn type_key(&mut self, key: &KeyEvent) -> WatchedKey {
        match self.line.press(key) {
            LineEdit::Submitted if !self.line.text.trim().is_empty() => {
                let line = std::mem::take(&mut self.line.text);
                self.line = LineInput::default();
                WatchedKey::Tell(line)
            }
            LineEdit::Cancelled => {
                self.line = LineInput::default();
                WatchedKey::Nothing
            }
            _ => WatchedKey::Nothing,
        }
    }

    pub fn paste(&mut self, text: &str) {
        self.line.type_text(text);
    }

    /// Draws the screen, its foot kept for the line being typed; and says where the cursor is.
    pub fn draw(&self, frame: &mut Frame, area: Rect) -> Option<Position> {
        if area.height < 2 {
            return None;
        }
        let screen_area = Rect::new(area.x, area.y, area.width, area.height - 1);
        let text = self
            .screen
            .lock()
            .map(|screen| screen.clone())
            .unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        // Only the bottom of it shows, which is where an agent is writing.
        let start = lines.len().saturating_sub(screen_area.height as usize);
        frame.render_widget(
            Paragraph::new(
                lines[start..]
                    .iter()
                    .map(|line| Line::raw(line.to_string()))
                    .collect::<Vec<_>>(),
            ),
            screen_area,
        );
        let foot = Rect::new(area.x, area.bottom() - 1, area.width, 1);
        let prompt = format!(" ◌ moon {} owns this agent · tell › ", self.moon);
        let prompt_width = prompt.chars().count() as u16;
        let cursor = self.line.cursor_in_characters() as u16;
        let room = foot.width.saturating_sub(prompt_width + 1);
        let skip = cursor.saturating_sub(room);
        let shown: String = self.line.text.chars().skip(skip as usize).collect();
        frame.render_widget(
            Line::from(vec![
                Span::styled(prompt, Style::new().fg(HANDLE)),
                Span::raw(shown),
            ])
            .style(Style::new().bg(ratatui::style::Color::Indexed(235))),
            foot,
        );
        if self.line.text.is_empty() && foot.width > prompt_width + 30 {
            frame.render_widget(
                Line::styled("enter sends it a line", Style::new().fg(DIM)),
                Rect::new(foot.x + prompt_width, foot.y, 30, 1),
            );
        }
        Some(Position::new(foot.x + prompt_width + cursor - skip, foot.y))
    }
}

impl Drop for WatchedRun {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}
