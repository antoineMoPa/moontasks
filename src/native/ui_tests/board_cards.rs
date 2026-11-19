//! A card's own offers: its notes, the files linked to it, and the sessions it can be given.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use egui_kittest::Harness;

use crate::native::{panes::Pane, theme::ThemeMode};

use super::{app_for, press_key, seeded_fixture, settle, type_letter};

/// The deploy list a task wrote sits under everything else on its card: a row per review file,
/// in the order their `order:` keys give, and a click on one opens that repo's review.
///
/// A review is pending until its file is moved to `committed/` - here both are in `requested/`.
#[test]
fn a_tasks_deploy_list_is_drawn_under_its_card() {
    use egui_kittest::kittest::Queryable as _;

    let fixture = seeded_fixture("board-review-requests");
    fixture.write(
        ".moontasks/deploy-the-thing-1111/metadata.json",
        "{\n  \"title\": \"Deploy the thing\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write_review_file(
        "requested",
        "root-take-the-module-forward",
        "task: deploy-the-thing-1111\nrepo: .\norder: 1\n",
        "chore: take the module forward\n",
    );
    fixture.write_review_file(
        "requested",
        "turbocharger-on-a-branch",
        "task: deploy-the-thing-1111\nrepo: vendor/turbocharger\n\
         branch: moontask/show-moon-icons-in-bootscreens-59c3e24c\norder: 2\n",
        "chore: on a branch\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    // The reviews the window has open, and which pane is in front, watched from out here.
    let reviews = Arc::new(Mutex::new(Vec::<String>::new()));
    let reviews_in_ui = Arc::clone(&reviews);
    let in_front = Arc::new(Mutex::new(None::<crate::native::panes::PaneKind>));
    let in_front_in_ui = Arc::clone(&in_front);

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
            *reviews_in_ui.lock().expect("poisoned") = app
                .model
                .layout
                .panes()
                .filter(|(_, pane)| pane.kind() == crate::native::panes::PaneKind::Review)
                .map(|(_, pane)| pane.tab_title())
                .collect();
            *in_front_in_ui.lock().expect("poisoned") = app
                .front_pane
                .and_then(|id| app.model.layout.pane(id))
                .map(|pane| pane.kind());
        });

    // The row waits on two answers from worker threads - the board, and the board's review
    // files - so it is waited for rather than looked for on a fixed frame.
    let row = "2 reviews".to_string();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && harness.query_by_label(row.as_str()).is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        harness.query_by_label(row.as_str()).is_some(),
        "the card should say how many reviews the task has, and how many are open"
    );

    // The row only counts the task's reviews: each one's own row is what opens it. A click
    // here leaves the board in front and opens no diff.
    harness.get_by_label(row.as_str()).click();
    for _ in 0..10 {
        harness.step();
    }
    assert_eq!(
        *in_front.lock().expect("poisoned"),
        Some(crate::native::panes::PaneKind::Tasks),
        "clicking the row should leave the board in front"
    );

    assert!(
        reviews.lock().expect("poisoned").len() <= 1,
        "and no review of the repo should have been opened for it"
    );
}

/// Under the reviews row, a card lists each review by its subject, and a click on one goes
/// straight to moon's review of it.
#[test]
fn each_review_is_listed_on_its_card_and_opens_its_review() {
    use egui_kittest::kittest::Queryable as _;

    let fixture = seeded_fixture("board-review-rows");
    fixture.write(
        ".moontasks/deploy-the-thing-1111/metadata.json",
        "{\n  \"title\": \"Deploy the thing\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write_review_file(
        "requested",
        "root-take-the-module-forward",
        "task: deploy-the-thing-1111\nrepo: .\norder: 1\n",
        "chore: take the module forward\n",
    );
    fixture.write_review_file(
        "deployed",
        "root-ship-it",
        "task: deploy-the-thing-1111\nrepo: .\npr: 412\n",
        "feat: ship it\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let reviews = Arc::new(Mutex::new(Vec::<String>::new()));
    let reviews_in_ui = Arc::clone(&reviews);
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
            *reviews_in_ui.lock().expect("poisoned") = app
                .model
                .layout
                .panes()
                .filter(|(_, pane)| pane.kind() == crate::native::panes::PaneKind::Review)
                .map(|(_, pane)| pane.tab_title())
                .collect();
        });

    let requested = "chore: take the module forward";
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && harness.query_by_label(requested).is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        harness.query_by_label(requested).is_some(),
        "the requested review should be listed on its card by its subject"
    );
    assert!(
        harness.query_by_label("feat: ship it").is_some(),
        "and so should the deployed one"
    );
    assert!(
        harness.query_by_label("PR 412").is_some(),
        "and the deployed one's row should name its pull request"
    );
    harness.run_steps(3);
    if let Ok(dir) = std::env::var("MOON_BOARD_CARD_SHOTS") {
        let image = harness.render().expect("the window could not be rendered");
        image
            .save(std::path::Path::new(&dir).join("card-review-rows.png"))
            .expect("the screenshot could not be saved");
    }

    harness.get_by_label(requested).click();
    assert!(
        settle(&mut harness, || !reviews
            .lock()
            .expect("poisoned")
            .is_empty()),
        "clicking a requested review should open moon's review of it"
    );
}

/// A review with a pull request opens on that with a click, so moon's own review of it is on
/// the row's right-click menu: `view in moon` opens the repo's diff.
#[test]
fn a_review_with_a_pull_request_is_viewed_in_moon_from_its_rows_menu() {
    use egui_kittest::kittest::Queryable as _;

    let fixture = seeded_fixture("board-review-row-menu");
    fixture.write(
        ".moontasks/deploy-the-thing-1111/metadata.json",
        "{\n  \"title\": \"Deploy the thing\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write_review_file(
        "requested",
        "root-ship-it",
        "task: deploy-the-thing-1111\nrepo: .\npr: https://example.com/o/r/pull/412\n",
        "feat: ship it\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let in_front = Arc::new(Mutex::new(None::<crate::native::panes::PaneKind>));
    let in_front_in_ui = Arc::clone(&in_front);
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
            *in_front_in_ui.lock().expect("poisoned") = app
                .front_pane
                .and_then(|id| app.model.layout.pane(id))
                .map(|pane| pane.kind());
        });

    let row = "feat: ship it";
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && harness.query_by_label(row).is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        harness.query_by_label("PR 412").is_some(),
        "the row should name its pull request"
    );
    assert_eq!(
        *in_front.lock().expect("poisoned"),
        Some(crate::native::panes::PaneKind::Tasks)
    );

    harness.get_by_label(row).click_secondary();
    harness.run_steps(3);
    let view_in_moon = crate::native::board::resources::VIEW_IN_MOON_LABEL;
    assert!(
        harness.query_by_label(view_in_moon).is_some(),
        "a right-click on a row with a pull request should offer moon's review"
    );
    assert!(
        harness.query_by_label_contains("move to").is_none(),
        "and the card's own menu should stay shut under the row's"
    );
    if let Ok(dir) = std::env::var("MOON_BOARD_CARD_SHOTS") {
        let image = harness.render().expect("the window could not be rendered");
        image
            .save(std::path::Path::new(&dir).join("card-review-row-menu.png"))
            .expect("the screenshot could not be saved");
    }
    harness.get_by_label(view_in_moon).click();
    assert!(
        settle(&mut harness, || *in_front.lock().expect("poisoned")
            == Some(crate::native::panes::PaneKind::Review)),
        "and choosing it should bring moon's review of the repo in front"
    );
}

/// The × at the end of a review's row on its card dismisses that review - deletes its file -
/// and opens nothing, though the row under it is a click that opens the review.
#[test]
fn a_reviews_dismiss_mark_on_its_card_deletes_it_and_opens_nothing() {
    use egui_kittest::kittest::Queryable as _;

    let fixture = seeded_fixture("board-review-dismiss");
    fixture.write(
        ".moontasks/deploy-the-thing-1111/metadata.json",
        "{\n  \"title\": \"Deploy the thing\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write_review_file(
        "requested",
        "root-take-the-module-forward",
        "task: deploy-the-thing-1111\nrepo: .\n",
        "chore: take the module forward\n",
    );
    fixture.write_review_file(
        "requested",
        "root-ship-it",
        "task: deploy-the-thing-1111\nrepo: .\n",
        "feat: ship it\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let reviews = Arc::new(Mutex::new(0usize));
    let reviews_in_ui = Arc::clone(&reviews);
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
            *reviews_in_ui.lock().expect("poisoned") = app
                .model
                .layout
                .panes()
                .filter(|(_, pane)| pane.kind() == crate::native::panes::PaneKind::Review)
                .count();
        });

    let dismissed = "chore: take the module forward";
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && harness.query_by_label(dismissed).is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(3);
    // The window opens on a review of its own: what counts is that no other one opens.
    let reviews_before = *reviews.lock().expect("poisoned");
    let row = harness.get_by_label(dismissed).rect();
    harness
        .query_all_by_label(crate::native::board::review_dismiss::DISMISS_MARK_LABEL)
        .find(|mark| (mark.rect().center().y - row.center().y).abs() < row.height())
        .expect("the review's row has a dismiss mark")
        .click();

    let file = fixture
        .root
        .join(".moontasks/reviews/requested/root-take-the-module-forward.md");
    assert!(
        settle(&mut harness, || !file.exists()),
        "the dismissed review's file should be deleted"
    );
    harness.run_steps(10);
    assert!(
        harness.query_by_label(dismissed).is_none(),
        "the dismissed review should be off its card"
    );
    assert!(
        harness.query_by_label("feat: ship it").is_some(),
        "the other review should stay"
    );
    assert_eq!(
        *reviews.lock().expect("poisoned"),
        reviews_before,
        "dismissing opened a review"
    );
}

/// A review moved to `committed/` reads as reviewed on its card, though its repo still has changes.
///
/// Where the review file is is what says whether it is pending - not the card's column, and not
/// whether the repo is clean - so moving the file back to `requested/` asks for it again.
#[test]
fn a_review_that_is_done_reads_as_reviewed_on_its_card() {
    use egui_kittest::kittest::Queryable as _;

    let fixture = seeded_fixture("board-review-requests-done");
    fixture.write(
        ".moontasks/deploy-the-thing-1111/metadata.json",
        "{\n  \"title\": \"Deploy the thing\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write_review_file(
        crate::moontasks::reviews::COMMITTED_REVIEW_STATUS,
        "root-take-the-module-forward",
        "task: deploy-the-thing-1111\nrepo: .\n",
        "chore: take the module forward\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    // Whether the review reads as pending, watched from out here: the row's dot says it, green
    // or yellow, and a colour is not something to query for.
    let pending_read = Arc::new(Mutex::new(None::<bool>));
    let pending_in_ui = Arc::clone(&pending_read);

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
            *pending_in_ui.lock().expect("poisoned") = app
                .model
                .review_requests
                .first()
                .map(|request| request.pending());
        });

    // The board and its review files are read on worker threads, so the row is waited for.
    let reviewed = "1 review".to_string();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && harness.query_by_label(reviewed.as_str()).is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        harness.query_by_label(reviewed.as_str()).is_some(),
        "a review in committed/ should be counted on its card"
    );
    assert_eq!(
        *pending_read.lock().expect("poisoned"),
        Some(false),
        "and not as pending, though the repo still has changes to commit"
    );
}

/// A card's notes are its description: their first lines sit under the title, and a click
/// opens the task's own pane with the keyboard already in its notes box - so the words go
/// where the click was aimed without a file being opened beside the board. A task with none
/// offers `[add notes]` instead, which is the same click, and what is typed there is what
/// makes `notes.md` real.
#[test]
fn a_cards_notes_open_the_task_ready_to_write() {
    use egui_kittest::kittest::Queryable as _;

    let fixture = seeded_fixture("board-notes");
    for (task_id, title) in [
        ("write-the-parser-1111", "Write the parser"),
        ("fix-the-login-page-2222", "Fix the login page"),
    ] {
        fixture.write(
            &format!(".moontasks/{task_id}/metadata.json"),
            &format!(
                "{{\n  \"title\": \"{title}\",\n  \"status\": \"todo\",\n  \
                 \"created_at_unix\": 1700000000,\n  \"resources\": []\n}}\n"
            ),
        );
    }
    // One task has notes to show; the other has none, and offers to start them.
    fixture.write(
        ".moontasks/write-the-parser-1111/notes.md",
        "Ship it by Friday, working top down.\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let ready = Arc::new(AtomicBool::new(false));
    let ready_in_ui = Arc::clone(&ready);
    // The task whose pane the clicks end up opening, watched for from out here: the app is the
    // closure's from the moment the harness is built.
    let task_pane: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let task_pane_in_ui = Arc::clone(&task_pane);
    // The files open in the window, which the clicks must not add to any more: the notes are
    // written on the task's pane rather than in a file beside it.
    let file_panes = Arc::new(Mutex::new(Vec::<String>::new()));
    let file_panes_in_ui = Arc::clone(&file_panes);
    // The task the window is being worked in: a task's pane is one of its tabs, so the board
    // marks its card while that pane is in front.
    let worked_in: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let worked_in_in_ui = Arc::clone(&worked_in);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            app.draw(ui);
            *worked_in_in_ui.lock().expect("poisoned") = super::marked_task(&app);
            ready_in_ui.store(
                app.model.board.loaded && app.model.board.tasks.len() == 2,
                Ordering::Relaxed,
            );
            if let Ok(mut panes) = file_panes_in_ui.lock() {
                *panes = app
                    .model
                    .layout
                    .panes()
                    .filter_map(|(_, pane)| match pane {
                        Pane::File { file_path, .. } => Some(file_path.clone()),
                        _ => None,
                    })
                    .collect();
            }
            if let Ok(mut open) = task_pane_in_ui.lock() {
                *open = app.model.layout.panes().find_map(|(_, pane)| match pane {
                    Pane::Start { task_id, .. } => Some(task_id.clone()),
                    _ => None,
                });
            }
        });

    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !ready.load(Ordering::Relaxed) {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        ready.load(Ordering::Relaxed),
        "the board never read the two tasks out of .moontasks"
    );
    harness.run_steps(3);

    // The description shows on the one card, and the way to start one on the other.
    let pane_open = || task_pane.lock().expect("poisoned").clone();
    let files_open = || {
        file_panes
            .lock()
            .map(|panes| panes.clone())
            .unwrap_or_default()
    };

    harness.get_by_label_contains("Ship it by Friday").click();
    assert!(
        settle(&mut harness, || pane_open().as_deref()
            == Some("write-the-parser-1111")),
        "clicking the description should have opened that task's pane, got {:?}",
        pane_open()
    );
    // The keyboard is in the notes box already, with the caret past what is written there: the
    // click was someone reaching for those words, so the next ones land after them.
    harness.run_steps(2);
    type_letter(&mut harness, egui::Key::A, "And test it.");
    let notes = fixture
        .root
        .join(".moontasks/write-the-parser-1111/notes.md");
    assert!(
        settle(&mut harness, || std::fs::read_to_string(&notes).is_ok_and(
            |written| written == "Ship it by Friday, working top down.\nAnd test it."
        )),
        "the typing should have gone into the notes box, saw {:?}",
        std::fs::read_to_string(&notes)
    );

    // The other card, which has no notes to show and offers to start them instead: the same
    // click, on a task whose `notes.md` is not there yet.
    harness.get_by_label("[add notes]").click();
    assert!(
        settle(&mut harness, || pane_open().as_deref()
            == Some("fix-the-login-page-2222")),
        "[add notes] should have opened the other task's pane, got {:?}",
        pane_open()
    );
    harness.run_steps(2);
    type_letter(&mut harness, egui::Key::S, "Start with the session cookie");
    let started = fixture
        .root
        .join(".moontasks/fix-the-login-page-2222/notes.md");
    assert!(
        settle(&mut harness, || std::fs::read_to_string(&started)
            .is_ok_and(
                |written| written.contains("Start with the session cookie")
            )),
        "writing the notes is what makes the file real, saw {:?}",
        std::fs::read_to_string(&started)
    );

    // And neither click opened a file beside the board: the notes are written on the pane.
    assert!(
        files_open().is_empty(),
        "the notes should not have opened as files, got {:?}",
        files_open()
    );
    assert_eq!(
        *worked_in.lock().expect("poisoned"),
        Some("fix-the-login-page-2222".to_string()),
        "the pane is the second task's, so its card is the one marked"
    );
}

/// A file on a card is a way back to the file: its path opens it in a pane, the way the
/// notes do. And `[start]` is where one is put there, through the file finder - a pick there
/// lands on the card and opens, rather than only opening.
#[test]
fn a_linked_file_opens_from_its_card_and_start_links_another() {
    let fixture = seeded_fixture("board-files");
    fixture.write(
        ".moontasks/write-the-parser-1111/metadata.json",
        "{\n  \"title\": \"Write the parser\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": [\n    {\n      \
         \"id\": \"file-1111\",\n      \"kind\": \"file\",\n      \
         \"file_path\": \"src/extra.rs\",\n      \"started_at_unix\": 1700000001\n    }\n  ]\n}\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let ready = Arc::new(AtomicBool::new(false));
    let ready_in_ui = Arc::clone(&ready);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let file_panes = Arc::new(Mutex::new(Vec::<String>::new()));
    let file_panes_in_ui = Arc::clone(&file_panes);
    // What the finder has searched for, so the test can wait for the answer to the typed name.
    let searched = Arc::new(Mutex::new(None::<String>));
    let searched_in_ui = Arc::clone(&searched);
    // How many resources the card has, read back out of the board's own last answer.
    let linked = Arc::new(Mutex::new(Vec::<String>::new()));
    let linked_in_ui = Arc::clone(&linked);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            app.draw(ui);
            ready_in_ui.store(
                app.model.board.loaded && app.model.board.tasks.len() == 1,
                Ordering::Relaxed,
            );
            if let Ok(mut panes) = file_panes_in_ui.lock() {
                *panes = app
                    .model
                    .layout
                    .panes()
                    .filter_map(|(_, pane)| match pane {
                        Pane::File { file_path, .. } => Some(file_path.clone()),
                        _ => None,
                    })
                    .collect();
            }
            if let Ok(mut searched) = searched_in_ui.lock() {
                *searched = app
                    .model
                    .palette
                    .files
                    .searched
                    .as_ref()
                    .filter(|_| app.model.palette.files.done)
                    .map(|asked| asked.query.clone());
            }
            if let Ok(mut linked) = linked_in_ui.lock() {
                *linked = app
                    .model
                    .board
                    .tasks
                    .iter()
                    .flat_map(|task| task.resources.iter())
                    .filter_map(|resource| resource.file_path.clone())
                    .collect();
            }
        });

    assert!(
        settle(&mut harness, || ready.load(Ordering::Relaxed)),
        "the board never read the task out of .moontasks"
    );
    harness.run_steps(3);
    let panes_open = || {
        file_panes
            .lock()
            .map(|panes| panes.clone())
            .unwrap_or_default()
    };

    // The card carries the file the way it carries a run: a mark, then the path.

    // The linked file is on the card by its path, and the path opens it.
    use egui_kittest::kittest::Queryable as _;
    harness.get_by_label("src/extra.rs").click();
    assert!(
        settle(&mut harness, || panes_open()
            .contains(&"src/extra.rs".to_string())),
        "clicking the linked file should have opened it, got {:?}",
        panes_open()
    );

    // `[start]` -> `file…` is the file finder, picking for this card: the pick is linked and
    // then opened.
    super::start_of_the_first_card(&harness).click();
    harness.run_steps(3);
    harness.get_by_label("file…").click();
    harness.run_steps(3);
    for (key, letter) in [
        (egui::Key::L, "l"),
        (egui::Key::I, "i"),
        (egui::Key::B, "b"),
    ] {
        type_letter(&mut harness, key, letter);
    }
    assert!(
        settle(&mut harness, || {
            searched.lock().expect("poisoned").as_deref() == Some("lib")
        }),
        "the finder never searched for what was typed - is ag installed?"
    );
    press_key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);

    assert!(
        settle(&mut harness, || panes_open()
            .contains(&"src/lib.rs".to_string())),
        "the picked file should have opened, got {:?}",
        panes_open()
    );
    assert!(
        settle(&mut harness, || linked
            .lock()
            .expect("poisoned")
            .contains(&"src/lib.rs".to_string())),
        "the picked file should be on the card, got {:?}",
        linked.lock().expect("poisoned")
    );
    let metadata = std::fs::read_to_string(
        fixture
            .root
            .join(".moontasks/write-the-parser-1111/metadata.json"),
    )
    .expect("failed to read the task's metadata");
    assert!(
        metadata.contains("\"file_path\": \"src/lib.rs\""),
        "the link should be written to the task folder: {metadata}"
    );
}

/// The attach modal offers the sessions the agents themselves have on this machine, which
/// is nothing a test can rely on - so the listing is injected, and what is checked is the
/// modal itself: what it shows, and Escape closing it.
#[test]
fn the_attach_modal_lists_the_agents_own_sessions() {
    use crate::api::AgentKind;

    let fixture = seeded_fixture("board-attach");
    fixture.write(
        ".moontasks/write-the-parser-1111/metadata.json",
        "{\n  \"title\": \"Write the parser\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened_in_ui = Arc::new(AtomicBool::new(false));
    // The board and its card have been read: the snapshot's background is settled.
    let ready = Arc::new(AtomicBool::new(false));
    let ready_in_ui = Arc::clone(&ready);
    let inject = Arc::new(AtomicBool::new(false));
    let inject_in_ui = Arc::clone(&inject);
    // What the modal's state is right now, read back out of the draw closure.
    let picker_open = Arc::new(AtomicBool::new(false));
    let picker_open_in_ui = Arc::clone(&picker_open);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            // Once, when the test says so - the way OpenAttachPicker fills it in, but with
            // sessions this machine is known not to have.
            if inject_in_ui.swap(false, Ordering::Relaxed) {
                app.model.board.attach_picker = Some(crate::native::model::AttachPicker {
                    task_id: "write-the-parser-1111".to_string(),
                    task_title: "Write the parser".to_string(),
                    sessions: Some(vec![
                        crate::agent_sessions::AgentSessionView {
                            agent: AgentKind::Claude,
                            id: "3f37e6a1-4a11-4333-8444-555555555555".to_string(),
                            title: "Fix the login page".to_string(),
                            updated_at_unix: 1_700_003_600,
                        },
                        crate::agent_sessions::AgentSessionView {
                            agent: AgentKind::OpenCode,
                            id: "ses_012f01ba5ffeTRe0q5MsyL9wbO".to_string(),
                            title: "Character-precise review selection".to_string(),
                            updated_at_unix: 1_699_900_000,
                        },
                        crate::agent_sessions::AgentSessionView {
                            agent: AgentKind::Codex,
                            id: "019efeff-2a80-7b11-b0b1-c5ab3e09b353".to_string(),
                            title: "Rewrite the scheduler".to_string(),
                            updated_at_unix: 1_699_800_000,
                        },
                    ]),
                    error: None,
                    manual_id: String::new(),
                    manual_agent: None,
                });
            }
            app.draw(ui);
            // The review has answered too: the modal offers its by-id entry for the agents
            // the review says are installed, and the board's read no longer queues behind
            // the review's git, so either can land first.
            let review_answered = app
                .model
                .review_ref(&app.model.root_session_id)
                .is_some_and(|review| review.payload.is_some());
            ready_in_ui.store(
                app.model.board.loaded && app.model.board.tasks.len() == 1 && review_answered,
                Ordering::Relaxed,
            );
            picker_open_in_ui.store(app.model.board.attach_picker.is_some(), Ordering::Relaxed);
        });

    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        harness.step();
        if ready.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        ready.load(Ordering::Relaxed),
        "the board never read the task out of .moontasks"
    );

    inject.store(true, Ordering::Relaxed);
    harness.run_steps(3);
    assert!(
        picker_open.load(Ordering::Relaxed),
        "the injected modal never showed"
    );

    // Escape is the way out that touches nothing.
    press_key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(
        !picker_open.load(Ordering::Relaxed),
        "escape did not close the attach modal"
    );
}

/// A markdown document listed in a task's `file_attachments.txt` is text the window writes
/// itself, so a click on it opens a tab on it the way `moon edit` does rather than handing it
/// to the machine's opener.
#[test]
fn a_markdown_attachment_opens_in_a_tab() {
    use egui_kittest::kittest::Queryable as _;

    const TASK: &str = "write-the-parser-1111";
    let fixture = seeded_fixture("board-markdown-attachment");
    fixture.write(
        &format!(".moontasks/{TASK}/metadata.json"),
        "{\n  \"title\": \"Write the parser\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write(
        &format!(".moontasks/{TASK}/file_attachments.txt"),
        "plan.md\n",
    );
    fixture.write(&format!(".moontasks/{TASK}/plan.md"), "# The plan\n");

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let ready = Arc::new(AtomicBool::new(false));
    let ready_in_ui = Arc::clone(&ready);
    // The files open in the window, each with whether it was opened in the window's own
    // session: the document is of the board's repo, so no other project is opened for it.
    let file_panes = Arc::new(Mutex::new(Vec::<(String, bool)>::new()));
    let file_panes_in_ui = Arc::clone(&file_panes);
    let errors = Arc::new(AtomicBool::new(false));
    let errors_in_ui = Arc::clone(&errors);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            app.draw(ui);
            ready_in_ui.store(
                app.model.board.loaded
                    && app
                        .model
                        .board
                        .tasks
                        .iter()
                        .any(|task| !task.attachments.is_empty()),
                Ordering::Relaxed,
            );
            let root_session_id = app.model.root_session_id.clone();
            *file_panes_in_ui.lock().expect("poisoned") = app
                .model
                .layout
                .panes()
                .filter_map(|(_, pane)| match pane {
                    Pane::File {
                        file_path,
                        session_id,
                        ..
                    } => Some((file_path.clone(), *session_id == root_session_id)),
                    _ => None,
                })
                .collect();
            errors_in_ui.store(!app.model.toasts.is_empty(), Ordering::Relaxed);
        });

    assert!(
        settle(&mut harness, || ready.load(Ordering::Relaxed)),
        "the board never read the task's attachment"
    );
    harness.run_steps(3);

    harness.get_by_label("plan.md").click();
    let files_open = || file_panes.lock().expect("poisoned").clone();
    assert!(
        settle(&mut harness, || !files_open().is_empty()),
        "clicking the attachment should have opened a tab on it"
    );
    assert_eq!(
        files_open(),
        vec![(format!(".moontasks/{TASK}/plan.md"), true)]
    );
    assert!(
        !errors.load(Ordering::Relaxed),
        "opening it said something went wrong"
    );
}

/// A board whose one card lists `document` in its `file_attachments.txt`, drawn until the board
/// has read it.
fn draw_a_card_attaching(fixture_name: &str, document: &str) -> Harness<'static> {
    const TASK: &str = "write-the-parser-1111";
    let fixture = seeded_fixture(fixture_name);
    fixture.write(
        &format!(".moontasks/{TASK}/metadata.json"),
        "{\n  \"title\": \"Write the parser\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write(
        &format!(".moontasks/{TASK}/file_attachments.txt"),
        &format!("{document}\n"),
    );

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let ready = Arc::new(AtomicBool::new(false));
    let ready_in_ui = Arc::clone(&ready);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            let _keep_the_folder = &fixture;
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            app.draw(ui);
            ready_in_ui.store(
                app.model.board.loaded
                    && app
                        .model
                        .board
                        .tasks
                        .iter()
                        .any(|task| !task.attachments.is_empty()),
                Ordering::Relaxed,
            );
        });

    assert!(
        settle(&mut harness, || ready.load(Ordering::Relaxed)),
        "the board never read the task's attachment"
    );
    harness.run_steps(3);
    harness
}

/// A document's name is cut to its row: a card is as wide as its column, and a name longer
/// than that used to be drawn whole, pushing the card out over the column beside it.
#[test]
fn a_long_attachment_name_stays_inside_its_card() {
    use egui_kittest::kittest::Queryable as _;

    const DOCUMENT: &str =
        "the-quarterly-figures-as-they-stood-before-the-second-round-of-corrections-final-v3.pdf";
    let harness = draw_a_card_attaching("board-long-attachment-name", DOCUMENT);

    let name = harness.get_by_label(DOCUMENT).rect();
    assert!(
        name.width() < crate::native::board::COLUMN_WIDTH,
        "the name is {} wide, in a column of {}",
        name.width(),
        crate::native::board::COLUMN_WIDTH
    );
}

/// A document listed by a path too long for its card keeps its file name and loses the path's
/// start, behind a `…`, rather than its end - the name is what the row is read for.
#[test]
fn a_long_attachment_path_keeps_its_file_name() {
    use egui_kittest::kittest::Queryable as _;

    const DOCUMENT: &str = "/Users/someone/Documents/a-folder-with-quite-a-long-name/another-folder-with-a-long-name/brief.pdf";
    let harness = draw_a_card_attaching("board-long-attachment-path", DOCUMENT);

    use egui_kittest::kittest::NodeT as _;

    let row = harness.get_by_label_contains("brief.pdf");
    let shown = row.accesskit_node().label().unwrap_or_default();
    assert!(
        shown.starts_with('…') && shown.ends_with("-long-name/brief.pdf"),
        "the path should lose its start and keep what fits of its folders and name: {shown}"
    );
    assert!(
        row.rect().width() < crate::native::board::COLUMN_WIDTH,
        "the path is {} wide, in a column of {}",
        row.rect().width(),
        crate::native::board::COLUMN_WIDTH
    );
}
