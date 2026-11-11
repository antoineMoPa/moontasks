//! This module draws what goes over the board: a line being typed, the start menu, a card's
//! runs, a page of text, and the question before quitting.

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use super::{ACCENT, DIM, GOING, PICKED};
use crate::{
    app::{App, Overlay, PromptAsks},
    board_actions::label_of,
    run_menu::{label_on_the_card, runs_listed_on},
};

pub fn draw_the_overlay(frame: &mut Frame, app: &App) {
    let Some(overlay) = &app.overlay else {
        return;
    };
    let screen = frame.area();
    match overlay {
        Overlay::Prompt { asks, line } => {
            let area = centered(screen, 70, 3);
            let block = framed(&what_the_line_is_for(asks));
            let inner = block.inner(area);
            frame.render_widget(Clear, area);
            frame.render_widget(block, area);
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
            let block = framed(&title);
            let inner = block.inner(area);
            frame.render_widget(Clear, area);
            frame.render_widget(block, area);
            for (at, item) in items.iter().enumerate() {
                let style = if at == *index {
                    Style::new().bg(PICKED).add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                };
                let row = Rect::new(inner.x, inner.y + at as u16, inner.width, 1);
                frame.render_widget(Line::styled(format!(" {}", label_of(*item)), style), row);
            }
        }
        Overlay::RunMenu { task_id, index } => {
            let Some(task) = app.board.task(task_id) else {
                return;
            };
            let runs = runs_listed_on(task);
            let area = centered(screen, 60, runs.len() as u16 + 3);
            let block = framed(&format!("runs of {}", super::cut_to(&task.title, 40)));
            let inner = block.inner(area);
            frame.render_widget(Clear, area);
            frame.render_widget(block, area);
            for (at, run) in runs.iter().enumerate() {
                let style = if at == *index {
                    Style::new().bg(PICKED).add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                };
                let (mark, color) = if crate::run_menu::is_going(run) {
                    ("●", GOING)
                } else {
                    ("○", DIM)
                };
                let row = Rect::new(inner.x, inner.y + at as u16, inner.width, 1);
                frame.render_widget(
                    Line::from(vec![
                        Span::styled(format!(" {mark} "), style.fg(color)),
                        Span::styled(label_on_the_card(run, &task.title), style),
                        Span::styled(
                            format!("  {}", super::age_of(run.started_at_unix)),
                            style.fg(DIM),
                        ),
                    ])
                    .style(style),
                    row,
                );
            }
            let keys = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
            frame.render_widget(
                Line::styled(
                    " enter opens · x stops · R resumes · T tells",
                    Style::new().fg(DIM),
                ),
                keys,
            );
        }
        Overlay::Text { title, body } => {
            let rows = body.lines().count() as u16 + 3;
            let area = centered(screen, 100, rows.min(screen.height.saturating_sub(2)));
            let block = framed(title);
            frame.render_widget(Clear, area);
            let mut lines: Vec<Line> = body
                .lines()
                .map(|line| Line::raw(line.to_string()))
                .collect();
            lines.push(Line::styled("any key closes this", Style::new().fg(DIM)));
            frame.render_widget(
                Paragraph::new(lines)
                    .block(block)
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        Overlay::ConfirmQuit => {
            let area = centered(screen, 60, 4);
            let block = framed("quit?");
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::raw("What this moon started ends with it. Agents keep their runs,"),
                    Line::from(vec![
                        Span::raw("and resume from their cards.  "),
                        Span::styled("y", Style::new().fg(ACCENT)),
                        Span::raw(" quits, any other key stays"),
                    ]),
                ])
                .block(block),
                area,
            );
        }
    }
}

fn what_the_line_is_for(asks: &PromptAsks) -> String {
    match asks {
        PromptAsks::NewCard { .. } => "new card - its title".to_string(),
        PromptAsks::Rename { .. } => "rename the card".to_string(),
        PromptAsks::Tags { .. } => "tags, between commas".to_string(),
        PromptAsks::Tell { title, .. } => format!("tell {title}"),
        PromptAsks::Filter => "filter the board".to_string(),
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
