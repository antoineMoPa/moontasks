//! The review lane under the board: a column per review status, a card per review file, and a
//! status's hook going off when a card is dragged into it, and its review opening when it is
//! clicked.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable};

use crate::{
    api::OpenSessionRequest,
    backend::local::LocalBackend,
    native::{
        Launch,
        app::App,
        board::review_lane::{DISMISS_MARK_LABEL, review_card_id, review_hook_pill_id},
        theme::ThemeMode,
    },
};

use super::{CARDS, click_like_a_hand, drag_like_a_hand, press_modifiers, seeded_fixture, settle};

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
    /// The reviews a review pane was opened from, which is what clicking a card does.
    opened_reviews: Vec<String>,
    /// The review cards marked, by review id, sorted.
    marked: Vec<String>,
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
    crate::moontasks::hooks::turn_off_network_check();
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
                    .read_response(review_card_id(&request.file.status, &request.file.id))?
                    .rect;
                Some((request.file.id.clone(), rect))
            })
            .collect();
        seen.pills = ["requested", "committed", "pushed", "deployed"]
            .into_iter()
            .filter_map(|status| {
                let rect = ui.ctx().read_response(review_hook_pill_id(status))?.rect;
                Some((status.to_string(), rect))
            })
            .collect();
        seen.marked = app
            .model
            .board
            .marked_reviews
            .iter()
            .map(|review| review.id.clone())
            .collect();
        seen.marked.sort();
        seen.opened_reviews = app
            .model
            .chosen_review_of_session
            .values()
            .cloned()
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
        ".moontasks/reviews/committed/parser.md",
        "---\ntask: write-the-parser-1111\nrepo: .\nbranch: parser\npr: 412\n---\n\
         feat(parser): read the files\n",
    );
    fixture.write(
        ".moontasks/reviews/pushed/login.md",
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
        ".moontasks/hooks/reviews/deployed.md",
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
            seen.cards.len() == 3 && seen.pills.len() == 4
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
            .find(|(status, _)| status == "deployed")
            .map(|(_, rect)| rect.left_bottom() + egui::vec2(-40.0, 30.0))
            .expect("the deployed heading is drawn");
        (card, target)
    };
    drag_like_a_hand(&mut harness, card, target, egui::Modifiers::NONE);

    let moved_to = fixture
        .root
        .join(".moontasks/reviews/deployed/parser-tests.md");
    assert!(
        settle(&mut harness, || moved_to.is_file()),
        "the dragged review's file never arrived in deployed/"
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !fired.exists() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let log = std::fs::read_to_string(fixture.root.join(".moontasks/hooks/hooks.log"))
        .unwrap_or_default();
    assert!(fired.exists(), "the deployed hook never ran; log: {log}");
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
        log.contains("review deployed: started on parser-tests"),
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

/// Press a card, nudge the pointer `by`, and let go there.
fn press_and_let_go(harness: &mut Harness<'_>, at: egui::Pos2, by: egui::Vec2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.run_steps(2);
    for (pos, pressed) in [(at, true), (at + by, false)] {
        if !pressed {
            harness
                .input_mut()
                .events
                .push(egui::Event::PointerMoved(pos));
            harness.step();
        }
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(2);
    }
}

/// Two quick clicks, each pressed and let go within a frame: the harness steps a quarter second
/// a frame, and egui's double-click is two clicks within 0.3s.
fn double_click(harness: &mut Harness<'_>, at: egui::Pos2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.run_steps(2);
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        harness.step();
    }
    harness.run_steps(2);
}

/// A click on a card marks it and opens nothing, so cmd+click can gather more beside it; a
/// double-click let go of within 3px of where it was pressed opens its review. One let go of
/// further away was the start of a drag, and opens nothing - though egui would still call it a
/// click.
#[test]
fn a_click_marks_a_review_card_and_a_double_click_opens_it() {
    let fixture = seeded_fixture("review-lane-click");
    write_the_parsers_reviews(&fixture);
    let LaneBoard { mut harness, seen } = lane_board(&fixture.root, false);
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").cards.len()
            == 3),
        "the lane never drew the three review cards"
    );
    let card_at = |id: &str| {
        seen.lock()
            .expect("poisoned")
            .cards
            .iter()
            .find(|(card, _)| card == id)
            .map(|(_, rect)| rect.center())
            .expect("the card is drawn")
    };

    press_and_let_go(&mut harness, card_at("parser-tests"), egui::vec2(2.0, 1.0));
    harness.run_steps(20);
    press_modifiers(&mut harness, egui::Modifiers::COMMAND);
    click_like_a_hand(&mut harness, card_at("login"), egui::Modifiers::COMMAND);
    press_modifiers(&mut harness, egui::Modifiers::NONE);
    harness.run_steps(20);
    {
        let seen = seen.lock().expect("poisoned");
        assert!(
            seen.opened_reviews.is_empty(),
            "a single click opened its review"
        );
        assert_eq!(
            seen.marked,
            ["login", "parser-tests"],
            "a click then a cmd+click did not mark both cards"
        );
    }

    for _ in 0..2 {
        press_and_let_go(&mut harness, card_at("parser"), egui::vec2(5.0, 0.0));
    }
    harness.run_steps(20);
    assert!(
        seen.lock().expect("poisoned").opened_reviews.is_empty(),
        "a card let go of 5px away, twice, opened its review"
    );

    double_click(&mut harness, card_at("parser-tests"));
    assert!(
        settle(&mut harness, || {
            seen.lock().expect("poisoned").opened_reviews == ["parser-tests"]
        }),
        "double-clicking the card never opened its review"
    );
}

/// A follow-up request can reuse the name of a review already pushed: each of the two is a card
/// of its own, each answering to its own id - not one id drawn twice, which egui marks in red.
#[test]
fn two_reviews_of_one_name_in_two_statuses_are_two_cards() {
    let fixture = seeded_fixture("review-lane-namesakes");
    write_the_parsers_reviews(&fixture);
    fixture.write(
        ".moontasks/reviews/requested/login.md",
        "---\ntask: fix-the-login-page-2222\nrepo: .\n---\nfix(login): keep the session longer\n",
    );
    let LaneBoard { mut harness, seen } = lane_board(&fixture.root, false);
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").cards.len()
            == 4),
        "the lane never drew the four review cards"
    );
    harness.run_steps(3);
    let logins: Vec<egui::Rect> = seen
        .lock()
        .expect("poisoned")
        .cards
        .iter()
        .filter(|(id, _)| id == "login")
        .map(|(_, rect)| *rect)
        .collect();
    assert_eq!(logins.len(), 2);
    assert!(
        !logins[0].intersects(logins[1]),
        "the two login reviews answer to one card: {logins:?}"
    );
}

/// The × on a review card dismisses that review: its file is deleted - the one in the card's
/// own folder, not a follow-up request reusing its name in another.
#[test]
fn the_dismiss_mark_on_a_review_card_deletes_that_review_and_not_its_namesake() {
    let fixture = seeded_fixture("review-lane-dismiss");
    write_the_parsers_reviews(&fixture);
    fixture.write(
        ".moontasks/reviews/requested/login.md",
        "---\ntask: fix-the-login-page-2222\nrepo: .\n---\nfix(login): keep the session longer\n",
    );
    let LaneBoard { mut harness, seen } = lane_board(&fixture.root, false);
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").cards.len()
            == 4),
        "the lane never drew the four review cards"
    );
    harness.run_steps(3);
    // The pushed login card is the rightmost of the two.
    let pushed_login = seen
        .lock()
        .expect("poisoned")
        .cards
        .iter()
        .filter(|(id, _)| id == "login")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the pushed login card is drawn");

    harness
        .query_all_by_label(DISMISS_MARK_LABEL)
        .find(|mark| pushed_login.contains(mark.rect().center()))
        .expect("the pushed login card has a dismiss mark")
        .click();

    let pushed = fixture.root.join(".moontasks/reviews/pushed/login.md");
    assert!(
        settle(&mut harness, || !pushed.exists()),
        "the dismissed review's file is still there"
    );
    assert!(
        fixture
            .root
            .join(".moontasks/reviews/requested/login.md")
            .is_file(),
        "dismissing the pushed login review deleted its requested namesake"
    );
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").cards.len()
            == 3),
        "the dismissed review's card is still drawn: {:?}",
        seen.lock().expect("poisoned").cards
    );
}

/// A branch too long for its card is cut short of the ×, and [moonreview] sits on a line of its
/// own under it - never drawn over the branch.
#[test]
fn a_long_branch_never_runs_under_the_review_cards_moonreview_link() {
    let fixture = seeded_fixture("review-lane-long-branch");
    let branch = "a-branch-name-long-enough-to-fill-the-whole-review-card-and-more";
    fixture.write(
        ".moontasks/reviews/requested/long.md",
        &format!(
            "---\ntask: write-the-parser-1111\nrepo: .\nbranch: {branch}\n---\n\
             feat(parser): a long branch\n"
        ),
    );
    let render = std::env::var("MOON_REVIEW_LANE_SHOTS").is_ok();
    let LaneBoard { mut harness, seen } = lane_board(&fixture.root, render);
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").cards.len()
            == 1),
        "the lane never drew the review card"
    );
    harness.run_steps(3);
    leave_a_screenshot(&mut harness, "review-lane-long-branch");
    let card = seen.lock().expect("poisoned").cards[0].1;

    let link = harness.get_by_label("[moonreview]").rect();
    let mark = harness.get_by_label(DISMISS_MARK_LABEL).rect();
    let branch_label = harness
        .query_all_by_label_contains(branch)
        .map(|node| node.rect())
        .find(|rect| card.contains(rect.center()))
        .expect("the branch is drawn on the card");

    assert!(
        link.top() >= branch_label.bottom(),
        "[moonreview] {link:?} is not on a line under the branch {branch_label:?}"
    );
    assert!(
        branch_label.right() <= mark.left(),
        "the branch {branch_label:?} runs under the × {mark:?}"
    );
    assert!(
        card.contains_rect(link),
        "[moonreview] {link:?} is outside its card {card:?}"
    );
}

/// cmd+click gathers review cards without opening them, and dragging one of the group carries
/// the whole group to the status it is dropped on - as a group of board cards is carried.
#[test]
fn cmd_clicked_review_cards_are_dragged_to_a_status_together() {
    let fixture = seeded_fixture("review-lane-group-drag");
    write_the_parsers_reviews(&fixture);
    let render = std::env::var("MOON_REVIEW_LANE_SHOTS").is_ok();
    let LaneBoard { mut harness, seen } = lane_board(&fixture.root, render);
    assert!(
        settle(&mut harness, || {
            let seen = seen.lock().expect("poisoned");
            seen.cards.len() == 3 && seen.pills.len() == 4
        }),
        "the lane never drew the three review cards and the status pills"
    );
    harness.run_steps(3);
    let card_at = |id: &str| {
        seen.lock()
            .expect("poisoned")
            .cards
            .iter()
            .find(|(card, _)| card == id)
            .map(|(_, rect)| rect.center())
            .expect("the card is drawn")
    };

    press_modifiers(&mut harness, egui::Modifiers::COMMAND);
    for id in ["parser-tests", "parser"] {
        click_like_a_hand(&mut harness, card_at(id), egui::Modifiers::COMMAND);
    }
    press_modifiers(&mut harness, egui::Modifiers::NONE);
    harness.run_steps(3);
    assert!(
        seen.lock().expect("poisoned").opened_reviews.is_empty(),
        "a cmd+click opened its review rather than marking the card"
    );
    leave_a_screenshot(&mut harness, "review-lane-two-marked");

    let target = seen
        .lock()
        .expect("poisoned")
        .pills
        .iter()
        .find(|(status, _)| status == "deployed")
        .map(|(_, rect)| rect.left_bottom() + egui::vec2(-40.0, 30.0))
        .expect("the deployed heading is drawn");
    drag_like_a_hand(
        &mut harness,
        card_at("parser"),
        target,
        egui::Modifiers::NONE,
    );

    let deployed = fixture.root.join(".moontasks/reviews/deployed");
    assert!(
        settle(&mut harness, || {
            deployed.join("parser-tests.md").is_file() && deployed.join("parser.md").is_file()
        }),
        "the two marked reviews never both arrived in deployed/"
    );
    assert!(
        fixture
            .root
            .join(".moontasks/reviews/pushed/login.md")
            .is_file(),
        "the unmarked review was carried along"
    );
    harness.run_steps(5);
    leave_a_screenshot(&mut harness, "review-lane-group-dropped");
}
