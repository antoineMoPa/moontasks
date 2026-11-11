//! This module draws the board's columns and their cards.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use super::{ACCENT, DIM, GOING, HANDLE, PICKED, REVIEW, age_of, cut_to, tag_color_of};
use crate::{
    app::{App, Dragged},
    pointer::{where_a_column_would_land, where_cards_would_land},
    run_menu::{RUNS_LISTED, label_on_the_card, runs_listed_on},
};

/// Sets the narrowest a column is drawn: past that, the board scrolls sideways instead.
const NARROWEST_COLUMN: u16 = 22;
/// Sets the width of a folded column's strip, its borders included.
const FOLDED_WIDTH: u16 = 3;

pub fn draw_the_columns(frame: &mut Frame, app: &mut App, area: Rect, focused: bool) {
    let count = app.board.columns.len();
    if count == 0 {
        frame.render_widget(Paragraph::new(" this board has no columns"), area);
        return;
    }
    // As many columns as fit are shown, keeping the picked one in view. A folded column takes
    // a narrow strip, so more fit beside it.
    let widths: Vec<u16> = (0..count)
        .map(|column| {
            if app.column_is_folded(column) {
                FOLDED_WIDTH
            } else {
                NARROWEST_COLUMN
            }
        })
        .collect();
    let last_shown_from = |first: usize| {
        let mut used = widths[first];
        let mut last = first;
        while last + 1 < count && used + widths[last + 1] <= area.width {
            last += 1;
            used += widths[last];
        }
        last
    };
    let mut first = 0;
    while first < app.column && last_shown_from(first) < app.column {
        first += 1;
    }
    let shown: Vec<usize> = (first..=last_shown_from(first)).collect();
    let areas = Layout::horizontal(shown.iter().map(|column| {
        if app.column_is_folded(*column) {
            Constraint::Length(FOLDED_WIDTH)
        } else {
            Constraint::Fill(1)
        }
    }))
    .split(area);

    let dragging = app
        .drag
        .as_ref()
        .filter(|drag| drag.moving)
        .map(|drag| (drag.what.clone(), drag.at));
    for (position, column_area) in shown.iter().zip(areas.iter()) {
        app.hits.column_slots.push((*column_area, *position));
    }
    // A column being carried is drawn where it would land, the others making room, so the
    // board shows what letting go will do.
    let carried = match &dragging {
        Some((Dragged::Column(carried), at)) => {
            where_a_column_would_land(app, *at).map(|landing| (*carried, landing))
        }
        _ => None,
    };
    let mut order: Vec<usize> = (0..count).collect();
    if let Some((carried, landing)) = carried {
        order.remove(carried);
        order.insert(landing.min(order.len()), carried);
    }

    for (position, column_area) in shown.into_iter().zip(areas.iter().copied()) {
        let column = order[position];
        draw_one_column(
            app,
            frame,
            column,
            column_area,
            carried.is_some_and(|(carried, _)| carried == column),
            focused,
        );
    }

    if let Some((Dragged::Cards(cards), at)) = dragging {
        if let Some(landing) = where_cards_would_land(app, &cards, at)
            && let Some((area, _)) = app
                .hits
                .columns
                .iter()
                .find(|(_, column)| *column == landing.column)
        {
            let line = Rect::new(area.x + 1, landing.row, area.width.saturating_sub(2), 1);
            frame.render_widget(
                Line::styled("━".repeat(line.width as usize), Style::new().fg(ACCENT)),
                line,
            );
        }
        let title = app
            .board
            .task(&cards[0])
            .map(|task| cut_to(&task.title, 30))
            .unwrap_or_default();
        let label = match cards.len() {
            1 => format!(" ⇢ {title} "),
            many => format!(" ⇢ {title} +{} ", many - 1),
        };
        let width = (label.chars().count() as u16).min(area.width);
        let x = at.x.min(area.right().saturating_sub(width));
        frame.render_widget(
            Line::styled(
                label,
                Style::new().fg(ratatui::style::Color::Black).bg(ACCENT),
            ),
            Rect::new(x, at.y.min(area.bottom().saturating_sub(1)), width, 1),
        );
    }
}

/// Holds what one card shows, read off the board before it is drawn.
struct CardShown {
    task_id: String,
    title: String,
    tags: Vec<String>,
    review: bool,
    /// Holds the runs listed, as (run id, mark, its color, label, age).
    runs: Vec<(String, &'static str, ratatui::style::Color, String, String)>,
    more_runs: usize,
    /// Says whether it shows its title alone, on one line, with its runs as marks after it.
    one_line: bool,
}

impl CardShown {
    /// Returns the rows it is drawn on.
    fn rows(&self) -> u16 {
        if self.one_line {
            return 1;
        }
        let marks = u16::from(!self.tags.is_empty() || self.review);
        1 + marks + self.runs.len() as u16 + u16::from(self.more_runs > 0)
    }

    /// Returns the rows it takes, the gap under it included - which one-line cards go without.
    fn height(&self) -> u16 {
        self.rows() + u16::from(!self.one_line)
    }

    /// Returns what a one-line card shows after its title: a review waiting, and a mark per
    /// run.
    fn marks_after_the_title(&self) -> Vec<Span<'static>> {
        let mut marks = Vec::new();
        if self.review {
            marks.push(Span::styled(" ◆", Style::new().fg(REVIEW)));
        }
        if !self.runs.is_empty() {
            marks.push(Span::raw(" "));
        }
        for (_, mark, color, _, _) in &self.runs {
            marks.push(Span::styled(*mark, Style::new().fg(*color)));
        }
        if self.more_runs > 0 {
            marks.push(Span::styled(
                format!("+{}", self.more_runs),
                Style::new().fg(DIM),
            ));
        }
        marks
    }
}

fn card_shown_of(app: &App, task: &moon_core::moontasks::TaskView) -> CardShown {
    let own = std::process::id();
    let listed = runs_listed_on(task);
    let runs = listed
        .iter()
        .take(RUNS_LISTED)
        .map(|run| {
            let (mark, color) = match (run.running, run.going_elsewhere_in) {
                (_, Some(moon)) if moon != own => ("◌", HANDLE),
                (true, _) if run.attention.is_some() => ("!", REVIEW),
                (true, _) => ("●", GOING),
                (false, _) => ("○", DIM),
            };
            (
                run.id.clone(),
                mark,
                color,
                label_on_the_card(run, &task.title),
                age_of(run.started_at_unix),
            )
        })
        .collect();
    CardShown {
        task_id: task.id.clone(),
        title: task.title.clone(),
        tags: task.tags.clone(),
        review: app.board.has_a_review_waiting(&task.id),
        runs,
        more_runs: listed.len().saturating_sub(RUNS_LISTED),
        one_line: app.one_line_cards,
    }
}

fn draw_one_column(
    app: &mut App,
    frame: &mut Frame,
    column: usize,
    area: Rect,
    landing: bool,
    focused: bool,
) {
    let picked_column = column == app.column && focused;
    if app.column_is_folded(column) {
        return draw_a_folded_column(app, frame, column, area, landing || picked_column);
    }
    let cards: Vec<CardShown> = app
        .cards_shown_in(column)
        .iter()
        .map(|task| card_shown_of(app, task))
        .collect();
    let all = app.board.cards_in(&app.board.columns[column].id).len();
    let label = &app.board.columns[column].label;
    let count = if cards.len() == all {
        format!("{all}")
    } else {
        format!("{}/{all}", cards.len())
    };
    let border = if landing || picked_column {
        ACCENT
    } else {
        DIM
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(Line::from(vec![
            Span::styled(" ▾", Style::new().fg(DIM)),
            Span::styled(
                format!(" {label} "),
                Style::new()
                    .fg(if picked_column {
                        ACCENT
                    } else {
                        ratatui::style::Color::Reset
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{count} "), Style::new().fg(DIM)),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.hits.columns.push((area, column));
    app.hits
        .headings
        .push((Rect::new(area.x, area.y, area.width, 1), column));
    app.hits
        .folds
        .push((Rect::new(area.x + 1, area.y, 3, 1), column));

    if cards.is_empty() {
        let says = if all > 0 {
            "  (filtered out)"
        } else {
            "  (empty)"
        };
        frame.render_widget(Line::styled(says, Style::new().fg(DIM)), inner);
        return;
    }

    // Scroll so the picked card is on screen, whole.
    let picked = app.card_in_column[column].min(cards.len() - 1);
    let scroll = &mut app.scroll_in_column[column];
    if picked < *scroll {
        *scroll = picked;
    }
    while *scroll < picked
        && cards[*scroll..=picked]
            .iter()
            .map(CardShown::height)
            .sum::<u16>()
            > inner.height
    {
        *scroll += 1;
    }
    let scroll = *scroll;

    let width = inner.width.saturating_sub(1) as usize;
    let mut y = inner.y;
    let mut drawn = 0;
    for (offset, card) in cards.iter().enumerate().skip(scroll) {
        let height = card.rows();
        if y + height > inner.bottom() {
            break;
        }
        let is_picked = offset == picked && picked_column;
        let marked = app.marked.contains(&card.task_id);
        let style = if is_picked {
            Style::new().bg(PICKED)
        } else {
            Style::new()
        };
        let mark = if marked {
            Span::styled("▌", Style::new().fg(ACCENT))
        } else {
            Span::raw(" ")
        };
        let after_the_title = if card.one_line {
            card.marks_after_the_title()
        } else {
            Vec::new()
        };
        let after_width: usize = after_the_title.iter().map(Span::width).sum();
        let title_width = width.saturating_sub(after_width);
        let mut lines = vec![
            Line::from(
                [
                    mark.clone(),
                    Span::styled(
                        if card.one_line {
                            format!("{:<title_width$}", cut_to(&card.title, title_width))
                        } else {
                            cut_to(&card.title, width)
                        },
                        if is_picked {
                            style.add_modifier(Modifier::BOLD)
                        } else {
                            style
                        },
                    ),
                ]
                .into_iter()
                .chain(after_the_title)
                .collect::<Vec<_>>(),
            )
            .style(style),
        ];
        if !card.one_line && (!card.tags.is_empty() || card.review) {
            let mut marks = vec![mark.clone()];
            for tag in card.tags.iter().take(3) {
                marks.push(Span::styled(
                    format!(" {tag} "),
                    Style::new()
                        .fg(ratatui::style::Color::Black)
                        .bg(tag_color_of(tag)),
                ));
                marks.push(Span::raw(" "));
            }
            if card.review {
                marks.push(Span::styled("◆ review", Style::new().fg(REVIEW)));
            }
            lines.push(Line::from(marks).style(style));
        }
        let card_area = Rect::new(inner.x, y, inner.width, height);
        for (index, (run_id, run_mark, color, label, age)) in card.runs.iter().enumerate() {
            let run_picked = is_picked && app.picked_run == Some(index);
            let style = if run_picked {
                Style::new()
                    .fg(ratatui::style::Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                style
            };
            let row = y + lines.len() as u16;
            let label_width = width.saturating_sub(age.len() + 5);
            lines.push(
                Line::from(vec![
                    mark.clone(),
                    Span::styled(
                        format!(" {run_mark} "),
                        if run_picked {
                            style
                        } else {
                            Style::new().fg(*color)
                        },
                    ),
                    Span::styled(
                        format!("{:<label_width$}", cut_to(label, label_width)),
                        if run_picked {
                            style
                        } else {
                            Style::new().fg(HANDLE)
                        },
                    ),
                    Span::styled(
                        format!(" {age}"),
                        if run_picked {
                            style
                        } else {
                            Style::new().fg(DIM)
                        },
                    ),
                ])
                .style(style),
            );
            let _ = index;
            app.hits.card_runs.push((
                Rect::new(inner.x, row, inner.width, 1),
                card.task_id.clone(),
                run_id.clone(),
            ));
        }
        if card.more_runs > 0 {
            lines.push(
                Line::from(vec![
                    mark,
                    Span::styled(
                        format!("   +{} more · o", card.more_runs),
                        Style::new().fg(DIM),
                    ),
                ])
                .style(style),
            );
        }
        frame.render_widget(Paragraph::new(lines), card_area);
        app.hits
            .cards
            .push((card_area, column, card.task_id.clone()));
        y += card.height();
        drawn += 1;
    }
    let hidden = cards.len() - scroll - drawn;
    if hidden > 0 {
        let more = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
        frame.render_widget(
            Line::styled(format!("  … {hidden} more"), Style::new().fg(DIM)),
            more,
        );
    }
}

/// Draws a folded column as a narrow strip: its name down it, and how many cards it holds. A
/// click on it unfolds it.
fn draw_a_folded_column(app: &mut App, frame: &mut Frame, column: usize, area: Rect, lit: bool) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if lit { ACCENT } else { DIM }));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.hits.columns.push((area, column));
    app.hits
        .headings
        .push((Rect::new(area.x, area.y, area.width, 1), column));
    app.hits.folds.push((inner, column));
    let all = app.board.cards_in(&app.board.columns[column].id).len();
    let label = &app.board.columns[column].label;
    let down: Vec<Line> = std::iter::once(Line::styled("▸", Style::new().fg(DIM)))
        .chain(label.chars().map(|letter| {
            Line::styled(
                letter.to_string(),
                Style::new()
                    .fg(if lit {
                        ACCENT
                    } else {
                        ratatui::style::Color::Reset
                    })
                    .add_modifier(Modifier::BOLD),
            )
        }))
        .chain(std::iter::once(Line::raw("")))
        .chain(
            all.to_string()
                .chars()
                .map(|digit| Line::styled(digit.to_string(), Style::new().fg(DIM))),
        )
        .collect();
    frame.render_widget(Paragraph::new(down), inner);
}
