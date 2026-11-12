//! This module draws what goes over the board: a line being typed, what to start, the
//! command palette, a page of text, and the question before quitting. Each records the
//! screen around it, which a click closes it from, its own inside, and its rows and buttons.

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use super::widgets::{Look, draw_a_button};
use crate::{
    app::{App, Overlay, PromptAsks},
    board_actions::label_of,
    hits::Hit,
    line_input::LineInput,
    theme::{ACCENT, DIM, PICKED},
};

pub fn draw_the_overlay(frame: &mut Frame, app: &mut App) {
    let Some(overlay) = app.overlay.take() else {
        return;
    };
    let screen = frame.area();
    app.hits.record(screen, Hit::Close);
    match &overlay {
        Overlay::Prompt { asks, line } => {
            let inner = open_a_box(frame, app, centered(screen, 70, 3), &title_of(asks));
            // The end of the line stays in view as it grows.
            let cursor = line.cursor_in_characters() as u16;
            let skip = cursor.saturating_sub(inner.width.saturating_sub(1));
            let shown: String = line.text.chars().skip(skip as usize).collect();
            frame.render_widget(Line::raw(shown), inner);
            frame.set_cursor_position(Position::new(inner.x + cursor - skip, inner.y));
        }
        Overlay::StartMenu {
            items,
            index,
            task_id,
        } => {
            let title = app
                .board
                .task(task_id)
                .map(|task| format!("start on {}", super::cut_to(&task.title, 40)))
                .unwrap_or_else(|| "start".to_string());
            let area = centered(screen, 50, items.len() as u16 + 2);
            let inner = open_a_box(frame, app, area, &title);
            for (at, item) in items.iter().enumerate() {
                let row = Rect::new(inner.x, inner.y + at as u16, inner.width, 1);
                draw_a_menu_row(frame, app, row, at, at == *index, label_of(*item), None);
            }
        }
        Overlay::Palette { line, index } => draw_the_palette(frame, app, line, *index),
        Overlay::Text { title, body } => {
            let rows = body.lines().count() as u16 + 2;
            let widest = body.lines().map(|line| line.chars().count()).max();
            let width = (widest.unwrap_or(0) as u16 + 4).min(100);
            let area = centered(screen, width, rows.min(screen.height.saturating_sub(2)));
            // A click anywhere closes it, its inside too: it has nothing to click.
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(body.lines().map(Line::raw).collect::<Vec<_>>())
                    .block(framed(title))
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        Overlay::ConfirmQuit => {
            let inner = open_a_box(frame, app, centered(screen, 52, 3), "quit?");
            frame.render_widget(
                Line::styled(" ends what this moon started", Style::new().fg(DIM)),
                inner,
            );
            let right = inner.right();
            let x = right.saturating_sub(16);
            let width = draw_a_button(
                frame,
                app,
                (x, inner.y),
                right,
                "[quit]",
                Look::Danger,
                Hit::Quit,
            );
            draw_a_button(
                frame,
                app,
                (x + width + 1, inner.y),
                right,
                "[stay]",
                Look::Quiet,
                Hit::Stay,
            );
        }
    }
    app.overlay = Some(overlay);
}

/// Clears a box, frames it with a title, records its inside, and returns where its contents go.
fn open_a_box(frame: &mut Frame, app: &mut App, area: Rect, title: &str) -> Rect {
    let block = framed(title);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    app.hits.record(area, Hit::Inside);
    inner
}

/// Draws a row of a menu, lit when the keys picked it, and records it for a click: its label,
/// then - dim, after it - a description and, at its end, the keys that do the same.
fn draw_a_menu_row(
    frame: &mut Frame,
    app: &mut App,
    row: Rect,
    index: usize,
    picked: bool,
    label: String,
    after: Option<(&str, &str)>,
) {
    let style = if picked {
        Style::new().bg(PICKED)
    } else {
        Style::new()
    };
    let title = format!(" {label}");
    let mut spans = vec![Span::styled(
        title.clone(),
        if picked {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        },
    )];
    if let Some((description, keys)) = after {
        let width = row.width as usize;
        let keys = format!(" {keys} ");
        let room = width.saturating_sub(title.chars().count() + keys.chars().count() + 2);
        let description = super::cut_to(description, room);
        let gap = width.saturating_sub(
            title.chars().count() + 2 + description.chars().count() + keys.chars().count(),
        );
        spans.push(Span::styled(format!("  {description}"), style.fg(DIM)));
        spans.push(Span::styled(" ".repeat(gap), style));
        spans.push(Span::styled(keys, style.fg(DIM)));
    }
    frame.render_widget(Line::from(spans).style(style), row);
    app.hits.record(row, Hit::MenuRow(index));
}

/// Sets how many commands the palette lists at once.
const PALETTE_ROWS: usize = 14;

/// Draws the palette: what is typed, and the commands it matches, the picked one lit - with
/// their descriptions and the keys that do the same.
fn draw_the_palette(frame: &mut Frame, app: &mut App, line: &LineInput, index: usize) {
    let screen = frame.area();
    let commands = crate::palette::list_the_matching_commands(app, &line.text);
    let rows = commands.len().clamp(1, PALETTE_ROWS) as u16;
    let inner = open_a_box(frame, app, centered(screen, 90, rows + 3), "commands");
    frame.render_widget(
        Line::from(vec![
            Span::styled("› ", Style::new().fg(ACCENT)),
            Span::raw(line.text.clone()),
        ]),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    frame.set_cursor_position(Position::new(
        (inner.x + 2 + line.cursor_in_characters() as u16).min(inner.right().saturating_sub(1)),
        inner.y,
    ));
    if commands.is_empty() {
        frame.render_widget(
            Line::styled(" no command says that", Style::new().fg(DIM)),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        return;
    }
    // The list scrolls so the picked command stays in view.
    let first = index.saturating_sub(PALETTE_ROWS - 1);
    for (row, (at, command)) in commands
        .iter()
        .enumerate()
        .skip(first)
        .take(PALETTE_ROWS)
        .enumerate()
    {
        let rect = Rect::new(inner.x, inner.y + 1 + row as u16, inner.width, 1);
        draw_a_menu_row(
            frame,
            app,
            rect,
            at,
            at == index,
            command.title.clone(),
            Some((&command.description, &command.keys)),
        );
    }
}

/// Says what a line being typed is for, as its box's title.
fn title_of(asks: &PromptAsks) -> String {
    match asks {
        PromptAsks::NewCard { .. } => "new task".to_string(),
        PromptAsks::Rename { .. } => "rename".to_string(),
        PromptAsks::Tags { .. } => "tags".to_string(),
        PromptAsks::Tell { title, .. } => format!("tell {title}"),
        PromptAsks::Filter => "filter".to_string(),
    }
}

fn framed(title: &str) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(Span::styled(format!(" {title} "), Style::new().fg(ACCENT)))
}

/// Returns a box `width` wide and `height` high, in the middle of `screen` - smaller when it is.
fn centered(screen: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(screen.width.saturating_sub(2));
    let height = height.min(screen.height);
    Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + screen.height.saturating_sub(height) / 3,
        width,
        height,
    )
}
