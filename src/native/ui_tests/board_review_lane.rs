//! The review lane under the board: a column per review status, a card per review file, and a
//! status's hook going off when a card is dragged into it.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use egui_kittest::Harness;

use crate::{
    api::OpenSessionRequest,
    backend::local::LocalBackend,
    native::{
        Launch,
        app::App,
        board::review_lane::{review_card_id, review_hook_pill_id},
        theme::ThemeMode,
    },
};

use super::{CARDS, drag_like_a_hand, seeded_fixture, settle};

/// A window on a board of the three cards, its hooks started the way a real window's are.
struct LaneBoard {
    harness: Harness<'static>,
    seen: Arc<Mutex<Seen>>,
}

#[derive(Default)]
struct Seen {
    /// The review cards the lane drew, by review id.
    cards: Vec<(String, egui::Rect)>,
    /// Where the status headings' pills were drawn, by status.
    pills: Vec<(String, egui::Rect)>,
}

fn lane_board(root: &Path, render: bool) -> LaneBoard {
    for (task_id, title, created) in CARDS {
        let folder = root.join(format!(".moontasks/{task_id}"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("metadata.json"),
            format!(
                "{{\n  \"title\": \"{title}\",\n  \"status\": \"todo\",\n  \
                 \"created_at_unix\": {created},\n  \"resources\": []\n}}\n"
            ),
        )
        .unwrap();
    }

    let state = crate::server::build_state(Arc::new(Mutex::new(Instant::now())));
    crate::moontasks::hooks::start_hook_runner(state.core.clone());
    let launch = Launch {
        backend: Arc::new(LocalBackend::new(state)),
        open: Some(OpenSessionRequest {
            repo_path: root.display().to_string(),
            diff_target: None,
            active_commit: None,
        }),
        frame: crate::cli::Frame::Review,
    };
    let mut app = App::new(egui::Context::default(), launch);
    app.set_theme(ThemeMode::Dark);

    let opened = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let seen_in_ui = Arc::clone(&seen);
    let builder = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .with_theme(egui::Theme::Dark);
    let builder = if render { builder.wgpu() } else { builder };
    let harness = builder.build_ui(move |ui| {
        if !opened.load(Ordering::Relaxed)
            && matches!(app.model.stage, crate::native::model::Stage::Ready)
        {
            app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
            // The lane is a tab behind the board's: brought in front.
            app.open_pane(crate::native::panes::OpenPaneRequest::Reviews);
            opened.store(true, Ordering::Relaxed);
        }
        app.draw(ui);
        let mut seen = seen_in_ui.lock().expect("poisoned");
        seen.cards = app
            .model
            .review_requests
            .iter()
            .filter_map(|request| {
                let rect = ui
                    .ctx()
                    .read_response(review_card_id(&request.file.id))?
                    .rect;
                Some((request.file.id.clone(), rect))
            })
            .collect();
        seen.pills = ["requested", "reviewing", "done"]
            .into_iter()
            .filter_map(|status| {
                let rect = ui.ctx().read_response(review_hook_pill_id(status))?.rect;
                Some((status.to_string(), rect))
            })
            .collect();
    });
    LaneBoard { harness, seen }
}

/// Where a screenshot is left for a person to look at, when `MOON_REVIEW_LANE_SHOTS` names a
/// folder.
fn leave_a_screenshot(harness: &mut Harness<'_>, name: &str) {
    let Ok(dir) = std::env::var("MOON_REVIEW_LANE_SHOTS") else {
        return;
    };
    let image = harness.render().expect("the window could not be rendered");
    image
        .save(Path::new(&dir).join(format!("{name}.png")))
        .expect("the screenshot could not be saved");
}

fn write_the_parsers_reviews(fixture: &super::Fixture) {
    fixture.write(
        ".moontasks/reviews/requested/parser-tests.md",
        "---\ntask: write-the-parser-1111\nrepo: .\nbranch: parser-tests\npr: 415\n---\n\
         test(parser): cover the trailing newline\n",
    );
    fixture.write(
        ".moontasks/reviews/reviewing/parser.md",
        "---\ntask: write-the-parser-1111\nrepo: .\nbranch: parser\npr: 412\n---\n\
         feat(parser): read the files\n",
    );
    fixture.write(
        ".moontasks/reviews/done/login.md",
        "---\ntask: fix-the-login-page-2222\nrepo: .\npr: 409\ndeploy: deployed\n---\n\
         fix(login): keep the session\n",
    );
}

/// Dragging a review card into another status moves its file there, and that status's hook
/// starts a run on the review's card.
#[test]
fn dragging_a_review_into_a_status_moves_its_file_and_fires_the_statuss_hook() {
    let fixture = seeded_fixture("review-lane-drag");
    write_the_parsers_reviews(&fixture);
    let fired = fixture.root.join("hook-fired.txt");
    fixture.write(
        ".moontasks/hooks/reviews/reviewing.md",
        &format!(
            "---\nstart: shell\n---\necho \"{{pr}} from {{from}}\" > {}; sleep 20\n",
            fired.display()
        ),
    );
    let render = std::env::var("MOON_REVIEW_LANE_SHOTS").is_ok();
    let LaneBoard { mut harness, seen } = lane_board(&fixture.root, render);

    assert!(
        settle(&mut harness, || {
            let seen = seen.lock().expect("poisoned");
            seen.cards.len() == 3 && seen.pills.len() == 3
        }),
        "the lane never drew the three review cards and the status pills"
    );
    harness.run_steps(3);
    leave_a_screenshot(&mut harness, "review-lane");
    // Long enough for the runner to have taken its first look, which only notes where reviews are.
    let first_look = Instant::now() + Duration::from_secs(5);
    while Instant::now() < first_look {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }

    let (card, target) = {
        let seen = seen.lock().expect("poisoned");
        let card = seen
            .cards
            .iter()
            .find(|(id, _)| id == "parser-tests")
            .map(|(_, rect)| rect.center())
            .expect("the requested card is drawn");
        let target = seen
            .pills
            .iter()
            .find(|(status, _)| status == "reviewing")
            .map(|(_, rect)| rect.left_bottom() + egui::vec2(-40.0, 30.0))
            .expect("the reviewing heading is drawn");
        (card, target)
    };
    drag_like_a_hand(&mut harness, card, target, egui::Modifiers::NONE);

    let moved_to = fixture
        .root
        .join(".moontasks/reviews/reviewing/parser-tests.md");
    assert!(
        settle(&mut harness, || moved_to.is_file()),
        "the dragged review's file never arrived in reviewing/"
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !fired.exists() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let log = std::fs::read_to_string(fixture.root.join(".moontasks/hooks/hooks.log"))
        .unwrap_or_default();
    assert!(fired.exists(), "the reviewing hook never ran; log: {log}");
    let said = (0..100)
        .map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            std::fs::read_to_string(&fired).unwrap_or_default()
        })
        .find(|said| !said.is_empty())
        .unwrap_or_default();
    assert_eq!(said.trim(), "415 from requested");
    let log = std::fs::read_to_string(fixture.root.join(".moontasks/hooks/hooks.log"))
        .unwrap_or_default();
    assert!(
        log.contains("review reviewing: started on parser-tests"),
        "{log}"
    );
    harness.input_mut().events.push(egui::Event::PointerGone);
    // Long enough for the board to read the hook's shell onto the review's card.
    let shown = Instant::now() + Duration::from_secs(4);
    while Instant::now() < shown {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    leave_a_screenshot(&mut harness, "review-lane-after-drag");
}
