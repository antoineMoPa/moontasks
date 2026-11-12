//! This module draws, beside the columns, the picked task's details: its notes, its agents and
//! shells, its files, its review and its tracker - each section's button beside its heading.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
};

use super::{
    cut_to,
    widgets::{Look, draw_a_button, draw_a_run_row},
};
use crate::{
    app::App,
    hits::Hit,
    run_menu::runs_listed_on,
    theme::{ACCENT, DIM, ON_ACCENT, REVIEW, tag_color_of},
};

/// Sets the most lines of notes the details show.
const NOTES_SHOWN: usize = 14;

/// Lays the details out a row at a time, from the top of `area` down, stopping at its bottom.
struct Rows {
    area: Rect,
    y: u16,
}

impl Rows {
    /// Returns the next row, while there is one.
    fn next(&mut self) -> Option<Rect> {
        (self.y < self.area.bottom()).then(|| {
            self.y += 1;
            Rect::new(self.area.x, self.y - 1, self.area.width, 1)
        })
    }

    fn line(&mut self, frame: &mut Frame, line: Line) {
        if let Some(row) = self.next() {
            frame.render_widget(line, row);
        }
    }

    /// Draws a section's heading, dim, with a button after it when it has one.
    fn heading(
        &mut self,
        frame: &mut Frame,
        app: &mut App,
        text: &str,
        button: Option<(&str, Hit)>,
    ) {
        self.next();
        let Some(row) = self.next() else {
            return;
        };
        let heading = format!("── {text} ");
        let width = heading.chars().count() as u16;
        frame.render_widget(Line::styled(heading, Style::new().fg(DIM)), row);
        if let Some((label, hit)) = button {
            draw_a_button(
                frame,
                app,
                (row.x + width, row.y),
                row.right(),
                label,
                Look::Quiet,
                hit,
            );
        }
    }
}

pub fn draw_the_task_pane(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(task) = app.picked_card().cloned() else {
        frame.render_widget(Line::styled(" no task picked", Style::new().fg(DIM)), inner);
        return;
    };
    let width = inner.width as usize;
    let mut rows = Rows {
        area: inner,
        y: inner.y,
    };
    rows.line(
        frame,
        Line::styled(
            cut_to(&task.title, width),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
    );
    let column = app
        .board
        .columns
        .iter()
        .find(|column| column.id == task.status)
        .map(|column| column.label.clone())
        .unwrap_or_default();
    rows.line(
        frame,
        Line::from(
            std::iter::once(Span::styled(format!("{column}  "), Style::new().fg(DIM)))
                .chain(task.tags.iter().flat_map(|tag| {
                    [
                        Span::styled(
                            format!(" {tag} "),
                            Style::new().fg(ON_ACCENT).bg(tag_color_of(tag)),
                        ),
                        Span::raw(" "),
                    ]
                }))
                .collect::<Vec<_>>(),
        ),
    );

    rows.heading(
        frame,
        app,
        "notes",
        Some(("[edit]", Hit::EditNotes(task.id.clone()))),
    );
    if task.notes.trim().is_empty() {
        rows.line(
            frame,
            Line::styled("nothing written yet", Style::new().fg(DIM)),
        );
    }
    for line in task.notes.lines().take(NOTES_SHOWN) {
        rows.line(frame, Line::raw(cut_to(line, width)));
    }

    rows.heading(
        frame,
        app,
        "agents and shells",
        Some(("[start]", Hit::StartOn(task.id.clone()))),
    );
    let runs: Vec<_> = runs_listed_on(&task).into_iter().cloned().collect();
    if runs.is_empty() {
        rows.line(frame, Line::styled("none yet", Style::new().fg(DIM)));
    }
    for run in &runs {
        if let Some(row) = rows.next() {
            // A row keeps a column clear of the right border, as on the task.
            let row = Rect::new(row.x, row.y, row.width.saturating_sub(1), 1);
            draw_a_run_row(frame, app, row, (&task.id, &task.title), run, false, true);
        }
    }

    if !task.attachments.is_empty() {
        rows.heading(frame, app, "files", None);
        for file in &task.attachments {
            rows.line(frame, Line::raw(cut_to(file, width)));
        }
    }
    let reviews: Vec<_> = app
        .board
        .review_requests
        .iter()
        .filter(|request| request.task_id == task.id)
        .cloned()
        .collect();
    if !reviews.is_empty() {
        rows.heading(frame, app, "review", None);
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
            rows.line(
                frame,
                Line::from(vec![
                    Span::styled(format!("{mark} "), style),
                    Span::raw(format!("{}{branch}", request.name)),
                    Span::styled(
                        format!("  {} files", request.changed_files),
                        Style::new().fg(DIM),
                    ),
                ]),
            );
        }
    }
    if !task.remote_task_tracker_url.is_empty() {
        rows.heading(frame, app, "tracker", None);
        rows.line(
            frame,
            Line::raw(cut_to(&task.remote_task_tracker_url, width)),
        );
    }
}
