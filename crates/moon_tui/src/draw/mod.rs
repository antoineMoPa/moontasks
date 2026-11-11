//! This module draws the screen: the tab bar, the board or a shell under it, the keys at the
//! foot, and whatever is drawn over them. Where each thing lands is written down for the mouse.

mod columns;
mod overlays;
mod task_pane;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::{App, HitMap};

pub const ACCENT: Color = Color::Yellow;
pub const DIM: Color = Color::DarkGray;
pub const PICKED: Color = Color::Indexed(237);
pub const GOING: Color = Color::Green;
pub const HANDLE: Color = Color::Cyan;
pub const REVIEW: Color = Color::Yellow;

/// Lists the colors a tag can be drawn in: which one is settled by the tag's own letters, so a
/// tag is the same color on every card, as it is in the window.
const TAG_COLORS: [Color; 6] = [
    Color::Magenta,
    Color::Cyan,
    Color::Blue,
    Color::Green,
    Color::Red,
    Color::LightYellow,
];

pub fn tag_color_of(tag: &str) -> Color {
    let sum: usize = tag.bytes().map(usize::from).sum();
    TAG_COLORS[sum % TAG_COLORS.len()]
}

pub fn draw_the_screen(frame: &mut Frame, app: &mut App) {
    app.hits = HitMap::default();
    let [top, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    draw_the_status(frame, app, top);
    // The frames draw the panes, and the app draws what is in them: both are lent out.
    let layout = std::mem::take(&mut app.layout);
    let mut frames = std::mem::take(&mut app.frames);
    frames.draw(frame, body, &layout, app);
    app.layout = layout;
    app.frames = frames;
    draw_the_keys(frame, app, foot);
    overlays::draw_the_overlay(frame, app);
}

/// Draws the board's pane: its columns, and the picked task's pane beside them when it is
/// open.
pub fn draw_the_board(frame: &mut Frame, app: &mut App, area: Rect, focused: bool) {
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

/// Draws the top line: whose board this is, and what is going on it.
fn draw_the_status(frame: &mut Frame, app: &App, area: Rect) {
    let left = vec![
        Span::styled(
            " ☾ moon ",
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.session
                .repo_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            Style::new().fg(DIM),
        ),
    ];
    let going = app.agents_going().len();
    let reviews = app
        .board
        .review_requests
        .iter()
        .filter(|request| request.pending())
        .count();
    let right = vec![
        Span::styled(format!("● {going} going "), Style::new().fg(GOING)),
        Span::styled(format!(" ◆ {reviews} to review "), Style::new().fg(REVIEW)),
    ];
    let right_width: usize = right.iter().map(Span::width).sum();
    let left_width: usize = left.iter().map(Span::width).sum();
    let gap = (area.width as usize).saturating_sub(left_width + right_width);
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    frame.render_widget(Line::from(spans), area);
}

fn draw_the_keys(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(message) = app.flashed() {
        frame.render_widget(
            Line::styled(format!(" {message}"), Style::new().fg(Color::Red)),
            area,
        );
        return;
    }
    let keys: &[(&str, &str)] = match (app.board_is_focused(), app.prefix_armed) {
        (_, true) => &[
            ("o", "go to next frame"),
            ("n p", "show next/prev tab"),
            ("1-9", "show a run"),
            ("0", "show board"),
            ("t", "tell agent"),
            ("x", "close tab"),
            ("q", "quit"),
        ],
        (false, _) => &[(
            "ctrl-a",
            "then o goes to the next frame · 0 shows the board · x closes the tab · ? shows keys",
        )],
        (true, _) if app.picked_run.is_some() => &[
            ("enter", "open run"),
            ("x", "stop run"),
            ("R", "resume run"),
            ("T", "tell agent"),
            ("↑↓", "pick"),
            ("o", "list all runs"),
            ("?", "show keys"),
        ],
        (true, _) => &[
            ("arrows", "pick"),
            ("shift+arrows", "move card"),
            ("<>", "move column"),
            ("enter", "show details"),
            ("s", "start run"),
            ("o", "list runs"),
            ("n", "add card"),
            ("c C", "fold column · one-line cards"),
            ("r t e", "rename · tag · edit notes"),
            ("/", "filter"),
            ("tab", "go to next frame"),
            ("?", "show keys"),
            ("q", "quit"),
        ],
    };
    let mut spans = Vec::new();
    for (key, says) in keys {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::new().fg(Color::Black).bg(Color::Gray),
        ));
        spans.push(Span::raw(format!(" {says}  ")));
    }
    if !app.filter.is_empty() {
        spans.push(Span::styled(
            format!("filter: {}", app.filter),
            Style::new().fg(ACCENT),
        ));
    }
    frame.render_widget(Line::from(spans), area);
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

/// Says how long ago something was, for a run's row.
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
