//! These tests draw the whole screen of a fresh board into a test terminal, and click on it.

use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
};

use moon_core::moontasks::ColumnEnd;

use crate::{
    app::{App, Overlay, PromptAsks},
    board_session::BoardSession,
};

/// Opens a board in a new git repo of its own, with one card in progress.
fn open_a_fresh_board() -> App {
    let folder = std::env::temp_dir().join(format!(
        "moon-tui-screen-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&folder)
        .status()
        .unwrap();
    let session = BoardSession::open_the_board_of(Some(folder)).unwrap();
    let mut app = App::new(session, false).unwrap();
    let column = app.board.columns[app.column].id.clone();
    let task_id = app
        .session
        .create_card("fix the races", &column, ColumnEnd::Top)
        .unwrap();
    app.read_the_board_again();
    app.pick_task(&task_id);
    app
}

fn draw(terminal: &mut Terminal<TestBackend>, app: &mut App) -> Vec<String> {
    terminal
        .draw(|frame| super::draw_the_screen(frame, app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

fn click(app: &mut App, column: u16, row: u16) {
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        crate::pointer::handle_the_mouse(
            app,
            MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            },
        );
    }
}

fn find(screen: &[String], text: &str) -> (u16, u16) {
    screen
        .iter()
        .enumerate()
        .find_map(|(y, row)| {
            row.find(text)
                .map(|byte| (row[..byte].chars().count() as u16, y as u16))
        })
        .unwrap_or_else(|| panic!("{text:?} is not on screen:\n{}", screen.join("\n")))
}

#[test]
fn the_foot_shows_no_key_hints() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    // The board's bottom border is the last row: no line of hints under it.
    assert!(screen.last().unwrap().trim_start().starts_with('╰'));
}

/// Finds the column's `+` buttons on the picked card's column, top to bottom.
fn plus_buttons_around(screen: &[String], card: &str) -> Vec<(u16, u16)> {
    let (card_x, card_y) = find(screen, card);
    screen
        .iter()
        .enumerate()
        .filter_map(|(y, row)| {
            row.match_indices("[  +  ]")
                .map(|(byte, _)| (row[..byte].chars().count() as u16, y as u16))
                .find(|(x, _)| x.abs_diff(card_x) < 40)
        })
        .inspect(|(_, y)| assert_ne!(*y, card_y))
        .collect()
}

#[test]
fn the_plus_above_the_first_card_adds_at_the_top_and_below_the_last_at_the_bottom() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let (_, card_y) = find(&screen, "fix the races");
    let buttons = plus_buttons_around(&screen, "fix the races");
    assert_eq!(buttons.len(), 2, "{}", screen.join("\n"));
    assert!(buttons[0].1 < card_y && buttons[1].1 > card_y);
    for ((x, y), end) in buttons.into_iter().zip([ColumnEnd::Top, ColumnEnd::Bottom]) {
        app.overlay = None;
        click(&mut app, x + 3, y);
        assert!(matches!(
            &app.overlay,
            Some(Overlay::Prompt {
                asks: PromptAsks::NewCard { joins, .. },
                ..
            }) if *joins == end
        ));
    }
    let screen = draw(&mut terminal, &mut app);
    find(&screen, "new task");
}

#[test]
fn a_click_on_the_picked_cards_start_offers_what_to_start() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, "[start]");
    click(&mut app, x + 1, y);
    assert!(matches!(app.overlay, Some(Overlay::StartMenu { .. })));
}

#[test]
fn a_click_on_the_strips_plus_starts_a_shell_tab_on_the_picked_card() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, " + ");
    assert_eq!(y, 0, "the strip's + is on the tab strip");
    click(&mut app, x + 1, y);
    let screen = draw(&mut terminal, &mut app);
    assert_eq!(app.shells.len(), 1);
    find(&screen, "shell 1 · fix the races");
}

fn type_into_the_palette(app: &mut App, key: ratatui::crossterm::event::KeyEvent) {
    let Some(Overlay::Palette { line, index }) = app.overlay.take() else {
        panic!("the palette is not open");
    };
    crate::palette::press_on_the_palette(app, line, index, key);
}

#[test]
fn the_palette_finds_rename_task_and_runs_it() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    crate::board_keys::press_on_the_board(
        &mut app,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
    );
    for letter in "ren ta".chars() {
        type_into_the_palette(
            &mut app,
            KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE),
        );
    }
    let screen = draw(&mut terminal, &mut app);
    find(&screen, "Rename task");
    assert!(!screen.join("\n").contains("Edit notes"));
    type_into_the_palette(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.overlay,
        Some(Overlay::Prompt {
            asks: PromptAsks::Rename { .. },
            ..
        })
    ));
}

#[test]
fn the_palette_moves_the_task_to_another_column() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    let mut app = open_a_fresh_board();
    crate::palette::open_the_palette(&mut app);
    for letter in "move done".chars() {
        type_into_the_palette(
            &mut app,
            KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE),
        );
    }
    type_into_the_palette(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let done = app
        .board
        .columns
        .iter()
        .find(|column| column.label.eq_ignore_ascii_case("done"))
        .unwrap()
        .id
        .clone();
    assert_eq!(app.board.cards_in(&done).len(), 1);
}

fn move_the_pointer_to(app: &mut App, column: u16, row: u16) {
    crate::pointer::handle_the_mouse(
        app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
    );
}

fn ground_at(terminal: &Terminal<TestBackend>, x: u16, y: u16) -> ratatui::style::Color {
    terminal.backend().buffer()[(x, y)].bg
}

#[test]
fn what_the_pointer_is_over_is_lit_and_nothing_else() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let plus = find(&screen, "[  +  ]");
    let heading = find(&screen, "IN PROGRESS");
    let card = find(&screen, "fix the races");
    let unlit: Vec<_> = [plus, heading]
        .iter()
        .map(|(x, y)| ground_at(&terminal, *x, *y))
        .collect();
    for (at, before) in [plus, heading].into_iter().zip(unlit) {
        move_the_pointer_to(&mut app, at.0 + 1, at.1);
        draw(&mut terminal, &mut app);
        assert_ne!(
            ground_at(&terminal, at.0 + 1, at.1),
            before,
            "{at:?} is not lit"
        );
        // The card, elsewhere, is not.
        assert_eq!(ground_at(&terminal, card.0 + 2, card.1), super::PICKED);
    }
    move_the_pointer_to(&mut app, card.0 + 2, card.1);
    draw(&mut terminal, &mut app);
    assert_eq!(
        ground_at(&terminal, card.0 + 2, card.1),
        super::HOVER_ON_PICKED
    );
}

#[test]
fn the_palette_deletes_the_run_of_the_tab_in_front() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    let mut app = open_a_fresh_board();
    let task_id = app.picked_task_id().unwrap();
    crate::board_actions::start_on_the_task(
        &mut app,
        &task_id,
        crate::app::StartItem::Shell(moon_core::moontasks::StartFolder::Repo),
    );
    assert_eq!(app.shells.len(), 1);
    assert!(!app.board_is_focused());
    crate::palette::open_the_palette(&mut app);
    for letter in "delete this shell".chars() {
        type_into_the_palette(
            &mut app,
            KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE),
        );
    }
    type_into_the_palette(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.shells.is_empty());
    assert!(app.board.task(&task_id).unwrap().resources.is_empty());
    assert!(app.board_is_focused());
}

#[test]
fn a_runs_x_asks_once_then_deletes_it_on_the_second_click() {
    let mut app = open_a_fresh_board();
    let task_id = app.picked_task_id().unwrap();
    crate::board_actions::start_on_the_task(
        &mut app,
        &task_id,
        crate::app::StartItem::Shell(moon_core::moontasks::StartFolder::Repo),
    );
    app.focus_the_board();
    let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, "[x]");
    click(&mut app, x + 1, y);
    let screen = draw(&mut terminal, &mut app);
    find(&screen, "delete?");
    assert_eq!(app.board.task(&task_id).unwrap().resources.len(), 1);
    click(&mut app, x + 1, y);
    assert!(app.board.task(&task_id).unwrap().resources.is_empty());
    assert!(app.shells.is_empty());
    let screen = draw(&mut terminal, &mut app);
    assert!(!screen.join("\n").contains("[x]"));
}

/// Starts a shell on the picked task and brings the board back to the front.
fn start_a_shell(app: &mut App) -> String {
    let task_id = app.picked_task_id().unwrap();
    crate::board_actions::start_on_the_task(
        app,
        &task_id,
        crate::app::StartItem::Shell(moon_core::moontasks::StartFolder::Repo),
    );
    app.focus_the_board();
    task_id
}

#[test]
fn menus_take_clicks_and_a_click_outside_closes_them() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    crate::board_actions::offer_what_to_start(&mut app);
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, " shell in the task's folder");
    // Outside the box closes it; inside it, off its rows, leaves it open.
    click(&mut app, 0, 23);
    assert!(app.overlay.is_none());
    crate::board_actions::offer_what_to_start(&mut app);
    draw(&mut terminal, &mut app);
    click(&mut app, x + 3, y);
    assert!(app.overlay.is_none());
    assert_eq!(app.shells.len(), 1);
}

#[test]
fn a_palette_row_runs_on_a_click() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    crate::palette::open_the_palette(&mut app);
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, " Rename task");
    click(&mut app, x + 2, y);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Prompt {
            asks: PromptAsks::Rename { .. },
            ..
        })
    ));
}

#[test]
fn quitting_asks_with_buttons() {
    let mut app = open_a_fresh_board();
    start_a_shell(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    app.ask_to_quit();
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, "[stay]");
    click(&mut app, x + 1, y);
    assert!(app.overlay.is_none() && !app.quit);
    app.ask_to_quit();
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, "[quit]");
    click(&mut app, x + 1, y);
    assert!(app.quit);
}

#[test]
fn the_details_have_buttons_and_no_key_hints() {
    let mut app = open_a_fresh_board();
    start_a_shell(&mut app);
    app.task_pane_open = true;
    let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let text = screen.join("\n");
    for hint in ["e edits", "s starts", "/private", "/var/"] {
        assert!(!text.contains(hint), "{hint:?} is on screen:\n{text}");
    }
    find(&screen, "── notes [edit]");
    // A shell has nothing to resume it, so it offers no [stop]: only [x].
    let (_, y) = find(&screen, "shell 1 ");
    assert!(!screen[y as usize].contains("[stop]"));
    let (x, y) = find(&screen, "agents and shells [start]");
    click(&mut app, x + "agents and shells [".len() as u16, y);
    assert!(matches!(app.overlay, Some(Overlay::StartMenu { .. })));
}

#[test]
fn the_filter_shows_over_the_board_and_its_x_clears_it() {
    let mut app = open_a_fresh_board();
    app.filter = "zzz".to_string();
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, "/ zzz");
    let (cx, cy) = find(&screen[y as usize..y as usize + 1], "[x]");
    assert!(cx > x);
    click(&mut app, cx + 1, y + cy);
    assert!(app.filter.is_empty());
}

#[test]
fn start_shows_on_the_task_under_the_pointer() {
    let mut app = open_a_fresh_board();
    let column = app.board.columns[app.column].id.clone();
    app.session
        .create_card("another one", &column, ColumnEnd::Bottom)
        .unwrap();
    app.read_the_board_again();
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    let screen = draw(&mut terminal, &mut app);
    let (x, y) = find(&screen, "another one");
    assert!(!screen[y as usize].contains("[start]"));
    move_the_pointer_to(&mut app, x + 1, y);
    let screen = draw(&mut terminal, &mut app);
    assert!(screen[y as usize].contains("[start]"));
}

#[test]
fn only_a_failure_is_red() {
    let mut app = open_a_fresh_board();
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    app.flash("stopped shell 1".to_string());
    draw(&mut terminal, &mut app);
    let foot = terminal.backend().buffer()[(2, 11)].fg;
    assert_eq!(foot, crate::theme::DIM);
    app.flash_failure("could not start".to_string());
    draw(&mut terminal, &mut app);
    assert_eq!(
        terminal.backend().buffer()[(2, 11)].fg,
        crate::theme::DANGER
    );
}
