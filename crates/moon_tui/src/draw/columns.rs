//! This module draws the board's columns and their tasks.

use moon_core::moontasks::{ColumnEnd, TaskResourceView};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use super::{
    cut_to,
    widgets::{Look, draw_a_button, draw_a_run_row},
};
use crate::{
    app::{App, Dragged},
    hits::Hit,
    pointer::{where_a_column_would_land, where_cards_would_land},
    run_menu::{RUNS_LISTED, runs_listed_on},
    theme::{ACCENT, DIM, ON_ACCENT, PICKED, REVIEW, tag_color_of},
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
            Line::styled(label, Style::new().fg(ON_ACCENT).bg(ACCENT)),
            Rect::new(x, at.y.min(area.bottom().saturating_sub(1)), width, 1),
        );
    }
}

/// Holds what one task shows, read off the board before it is drawn.
struct CardShown {
    task_id: String,
    title: String,
    tags: Vec<String>,
    review: bool,
    /// Holds the agents and shells listed on it.
    runs: Vec<TaskResourceView>,
    /// Counts the ones past those, which its details list.
    more_runs: usize,
}

impl CardShown {
    /// Returns the rows it is drawn on.
    fn rows(&self) -> u16 {
        let marks = u16::from(!self.tags.is_empty() || self.review);
        1 + marks + self.runs.len() as u16 + u16::from(self.more_runs > 0)
    }

    /// Returns the rows it takes, the gap under it included.
    fn height(&self) -> u16 {
        self.rows() + 1
    }
}

fn card_shown_of(app: &App, task: &moon_core::moontasks::TaskView) -> CardShown {
    let listed = runs_listed_on(task);
    CardShown {
        task_id: task.id.clone(),
        title: task.title.clone(),
        tags: task.tags.clone(),
        review: app.board.has_a_review_waiting(&task.id),
        runs: listed
            .iter()
            .take(RUNS_LISTED)
            .map(|run| (*run).clone())
            .collect(),
        more_runs: listed.len().saturating_sub(RUNS_LISTED),
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
    let label = app.board.columns[column].label.clone();
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
        .border_style(Style::new().fg(border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.hits.columns.push((area, column));
    // The heading is drawn over the top border: `▾`, the name, the count.
    let heading = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1);
    app.hits.record(heading, Hit::Heading(column));
    frame.render_widget(
        Line::from(vec![
            // Room for the `▾` drawn over it next.
            Span::raw("  "),
            Span::styled(
                format!(" {label} "),
                Style::new()
                    .fg(if picked_column { ACCENT } else { Color::Reset })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{count} "), Style::new().fg(DIM)),
        ]),
        heading,
    );
    draw_a_button(
        frame,
        app,
        (area.x + 1, area.y),
        area.right(),
        " ▾",
        Look::Quiet,
        Hit::Fold(column),
    );

    if cards.is_empty() {
        let mut y = inner.y;
        if all > 0 {
            frame.render_widget(
                Line::styled("  (filtered out)", Style::new().fg(DIM)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 2;
        }
        draw_the_new_task_button(app, frame, column, inner, y, ColumnEnd::Top);
        return;
    }

    // Scroll so the picked task is on screen, whole.
    let picked = app.card_in_column[column].min(cards.len() - 1);
    let scroll = &mut app.scroll_in_column[column];
    if picked < *scroll {
        *scroll = picked;
    }
    // The `+` above the first task and the gap under it take two rows.
    while *scroll < picked
        && cards[*scroll..=picked]
            .iter()
            .map(CardShown::height)
            .sum::<u16>()
            > inner.height.saturating_sub(2)
    {
        *scroll += 1;
    }
    let scroll = *scroll;

    // A row keeps a column clear of the right border.
    let width = inner.width.saturating_sub(1);
    let mut y = inner.y;
    if scroll == 0 {
        draw_the_new_task_button(app, frame, column, inner, y, ColumnEnd::Top);
        y += 2;
    }
    let mut drawn = 0;
    for (offset, card) in cards.iter().enumerate().skip(scroll) {
        let height = card.rows();
        if y + height > inner.bottom() {
            break;
        }
        draw_one_task(
            app,
            frame,
            card,
            column,
            Rect::new(inner.x, y, width, height),
            (
                offset == picked && picked_column,
                offset == picked && column == app.column,
            ),
        );
        y += card.height();
        drawn += 1;
    }
    let hidden = cards.len() - scroll - drawn;
    if hidden == 0 && y < inner.bottom() {
        draw_the_new_task_button(app, frame, column, inner, y, ColumnEnd::Bottom);
    }
    if hidden > 0 {
        let more = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
        frame.render_widget(
            Line::styled(format!("  … {hidden} more"), Style::new().fg(DIM)),
            more,
        );
    }
}

/// Draws one task: its title, its tags and review, its agents and shells - and `[start]` while
/// it is picked or under the pointer. `(lit, picked)` says whether it is lit as the keys'
/// choice, and whether it is the picked one, lit or not.
fn draw_one_task(
    app: &mut App,
    frame: &mut Frame,
    card: &CardShown,
    column: usize,
    area: Rect,
    (lit, picked): (bool, bool),
) {
    let style = if lit {
        Style::new().bg(PICKED)
    } else {
        Style::new()
    };
    let mark = if app.marked.contains(&card.task_id) {
        Span::styled("▌", Style::new().fg(ACCENT))
    } else {
        Span::raw(" ")
    };
    let width = area.width as usize;
    let mut lines = vec![
        Line::from(vec![
            mark.clone(),
            Span::styled(
                cut_to(&card.title, width.saturating_sub(1)),
                if lit {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                },
            ),
        ])
        .style(style),
    ];
    if !card.tags.is_empty() || card.review {
        let mut marks = vec![mark.clone()];
        for tag in card.tags.iter().take(3) {
            marks.push(Span::styled(
                format!(" {tag} "),
                Style::new().fg(ON_ACCENT).bg(tag_color_of(tag)),
            ));
            marks.push(Span::raw(" "));
        }
        if card.review {
            marks.push(Span::styled("◆ review", Style::new().fg(REVIEW)));
        }
        lines.push(Line::from(marks).style(style));
    }
    let runs_from = area.y + lines.len() as u16;
    frame.render_widget(Paragraph::new(lines).style(style), area);
    // The task is recorded first: what is drawn on it is reached before it.
    app.hits.record(
        area,
        Hit::Task {
            column,
            task_id: card.task_id.clone(),
        },
    );
    for (index, run) in card.runs.iter().enumerate() {
        let row = Rect::new(area.x, runs_from + index as u16, area.width, 1);
        let run_picked = lit && app.picked_run == Some(index);
        draw_a_run_row(
            frame,
            app,
            row,
            (&card.task_id, &card.title),
            run,
            run_picked,
            false,
        );
    }
    if card.more_runs > 0 {
        let row = Rect::new(area.x, runs_from + card.runs.len() as u16, area.width, 1);
        frame.render_widget(
            Line::styled(
                format!("    +{} more", card.more_runs),
                Style::new().fg(DIM),
            ),
            row,
        );
        app.hits.record(row, Hit::Details(card.task_id.clone()));
    }
    let hovered = app.pointer_at.is_some_and(|at| area.contains(at));
    if picked || hovered {
        let width = START_BUTTON.len() as u16;
        if area.width >= width + 6 {
            draw_a_button(
                frame,
                app,
                (area.right() - width, area.y),
                area.right(),
                START_BUTTON,
                Look::Main,
                Hit::StartOn(card.task_id.clone()),
            );
        }
    }
}

/// Sets what a task's button says, which a click offers what to start on it.
const START_BUTTON: &str = "[start]";

/// Draws `[  +  ]` across the column on row `y`: above its first task, a click asks a new
/// task's name for the top; below its last, for the bottom.
fn draw_the_new_task_button(
    app: &mut App,
    frame: &mut Frame,
    column: usize,
    inner: Rect,
    y: u16,
    joins: ColumnEnd,
) {
    const BUTTON: &str = "[  +  ]";
    let width = BUTTON.len() as u16;
    if inner.width < width || y >= inner.bottom() {
        return;
    }
    draw_a_button(
        frame,
        app,
        (inner.x + (inner.width - width) / 2, y),
        inner.right(),
        BUTTON,
        Look::Quiet,
        Hit::NewTask { column, joins },
    );
}

/// Draws a folded column as a narrow strip: its name down it, and how many tasks it holds. A
/// click on it unfolds it.
fn draw_a_folded_column(app: &mut App, frame: &mut Frame, column: usize, area: Rect, lit: bool) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if lit { ACCENT } else { DIM }));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.hits.columns.push((area, column));
    app.hits.record(
        Rect::new(area.x, area.y, area.width, 1),
        Hit::Heading(column),
    );
    app.hits.record(inner, Hit::Fold(column));
    let all = app.board.cards_in(&app.board.columns[column].id).len();
    let label = &app.board.columns[column].label;
    let down: Vec<Line> = std::iter::once(Line::styled("▸", Style::new().fg(DIM)))
        .chain(label.chars().map(|letter| {
            Line::styled(
                letter.to_string(),
                Style::new()
                    .fg(if lit { ACCENT } else { Color::Reset })
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
