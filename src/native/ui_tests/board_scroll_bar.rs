//! A column's scroll bar floats over its cards, and a press on it is the bar's alone.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use egui_kittest::{Harness, kittest::Queryable as _};

use crate::native::theme::ThemeMode;

use super::{app_for, seeded_fixture, settle};

const DONE_CARDS: usize = 60;

/// From the left of a card's title to the middle of its column's scroll bar, which is drawn
/// over the right edge of the cards. The test's first assertion - that the column scrolled -
/// is what says this still lands on the bar.
const TITLE_TO_SCROLL_BAR: f32 = 264.0;

/// Dragging the bar of a long column scrolls the column, and the card lying under the bar
/// where the press went down stays where it is rather than being carried off.
#[test]
fn dragging_a_columns_scroll_bar_picks_up_no_card() {
    let fixture = seeded_fixture("board-scroll-bar");
    for card in 0..DONE_CARDS {
        fixture.write(
            &format!(".moontasks/done-card-{card:04}/metadata.json"),
            &format!(
                "{{\n  \"title\": \"Done card {card}\",\n  \"status\": \"done\",\n  \
                 \"created_at_unix\": 1700000000,\n  \"position\": {card},\n  \
                 \"resources\": []\n}}\n"
            ),
        );
    }

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let cards = Arc::new(Mutex::new(0usize));
    let cards_in_ui = Arc::clone(&cards);
    // Whether any frame had a card on the cursor, or a press on one in progress.
    let took_a_card = Arc::new(AtomicBool::new(false));
    let took_a_card_in_ui = Arc::clone(&took_a_card);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            app.draw(ui);
            *cards_in_ui.lock().expect("poisoned") = app.model.board.tasks.len();
            let board = &app.model.board;
            if board.carrying.is_some() || board.press.as_ref().is_some_and(|p| p.on.is_some()) {
                took_a_card_in_ui.store(true, Ordering::Relaxed);
            }
        });

    assert!(
        settle(&mut harness, || *cards.lock().expect("poisoned")
            == DONE_CARDS),
        "the board should have read its cards"
    );
    harness.run_steps(3);

    // The bar stands over the right edge of the cards, its handle at the top of the column.
    let top_card = harness.get_by_label("Done card 0").rect();
    let on_the_bar = egui::pos2(top_card.left() + TITLE_TO_SCROLL_BAR, top_card.center().y);
    let further_down = on_the_bar + egui::vec2(0.0, 300.0);

    // Resting on the bar is what brings it out, the way a hand finds it before pressing.
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(on_the_bar));
    harness.run_steps(12);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: on_the_bar,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    for step in 1..=6 {
        let towards = on_the_bar + (further_down - on_the_bar) * (step as f32 / 6.0);
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(towards));
        harness.step();
    }
    harness.run_steps(4);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: further_down,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(4);

    assert!(
        harness.query_by_label("Done card 0").is_none(),
        "the bar was dragged down, so the top card is scrolled out of view"
    );
    assert!(
        !took_a_card.load(Ordering::Relaxed),
        "and the card under the bar was neither pressed nor carried"
    );
}
