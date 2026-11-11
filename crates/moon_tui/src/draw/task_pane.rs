//! This module draws, beside the columns when `enter` opens it, the picked task's notes, runs,
//! files and review.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use super::{ACCENT, DIM, GOING, REVIEW, age_of, cut_to, tag_color_of};
use crate::app::App;

fn block_titled(title: &str, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { ACCENT } else { DIM }))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(if focused { ACCENT } else { DIM }),
        ))
}

pub fn draw_the_task_pane(frame: &mut Frame, app: &App, area: Rect) {
    let block = block_titled("task", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(task) = app.picked_card() else {
        frame.render_widget(Line::styled(" no card picked", Style::new().fg(DIM)), inner);
        return;
    };
    let column = app
        .board
        .columns
        .iter()
        .find(|column| column.id == task.status)
        .map(|column| column.label.clone())
        .unwrap_or_default();
    let heading = |text: &str| Line::styled(format!("── {text} "), Style::new().fg(DIM));
    let mut lines = vec![
        Line::styled(
            task.title.clone(),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Line::from(
            std::iter::once(Span::styled(format!("{column}  "), Style::new().fg(DIM)))
                .chain(task.tags.iter().flat_map(|tag| {
                    [
                        Span::styled(
                            format!(" {tag} "),
                            Style::new()
                                .fg(ratatui::style::Color::Black)
                                .bg(tag_color_of(tag)),
                        ),
                        Span::raw(" "),
                    ]
                }))
                .collect::<Vec<_>>(),
        ),
        Line::styled(
            cut_to(&task.dir_path, inner.width as usize),
            Style::new().fg(DIM),
        ),
        Line::raw(""),
        heading("notes · e edits"),
    ];
    if task.notes.trim().is_empty() {
        lines.push(Line::styled("nothing written yet", Style::new().fg(DIM)));
    } else {
        lines.extend(
            task.notes
                .lines()
                .take(14)
                .map(|line| Line::raw(line.to_string())),
        );
    }
    lines.push(Line::raw(""));
    lines.push(heading("runs · s starts one"));
    let runs: Vec<_> = task
        .resources
        .iter()
        .filter(|run| run.terminal_id.is_some() || run.resumable)
        .collect();
    if runs.is_empty() {
        lines.push(Line::styled("none", Style::new().fg(DIM)));
    }
    for run in runs {
        let (mark, style) = if crate::run_menu::is_going(run) {
            ("●", Style::new().fg(GOING))
        } else {
            ("○", Style::new().fg(DIM))
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{mark} "), style),
            Span::raw(run.label.clone()),
            Span::styled(
                format!("  {}", age_of(run.started_at_unix)),
                Style::new().fg(DIM),
            ),
        ]));
    }
    if !task.attachments.is_empty() {
        lines.push(Line::raw(""));
        lines.push(heading("files"));
        lines.extend(task.attachments.iter().map(|file| Line::raw(file.clone())));
    }
    let reviews: Vec<_> = app
        .board
        .review_requests
        .iter()
        .filter(|request| request.task_id == task.id)
        .collect();
    if !reviews.is_empty() {
        lines.push(Line::raw(""));
        lines.push(heading("review"));
        for request in reviews {
            let (mark, style) = if request.pending() {
                ("◆", Style::new().fg(REVIEW))
            } else {
                ("✓", Style::new().fg(DIM))
            };
            let branch = request
                .branch
                .as_deref()
                .map(|branch| format!(" #{branch}"))
                .unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::raw(format!("{}{branch}", request.name)),
                Span::styled(
                    format!("  {} files", request.changed_files),
                    Style::new().fg(DIM),
                ),
            ]));
        }
    }
    if !task.remote_task_tracker_url.is_empty() {
        lines.push(Line::raw(""));
        lines.push(heading("tracker"));
        lines.push(Line::raw(task.remote_task_tracker_url.clone()));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
