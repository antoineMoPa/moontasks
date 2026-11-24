//! A card sent to DONE takes its review tab with it - whether it was moved here, or by an
//! agent or the CLI writing the task's folder.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use egui_kittest::{Harness, kittest::Queryable as _};

use crate::native::{
    panes::{OpenPaneRequest, Pane},
    theme::ThemeMode,
};

use super::{app_for, click_at, right_click_at, seeded_fixture, settle};

const TASK: &str = "write-the-parser-2222";

/// A board with one card in todo, and the window opened on the board and that task's review.
/// Answers with the harness, whether the board has loaded, and whether the review tab is open.
fn open_the_board_and_the_task_review(
    name: &str,
) -> (
    super::Fixture,
    Harness<'static>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
) {
    let fixture = seeded_fixture(name);
    fixture.write(
        &format!(".moontasks/{TASK}/metadata.json"),
        "{\n  \"title\": \"Write the parser\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let loaded = Arc::new(AtomicBool::new(false));
    let loaded_in_ui = Arc::clone(&loaded);
    let review_open = Arc::new(AtomicBool::new(false));
    let review_open_in_ui = Arc::clone(&review_open);

    let harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(OpenPaneRequest::TaskReview {
                    task_id: TASK.to_string(),
                    title: "Write the parser".to_string(),
                });
                app.open_pane(OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            app.draw(ui);
            loaded_in_ui.store(app.model.board.loaded, Ordering::Relaxed);
            review_open_in_ui.store(
                app.model
                    .layout
                    .find_pane(
                        |pane| matches!(pane, Pane::TaskReview { task_id, .. } if task_id == TASK),
                    )
                    .is_some(),
                Ordering::Relaxed,
            );
        });
    (fixture, harness, loaded, review_open)
}

#[test]
fn a_card_moved_to_done_from_its_menu_closes_its_review_tab() {
    let (_fixture, mut harness, loaded, review_open) =
        open_the_board_and_the_task_review("done-closes-review-menu");
    assert!(
        settle(&mut harness, || loaded.load(Ordering::Relaxed)
            && review_open.load(Ordering::Relaxed)),
        "the board and the task's review should both be open"
    );

    let card = harness
        .ctx
        .read_response(crate::native::board::cards::card_drag_id(TASK))
        .expect("expected the card to have been drawn")
        .rect
        .center();
    right_click_at(&mut harness, card);
    let move_to_done = harness.get_by_label("move to DONE").rect().center();
    click_at(&mut harness, move_to_done);

    assert!(
        settle(&mut harness, || !review_open.load(Ordering::Relaxed)),
        "the review tab should close once its card is in DONE"
    );
}

#[test]
fn a_card_moved_to_done_elsewhere_closes_its_review_tab() {
    let (fixture, mut harness, loaded, review_open) =
        open_the_board_and_the_task_review("done-closes-review-elsewhere");
    assert!(
        settle(&mut harness, || loaded.load(Ordering::Relaxed)
            && review_open.load(Ordering::Relaxed)),
        "the board and the task's review should both be open"
    );

    // An agent finishing the task: the folder says done, nothing on the board was touched.
    fixture.write(
        &format!(".moontasks/{TASK}/metadata.json"),
        "{\n  \"title\": \"Write the parser\",\n  \"status\": \"done\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );

    assert!(
        settle(&mut harness, || !review_open.load(Ordering::Relaxed)),
        "the review tab should close once the board reads its card in DONE"
    );
}
