//! This module draws the screen: the frames with the board or a shell in each, a message at
//! the foot while there is one, and whatever is drawn over them. What a click acts on is
//! recorded as it is drawn - see [`crate::hits`].

mod columns;
mod overlays;
#[cfg(test)]
mod screen_tests;
mod task_pane;
mod widgets;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
};

use crate::{
    app::App,
    hits::{Hit, HitMap},
    theme::{ACCENT, ACCENT_LIT, DANGER, DIM, HOVER, HOVER_ON_PICKED, PICKED},
};

pub fn draw_the_screen(frame: &mut Frame, app: &mut App) {
    app.hits = HitMap::default();
    // The foot line is only there while it has something to say.
    let foot_line = foot_line_of(app);
    let [body, foot] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(u16::from(foot_line.is_some())),
    ])
    .areas(frame.area());
    // The frames draw the panes, and the app draws what is in them: both are lent out.
    let layout = std::mem::take(&mut app.layout);
    let mut frames = std::mem::take(&mut app.frames);
    frames.draw(frame, body, &layout, app);
    app.layout = layout;
    app.frames = frames;
    if let Some(line) = foot_line {
        frame.render_widget(line, foot);
    }
    overlays::draw_the_overlay(frame, app);
    if !app.drag.as_ref().is_some_and(|drag| drag.moving) {
        light_what_the_pointer_is_over(frame, app);
    }
}

/// Draws the board's pane: the filter while there is one, its columns, and the picked task's
/// details beside them when they are open.
pub fn draw_the_board(frame: &mut Frame, app: &mut App, area: Rect, focused: bool) {
    let [filter, area] = Layout::vertical([
        Constraint::Length(u16::from(!app.filter.is_empty())),
        Constraint::Min(1),
    ])
    .areas(area);
    if !app.filter.is_empty() {
        draw_the_filter(frame, app, filter);
    }
    if !app.task_pane_open {
        columns::draw_the_columns(frame, app, area, focused);
        return;
    }
    let pane_width = 52.min(area.width / 2);
    let [board, side] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(pane_width)]).areas(area);
    columns::draw_the_columns(frame, app, board, focused);
    task_pane::draw_the_task_pane(frame, app, side);
}

/// Draws the filter over the board: `/ words [x]` - a click on the words edits them, `[x]`
/// clears them.
fn draw_the_filter(frame: &mut Frame, app: &mut App, area: Rect) {
    let words = format!(" / {} ", app.filter);
    let width = (words.chars().count() as u16).min(area.width);
    let text = Rect::new(area.x, area.y, width, 1);
    frame.render_widget(Line::styled(words, Style::new().fg(ACCENT)), text);
    app.hits.record(text, Hit::EditFilter);
    widgets::draw_a_button(
        frame,
        app,
        (text.right(), area.y),
        area.right(),
        widgets::DELETE_BUTTON,
        widgets::Look::Quiet,
        Hit::ClearFilter,
    );
}

/// Says what the foot line shows: a message flashed - red when something failed, dim when it
/// only says what happened.
fn foot_line_of(app: &App) -> Option<Line<'static>> {
    let (message, failed) = app.flashed()?;
    Some(Line::styled(
        format!(" {message}"),
        Style::new().fg(if failed { DANGER } else { DIM }),
    ))
}

/// Lights what a click under the pointer would do: a button turns the accent, anything else a
/// click acts on takes the hover shade. Outside an open menu, nothing is lit.
fn light_what_the_pointer_is_over(frame: &mut Frame, app: &App) {
    let Some(at) = app.pointer_at else {
        return;
    };
    let Some((rect, hit)) = app.hits.find_the_spot_at(at) else {
        return;
    };
    let buffer = frame.buffer_mut();
    let rect = rect.intersection(buffer.area);
    if matches!(hit, Hit::Close | Hit::Inside) {
        return;
    }
    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            let cell = &mut buffer[(x, y)];
            if hit.is_a_button() {
                if cell.bg == ACCENT {
                    cell.set_bg(ACCENT_LIT);
                } else {
                    cell.set_fg(ACCENT).set_bg(HOVER);
                }
                cell.modifier.insert(Modifier::BOLD);
            } else {
                match cell.bg {
                    PICKED => {
                        cell.set_bg(HOVER_ON_PICKED);
                    }
                    ACCENT => {}
                    _ => {
                        cell.set_bg(HOVER);
                    }
                }
            }
        }
    }
}

/// Cuts `text` to `width` characters, with an ellipsis when it was cut.
pub fn cut_to(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Says how long ago something was, for an agent's or a shell's row.
pub fn age_of(started_at_unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let seconds = now.saturating_sub(started_at_unix);
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}
