//! This module draws the pieces the screen is built from, so each looks and acts the same
//! wherever it is: a button, and the row an agent or a shell takes on its task.

use moon_core::moontasks::TaskResourceView;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::{age_of, cut_to};
use crate::{
    app::App,
    hits::Hit,
    run_menu::{is_going, kind_of_the_run, label_on_the_card},
    theme::{ACCENT, DANGER, DIM, GOING, HANDLE, ON_ACCENT},
};

/// Says how a button is drawn at rest.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Dim, as chrome: `[  +  ]`, `[x]`, `[edit]`.
    Quiet,
    /// On the accent: the main action where it is drawn - `[start]`.
    Main,
    /// In red: a click here cannot be undone - an armed `[x]`.
    Danger,
}

/// Draws a button at `x, y` and records what a click on it does. Returns its width, nothing
/// when it does not fit before `right`.
pub fn draw_a_button(
    frame: &mut Frame,
    app: &mut App,
    (x, y): (u16, u16),
    right: u16,
    label: &str,
    look: Look,
    hit: Hit,
) -> u16 {
    let width = label.chars().count() as u16;
    if x + width > right {
        return 0;
    }
    let style = match look {
        Look::Quiet => Style::new().fg(DIM),
        Look::Main => Style::new().fg(ON_ACCENT).bg(ACCENT),
        Look::Danger => Style::new().fg(DANGER).add_modifier(Modifier::BOLD),
    };
    let rect = Rect::new(x, y, width, 1);
    frame.render_widget(Line::styled(label.to_string(), style), rect);
    app.hits.record(rect, hit);
    width
}

/// Sets the button that ends an agent or a shell and takes it off its task.
pub const DELETE_BUTTON: &str = "[x]";

/// Draws the row an agent or a shell takes on its task: its mark, its name, its age, and the
/// buttons that act on it - for an agent, `[stop]` or `[resume]` when `with_stop`; then `[x]`. A click on the row itself opens it. `picked` lights the row as the keys' choice.
pub fn draw_a_run_row(
    frame: &mut Frame,
    app: &mut App,
    rect: Rect,
    task: (&str, &str),
    run: &TaskResourceView,
    picked: bool,
    with_stop: bool,
) {
    let (task_id, task_title) = task;
    let own = std::process::id();
    let (mark, color) = match (run.running, run.going_elsewhere_in) {
        (_, Some(moon)) if moon != own => ("◌", HANDLE),
        (true, _) if run.attention.is_some() => ("!", ACCENT),
        (true, _) => ("●", GOING),
        (false, _) => ("○", DIM),
    };
    let base = if picked {
        Style::new()
            .fg(ON_ACCENT)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    let lit = |style: Style| if picked { base } else { style };
    app.hits.record(
        rect,
        Hit::Run {
            task_id: task_id.to_string(),
            run_id: run.id.clone(),
        },
    );
    // Once its `[x]` was clicked, the row asks before a second click deletes it.
    let armed = app.delete_is_armed_for(&run.id);
    let after = if armed {
        " delete?".to_string()
    } else {
        format!(" {}", age_of(run.started_at_unix))
    };
    // A shell stopped is gone - nothing resumes it - so only an agent offers to stop.
    let stop = (with_stop && kind_of_the_run(run) == "agent").then(|| {
        if is_going(run) {
            (
                "[stop]",
                Hit::StopRun {
                    task_id: task_id.to_string(),
                    run_id: run.id.clone(),
                },
            )
        } else {
            (
                "[resume]",
                Hit::ResumeRun {
                    task_id: task_id.to_string(),
                    run_id: run.id.clone(),
                },
            )
        }
    });
    let buttons_width =
        DELETE_BUTTON.len() + 1 + stop.as_ref().map_or(0, |(label, _)| label.len() + 1);
    let width = rect.width as usize;
    let label_width = width.saturating_sub(4 + after.chars().count() + buttons_width);
    let name = label_on_the_card(run, task_title);
    frame.render_widget(
        Line::from(vec![
            Span::styled(format!("  {mark} "), lit(Style::new().fg(color))),
            Span::styled(
                format!("{:<label_width$}", cut_to(&name, label_width)),
                lit(Style::new().fg(HANDLE)),
            ),
            Span::styled(
                after,
                if armed {
                    Style::new().fg(DANGER)
                } else {
                    lit(Style::new().fg(DIM))
                },
            ),
        ])
        .style(base),
        rect,
    );
    let mut x = rect.right().saturating_sub(buttons_width as u16) + 1;
    if let Some((label, hit)) = stop {
        x += draw_a_button(
            frame,
            app,
            (x, rect.y),
            rect.right(),
            label,
            Look::Quiet,
            hit,
        ) + 1;
    }
    draw_a_button(
        frame,
        app,
        (x, rect.y),
        rect.right(),
        DELETE_BUTTON,
        if armed { Look::Danger } else { Look::Quiet },
        Hit::DeleteRun {
            task_id: task_id.to_string(),
            run_id: run.id.clone(),
        },
    );
}
