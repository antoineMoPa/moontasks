//! These tests put the frames under a mouse, drawn into a test terminal.

use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    text::Line,
};

use crate::{
    Frame as LayoutFrame, Frames, FramesEvent, Layout, PaneId, PaneView, PointerOutcome, Tab,
};

struct Names;

impl PaneView<&'static str> for Names {
    fn tab(&mut self, _: PaneId, name: &&'static str) -> Tab {
        Tab::new(*name)
    }

    fn draw_pane(
        &mut self,
        frame: &mut ratatui::Frame,
        area: Rect,
        _: PaneId,
        name: &&'static str,
        _: bool,
    ) {
        frame.render_widget(Line::raw(format!("inside {name}")), area);
    }
}

fn draw(terminal: &mut Terminal<TestBackend>, frames: &mut Frames, layout: &Layout<&'static str>) {
    terminal
        .draw(|frame| frames.draw(frame, frame.area(), layout, &mut Names))
        .unwrap();
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn row_of(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect()
}

fn names_in_order(layout: &Layout<&'static str>, frame: &LayoutFrame) -> Vec<&'static str> {
    frame
        .panes()
        .iter()
        .map(|pane| *layout.pane(*pane).unwrap())
        .collect()
}

/// Carries the pointer from one cell to another with the button down, drawing as it goes.
fn drag(
    terminal: &mut Terminal<TestBackend>,
    frames: &mut Frames,
    layout: &mut Layout<&'static str>,
    from: (u16, u16),
    to: (u16, u16),
) -> PointerOutcome {
    frames.handle_the_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), from.0, from.1),
        layout,
    );
    draw(terminal, frames, layout);
    frames.handle_the_mouse(
        mouse(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
        layout,
    );
    draw(terminal, frames, layout);
    let outcome = frames.handle_the_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), to.0, to.1),
        layout,
    );
    draw(terminal, frames, layout);
    outcome
}

fn three_tabs() -> (Terminal<TestBackend>, Frames, Layout<&'static str>) {
    let mut layout = Layout::with_pane("alpha");
    let frame = layout.active_frame();
    layout.add_pane(frame, "beta", None);
    layout.add_pane(frame, "gamma", None);
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    let mut frames = Frames::new();
    draw(&mut terminal, &mut frames, &layout);
    (terminal, frames, layout)
}

#[test]
fn the_tabs_are_drawn_on_the_strip_and_the_front_pane_under_it() {
    let (terminal, _, _) = three_tabs();
    assert!(row_of(&terminal, 0).starts_with(" alpha ×  beta ×  gamma ×"));
    assert!(row_of(&terminal, 1).starts_with("inside gamma"));
}

#[test]
fn a_tab_dragged_past_another_takes_its_place() {
    let (mut terminal, mut frames, mut layout) = three_tabs();
    // alpha is at 1, beta's middle at about 12: carried past gamma's middle, it goes last.
    let outcome = drag(&mut terminal, &mut frames, &mut layout, (2, 0), (22, 0));
    assert!(
        matches!(outcome, PointerOutcome::Taken(events) if matches!(events[..], [FramesEvent::PaneMoved(_)]))
    );
    let frame = layout.frame(layout.active_frame()).unwrap();
    assert_eq!(names_in_order(&layout, frame), ["beta", "gamma", "alpha"]);
    // It slides into its place rather than jumping there.
    assert!(frames.is_moving());
    for _ in 0..12 {
        draw(&mut terminal, &mut frames, &layout);
    }
    assert!(!frames.is_moving());
    assert!(row_of(&terminal, 0).starts_with(" beta ×  gamma ×  alpha ×"));
}

#[test]
fn a_tab_dropped_against_a_frames_right_edge_splits_it() {
    let (mut terminal, mut frames, mut layout) = three_tabs();
    drag(&mut terminal, &mut frames, &mut layout, (2, 0), (57, 6));
    assert_eq!(layout.frame_count(), 2);
    let right = layout.frame_ids()[1];
    assert_eq!(
        names_in_order(&layout, layout.frame(right).unwrap()),
        ["alpha"]
    );
    // There is a line between them, which drags to resize them.
    let divider = row_of(&terminal, 5)
        .find('│')
        .expect("a line between the frames") as u16;
    drag(
        &mut terminal,
        &mut frames,
        &mut layout,
        (divider, 5),
        (15, 5),
    );
    let moved = row_of(&terminal, 5).chars().position(|c| c == '│').unwrap() as u16;
    assert!(moved < divider, "the line moved left: {divider} -> {moved}");
}

#[test]
fn the_close_button_asks_and_a_click_in_a_pane_is_the_panes() {
    let (_, mut frames, mut layout) = three_tabs();
    let close = frames.handle_the_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), 7, 0),
        &mut layout,
    );
    assert!(
        matches!(close, PointerOutcome::Taken(events) if matches!(events[..], [FramesEvent::PaneCloseRequested(_)]))
    );
    let inside = frames.handle_the_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), 20, 6),
        &mut layout,
    );
    assert!(matches!(inside, PointerOutcome::InPane { .. }));
}

#[test]
fn a_click_on_the_new_tab_button_asks_for_a_tab_in_its_frame() {
    let (mut terminal, _, mut layout) = three_tabs();
    let mut frames = Frames::new().with_new_tab_button(true);
    draw(&mut terminal, &mut frames, &layout);
    assert!(row_of(&terminal, 0).starts_with(" alpha ×  beta ×  gamma ×  + "));
    let outcome = frames.handle_the_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), 27, 0),
        &mut layout,
    );
    assert_eq!(
        outcome,
        PointerOutcome::Taken(vec![FramesEvent::NewTabRequested(layout.active_frame())])
    );
}

#[test]
fn the_strip_has_no_new_tab_button_unless_asked() {
    let (terminal, _, _) = three_tabs();
    assert!(!row_of(&terminal, 0).contains('+'));
}

#[test]
fn the_new_tab_button_under_the_pointer_is_lit() {
    let (mut terminal, _, mut layout) = three_tabs();
    let mut frames = Frames::new().with_new_tab_button(true);
    draw(&mut terminal, &mut frames, &layout);
    let unlit = terminal.backend().buffer()[(27, 0)].fg;
    frames.handle_the_mouse(mouse(MouseEventKind::Moved, 27, 0), &mut layout);
    draw(&mut terminal, &mut frames, &layout);
    let lit = terminal.backend().buffer()[(27, 0)].fg;
    assert_ne!(lit, unlit);
    assert_eq!(lit, crate::FramesStyle::default().accent);
}
