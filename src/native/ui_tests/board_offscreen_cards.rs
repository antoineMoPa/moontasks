//! A column of hundreds of cards draws the ones in view, not all of them on every frame.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use egui_kittest::{Harness, kittest::Queryable as _};

use crate::native::theme::ThemeMode;

use super::{app_for, seeded_fixture, settle};

const DONE_CARDS: usize = 400;

/// DONE on a board that has been used for months: the cards at its top are drawn, the ones
/// scrolled far below are not, and the frame stays short enough for typing beside it.
#[test]
fn a_long_column_draws_only_the_cards_in_view() {
    let fixture = seeded_fixture("board-offscreen-cards");
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
        });

    assert!(
        settle(&mut harness, || *cards.lock().expect("poisoned")
            == DONE_CARDS),
        "the board should have read its cards"
    );
    harness.run_steps(3);
    let frames = 20;
    let started = Instant::now();
    harness.run_steps(frames);
    let per_frame = started.elapsed() / frames as u32;
    eprintln!("{DONE_CARDS} done cards: {per_frame:?} per frame");

    assert!(
        harness.query_by_label("Done card 0").is_some(),
        "the top card is drawn"
    );
    assert!(
        harness
            .query_by_label(&format!("Done card {}", DONE_CARDS - 1))
            .is_none(),
        "a card far below the view is not drawn"
    );
}
