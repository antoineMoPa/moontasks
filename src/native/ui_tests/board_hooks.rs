//! The board's hooks: a column's pill and its editor, a schedule's pill, and a hook going off
//! when a card arrives.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable as _};

use crate::{
    api::OpenSessionRequest,
    backend::local::LocalBackend,
    moontasks::ColumnId,
    native::{Launch, app::App, board::hooks::column_pill_id, theme::ThemeMode},
};

use super::{CARDS, click_at, seeded_fixture, settle, type_letter};

const QA_HOOK: &str = ".moontasks/hooks/columns/in_progress.md";

/// A window on a board of the three cards, its hooks started the way a real window's are, and
/// what it has read of them.
struct HookedBoard {
    harness: Harness<'static>,
    seen: Arc<Mutex<Seen>>,
}

#[derive(Default)]
struct Seen {
    pill: Option<egui::Rect>,
    hooks_read: bool,
    editor_open: bool,
    /// How many cards the board has in the hooked column.
    hooked_column_cards: usize,
    /// What the open editor says went wrong, if anything.
    editor_error: Option<String>,
    run_marks: Vec<String>,
}

fn hooked_board(root: &Path, render: bool) -> HookedBoard {
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
        .with_size(egui::vec2(1400.0, 700.0))
        .with_theme(egui::Theme::Dark);
    let builder = if render { builder.wgpu() } else { builder };
    let harness = builder.build_ui(move |ui| {
        if !opened.load(Ordering::Relaxed)
            && matches!(app.model.stage, crate::native::model::Stage::Ready)
        {
            app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
            opened.store(true, Ordering::Relaxed);
        }
        app.draw(ui);
        let mut seen = seen_in_ui.lock().expect("poisoned");
        seen.pill = ui
            .ctx()
            .read_response(column_pill_id(&ColumnId::new("in_progress")))
            .map(|response| response.rect);
        seen.hooks_read = !app.model.board.hooks.columns.is_empty();
        seen.editor_open = app.model.board.hook_editor.is_some();
        seen.hooked_column_cards = app
            .model
            .board
            .tasks
            .iter()
            .filter(|task| task.status == ColumnId::new("in_progress"))
            .count();
        seen.editor_error = app
            .model
            .board
            .hook_editor
            .as_ref()
            .and_then(|editor| editor.error.clone());
        seen.run_marks = app
            .model
            .board
            .tasks
            .iter()
            .flat_map(|task| task.resources.iter())
            .filter_map(|run| run.started_by_hook.clone())
            .collect();
    });
    HookedBoard { harness, seen }
}

/// Where a screenshot is left for a person to look at, when `MOON_HOOKS_SHOTS` names a folder.
fn leave_a_screenshot(harness: &mut Harness<'_>, name: &str) {
    let Ok(dir) = std::env::var("MOON_HOOKS_SHOTS") else {
        return;
    };
    let image = harness.render().expect("the window could not be rendered");
    image
        .save(Path::new(&dir).join(format!("{name}.png")))
        .expect("the screenshot could not be saved");
}

/// A column's hook is a pill at the right of its heading. Pressing it opens the editor, and
/// what is saved there is the hook's file.
#[test]
fn a_column_hooks_pill_opens_its_editor_and_saving_writes_its_file() {
    let fixture = seeded_fixture("hooks-pill");
    fixture.write(QA_HOOK, "---\nstart: claude\n---\nRun the tests.\n");
    fixture.write(
        ".moontasks/hooks/board/standup.md",
        "---\nevery: weekdays\nat: 08:30\nstart: claude\n---\nRead the board.\n",
    );
    let render = std::env::var("MOON_HOOKS_SHOTS").is_ok();
    let HookedBoard { mut harness, seen } = hooked_board(&fixture.root, render);

    assert!(
        settle(&mut harness, || {
            let seen = seen.lock().expect("poisoned");
            seen.hooks_read && seen.pill.is_some_and(|pill| pill.is_positive())
        }),
        "the column's hook never showed as a pill"
    );
    harness.run_steps(3);
    leave_a_screenshot(&mut harness, "board-with-hooks");

    let pill = seen.lock().expect("poisoned").pill.expect("drawn");
    click_at(&mut harness, pill.center());
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").editor_open),
        "the pill did not open its editor"
    );
    harness.run_steps(3);
    // The prompt has the keyboard as the editor opens: what is typed goes on the end of it.
    type_letter(&mut harness, egui::Key::Exclamationmark, "!");
    leave_a_screenshot(&mut harness, "column-hook-editor");

    harness.get_by_label("[save]").click();
    assert!(
        settle(&mut harness, || !seen.lock().expect("poisoned").editor_open),
        "saving did not put the editor away"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join(QA_HOOK)).unwrap(),
        "---\nstart: claude\n---\nRun the tests.!\n"
    );
}

/// A card arriving in a column with a hook starts the hook's run on it, marked with the column -
/// here a shell, which the hook's commands are typed into.
#[test]
fn a_card_arriving_in_a_hooked_column_starts_the_hooks_run() {
    let fixture = seeded_fixture("hooks-fire");
    fixture.write(
        QA_HOOK,
        "---\nstart: shell\n---\necho \"{title} from {from}\" > hook-fired.txt\n",
    );
    let HookedBoard { mut harness, seen } = hooked_board(&fixture.root, false);
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").hooks_read),
        "the board never read its hook"
    );
    // Long enough for the runner to have taken its first look, which only notes where cards are.
    let first_look = Instant::now() + Duration::from_secs(5);
    while Instant::now() < first_look {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }

    // Moved the way an agent's `moon tasks move` or a text editor moves it: in its record.
    let (task_id, title, _) = CARDS[1];
    let record = fixture
        .root
        .join(format!(".moontasks/{task_id}/metadata.json"));
    let moved = std::fs::read_to_string(&record)
        .unwrap()
        .replace("\"status\": \"todo\"", "\"status\": \"in_progress\"");
    std::fs::write(&record, moved).unwrap();

    let fired = fixture.root.join("hook-fired.txt");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !fired.exists() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let log = std::fs::read_to_string(fixture.root.join(".moontasks/hooks/hooks.log"))
        .unwrap_or_default();
    assert!(
        fired.exists(),
        "the hook's shell never ran its command; log: {log}"
    );
    let said = (0..100)
        .map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            std::fs::read_to_string(&fired).unwrap_or_default()
        })
        .find(|said| !said.is_empty())
        .unwrap_or_default();
    assert_eq!(said.trim(), format!("{title} from TODO"));
    assert!(log.contains("IN PROGRESS: started on"), "{log}");
}

/// A schedule is made from the board's own `[start]`: its menu offers `schedule…`, which opens
/// the editor of a new one under it.
#[test]
fn a_schedule_is_made_from_the_boards_start_menu() {
    let fixture = seeded_fixture("hooks-schedule");
    let render = std::env::var("MOON_HOOKS_SHOTS").is_ok();
    let HookedBoard { mut harness, seen } = hooked_board(&fixture.root, render);
    let deadline = Instant::now() + Duration::from_secs(20);
    while harness.query_all_by_label("[start]").next().is_none() && Instant::now() < deadline {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(3);

    // The board's `[start]` is the first one, over the columns.
    harness
        .get_all_by_label("[start]")
        .next()
        .expect("drawn")
        .click();
    harness.run_steps(3);
    harness.get_by_label("schedule…").click();
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").editor_open),
        "schedule… did not open the editor"
    );
    harness.run_steps(3);
    // Nothing to run until it is saved.
    assert!(
        harness.query_by_label("[run now]").is_none(),
        "a schedule not saved yet offers [run now]"
    );

    // A new schedule's title has the keyboard.
    for (key, letter) in [
        (egui::Key::S, "s"),
        (egui::Key::Y, "y"),
        (egui::Key::N, "n"),
    ] {
        type_letter(&mut harness, key, letter);
    }
    harness.get_by_label("hour").click();
    harness.run_steps(2);
    leave_a_screenshot(&mut harness, "schedule-editor");
    harness.get_by_label("[save]").click();
    assert!(
        settle(&mut harness, || !seen.lock().expect("poisoned").editor_open),
        "saving did not put the editor away"
    );
    let written =
        std::fs::read_to_string(fixture.root.join(".moontasks/hooks/board/syn.md")).unwrap();
    assert!(
        written.starts_with("---\nevery: hour\nat: :00\n"),
        "{written}"
    );
}

/// A card made and moved to a hooked column between two of the runner's looks still arrived:
/// it entered its column after it was made.
#[test]
fn a_card_made_and_moved_between_two_looks_starts_the_hooks_run() {
    let fixture = seeded_fixture("hooks-made-and-moved");
    fixture.write(
        QA_HOOK,
        "---\nstart: shell\n---\necho \"{title} from {from}\" > hook-fired.txt\n",
    );
    let HookedBoard { mut harness, seen } = hooked_board(&fixture.root, false);
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").hooks_read),
        "the board never read its hook"
    );
    let first_look = Instant::now() + Duration::from_secs(5);
    while Instant::now() < first_look {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }

    let folder = fixture.root.join(".moontasks/made-and-moved-1");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("metadata.json"),
        "{\n  \"title\": \"Quick one\",\n  \"status\": \"in_progress\",\n  \
         \"created_at_unix\": 1794525834,\n  \"entered_column_at_unix\": 1794525835,\n  \
         \"resources\": []\n}\n",
    )
    .unwrap();

    let fired = fixture.root.join("hook-fired.txt");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !fired.exists() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let log = std::fs::read_to_string(fixture.root.join(".moontasks/hooks/hooks.log"))
        .unwrap_or_default();
    assert!(fired.exists(), "the hook never fired; log: {log}");
    let said = (0..100)
        .map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            std::fs::read_to_string(&fired).unwrap_or_default()
        })
        .find(|said| !said.is_empty())
        .unwrap_or_default();
    assert_eq!(said.trim(), "Quick one from a new card");
}

/// While another moon still running was the last window focused on the board, its hooks fire
/// there, not here; once it has gone, they fire here.
#[test]
fn hooks_fire_in_the_window_last_focused_on_the_board() {
    let fixture = seeded_fixture("hooks-watcher");
    fixture.write(
        QA_HOOK,
        "---\nstart: shell\n---\necho \"{title}\" >> hook-fired.txt\n",
    );
    // Another moon, as far as this one can tell: a process that is running.
    fixture.write(
        ".moontasks/hooks/.watcher",
        &std::os::unix::process::parent_id().to_string(),
    );
    let HookedBoard { mut harness, seen } = hooked_board(&fixture.root, false);
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").hooks_read),
        "the board never read its hook"
    );
    let step_for = |harness: &mut Harness<'static>, secs: u64| {
        let until = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < until {
            harness.step();
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    step_for(&mut harness, 5);

    let move_card = |index: usize| {
        let record = fixture
            .root
            .join(format!(".moontasks/{}/metadata.json", CARDS[index].0));
        let moved = std::fs::read_to_string(&record)
            .unwrap()
            .replace("\"status\": \"todo\"", "\"status\": \"in_progress\"");
        std::fs::write(&record, moved).unwrap();
    };
    let fired = fixture.root.join("hook-fired.txt");
    move_card(0);
    step_for(&mut harness, 6);
    assert!(
        !fired.exists(),
        "the hook fired in a moon that was not the watcher"
    );

    // The watcher has gone: a pid nothing runs under.
    fixture.write(".moontasks/hooks/.watcher", "4194303");
    move_card(1);
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !fired.exists() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        fired.exists(),
        "the hook never fired once the watcher had gone"
    );
    let said = (0..100)
        .map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            std::fs::read_to_string(&fired).unwrap_or_default()
        })
        .find(|said| !said.is_empty())
        .unwrap_or_default();
    assert_eq!(said.trim(), CARDS[1].1);
}

/// `[run now]` in a column hook's editor fires the hook at once on the column's top card, with
/// no card arriving - and counts nothing toward the hooks stopping on it.
#[test]
fn run_now_fires_a_column_hook_on_its_top_card_at_once() {
    let fixture = seeded_fixture("hooks-run-now");
    fixture.write(
        QA_HOOK,
        "---\nstart: shell\n---\necho \"{title} in {from}\" > ran-now.txt\n",
    );
    let render = std::env::var("MOON_HOOKS_SHOTS").is_ok();
    let HookedBoard { mut harness, seen } = hooked_board(&fixture.root, render);
    // Put in the column before the runner's first look, which only notes where cards are.
    let (task_id, title, _) = CARDS[1];
    let record = fixture
        .root
        .join(format!(".moontasks/{task_id}/metadata.json"));
    let moved = std::fs::read_to_string(&record)
        .unwrap()
        .replace("\"status\": \"todo\"", "\"status\": \"in_progress\"");
    std::fs::write(&record, moved).unwrap();

    assert!(
        settle(&mut harness, || {
            let seen = seen.lock().expect("poisoned");
            seen.hooks_read
                && seen.hooked_column_cards == 1
                && seen.pill.is_some_and(|pill| pill.is_positive())
        }),
        "the column's hook never showed as a pill over the card moved into it"
    );
    harness.run_steps(3);
    let pill = seen.lock().expect("poisoned").pill.expect("drawn");
    click_at(&mut harness, pill.center());
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").editor_open),
        "the pill did not open its editor"
    );
    harness.run_steps(3);
    leave_a_screenshot(&mut harness, "column-hook-editor-run-now");
    harness.get_by_label("[run now]").click();

    let ran = fixture.root.join("ran-now.txt");
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut said = String::new();
    while Instant::now() < deadline && said.trim().is_empty() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
        said = std::fs::read_to_string(&ran).unwrap_or_default();
    }
    let log = std::fs::read_to_string(fixture.root.join(".moontasks/hooks/hooks.log"))
        .unwrap_or_default();
    let error = seen.lock().expect("poisoned").editor_error.clone();
    assert_eq!(
        said.trim(),
        format!("{title} in IN PROGRESS"),
        "log: {log}; editor: {error:?}"
    );
    assert!(
        log.contains(&format!("IN PROGRESS: run now on {task_id}")),
        "{log}"
    );
    assert!(
        seen.lock().expect("poisoned").editor_open,
        "the editor went away, with what was typed in it"
    );
    let record = std::fs::read_to_string(&record).unwrap();
    assert!(!record.contains("hook_runs_in_a_row"), "{record}");
}

/// A hook's run that has ended leaves its card - a run started by hand stays - and the hook's
/// editor lists it under `past sessions`, with the way back to it.
#[test]
fn an_ended_hook_run_leaves_its_card_and_is_listed_in_the_hooks_editor() {
    let fixture = seeded_fixture("hooks-past-runs");
    fixture.write(QA_HOOK, "---\nstart: claude\n---\nRun the tests.\n");
    let run = |id: &str, name: &str, hook: Option<&str>| {
        format!(
            "{{\"id\": \"{id}\", \"kind\": \"agent\", \"agent\": \"claude\", \"file_path\": null, \
             \"terminal_id\": null, \"agent_session_id\": \"{id}\", \"name\": \"{name}\", \
             \"started_at_unix\": 1700000100{}}}",
            hook.map(|hook| format!(", \"started_by_hook\": \"{hook}\""))
                .unwrap_or_default()
        )
    };
    fixture.write(
        ".moontasks/hooked-card-4444/metadata.json",
        &format!(
            "{{\n  \"title\": \"Hooked card\",\n  \"status\": \"in_progress\",\n  \
             \"created_at_unix\": 1700000003,\n  \"resources\": [{}, {}]\n}}\n",
            run("a1", "hooked by hand", None),
            run("a2", "hooked by the column", Some("IN PROGRESS")),
        ),
    );
    let HookedBoard { mut harness, seen } =
        hooked_board(&fixture.root, std::env::var("MOON_HOOKS_SHOTS").is_ok());

    assert!(
        settle(&mut harness, || {
            let seen = seen.lock().expect("poisoned");
            seen.hooks_read && seen.pill.is_some_and(|pill| pill.is_positive())
        }),
        "the column's hook never showed as a pill"
    );
    harness.run_steps(3);
    assert!(
        harness.query_by_label("hooked by hand").is_some(),
        "a run started by hand left its card"
    );
    assert!(
        harness.query_by_label("hooked by the column").is_none(),
        "the hook's ended run is still on its card"
    );

    let pill = seen.lock().expect("poisoned").pill.expect("drawn");
    click_at(&mut harness, pill.center());
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").editor_open),
        "the pill did not open its editor"
    );
    harness.run_steps(3);
    leave_a_screenshot(&mut harness, "hook-editor-past-sessions");
    assert!(harness.query_by_label("PAST SESSIONS").is_some());
    assert!(
        harness.query_by_label("hooked by the column").is_some(),
        "the editor does not list the hook's run on its card"
    );
    // Its own, beside the one of the run on the card started by hand.
    assert_eq!(harness.query_all_by_label("resume").count(), 2);
}

/// A harness of the person's - see `crate::harnesses` - is one more thing a hook starts: picked
/// in the hook's editor it is what the file names, a card arriving runs its command with the
/// hook's prompt filled in, and a card's `[start]` offers it by name.
#[test]
fn a_harness_is_picked_in_a_hooks_editor_run_as_the_hook_fires_and_offered_by_start() {
    let fixture = seeded_fixture("hooks-harness");
    fixture.write(QA_HOOK, "---\nstart: claude\n---\nSay {title}.\n");
    crate::settings::store(&crate::settings::Settings {
        harnesses: vec![crate::harnesses::Harness {
            name: "noter".to_string(),
            command: "printf '%s' {prompt} > {repo}/harness-fired.txt".to_string(),
        }],
        ..Default::default()
    })
    .expect("the settings could not be written");
    let render = std::env::var("MOON_HOOKS_SHOTS").is_ok();
    let HookedBoard { mut harness, seen } = hooked_board(&fixture.root, render);
    assert!(
        settle(&mut harness, || {
            let seen = seen.lock().expect("poisoned");
            seen.hooks_read && seen.pill.is_some_and(|pill| pill.is_positive())
        }),
        "the column's hook never showed as a pill"
    );
    harness.run_steps(3);

    let pill = seen.lock().expect("poisoned").pill.expect("drawn");
    click_at(&mut harness, pill.center());
    assert!(
        settle(&mut harness, || seen.lock().expect("poisoned").editor_open),
        "the pill did not open its editor"
    );
    harness.run_steps(3);
    harness.get_by_label("noter").click();
    harness.run_steps(3);
    leave_a_screenshot(&mut harness, "harness-hook-editor");
    harness.get_by_label("[save]").click();
    assert!(
        settle(&mut harness, || !seen.lock().expect("poisoned").editor_open),
        "saving did not put the editor away"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join(QA_HOOK)).unwrap(),
        "---\nstart: noter\n---\nSay {title}.\n"
    );

    // Long enough for the runner to have taken its first look, which only notes where cards are.
    let first_look = Instant::now() + Duration::from_secs(5);
    while Instant::now() < first_look {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let (task_id, title, _) = CARDS[1];
    let record = fixture
        .root
        .join(format!(".moontasks/{task_id}/metadata.json"));
    let moved = std::fs::read_to_string(&record)
        .unwrap()
        .replace("\"status\": \"todo\"", "\"status\": \"in_progress\"");
    std::fs::write(&record, moved).unwrap();

    let fired = fixture.root.join("harness-fired.txt");
    let said = || std::fs::read_to_string(&fired).unwrap_or_default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && said().is_empty() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let log = std::fs::read_to_string(fixture.root.join(".moontasks/hooks/hooks.log"))
        .unwrap_or_default();
    assert_eq!(said(), format!("Say {title}."), "log: {log}");

    // A card still in the first column offers the harness under its `[start]`.
    harness.run_steps(3);
    harness
        .get_all_by_label("[start]")
        .nth(1)
        .expect("a card's [start]")
        .click();
    harness.run_steps(4);
    leave_a_screenshot(&mut harness, "harness-start-menu");
    assert!(
        harness.query_by_label("noter").is_some(),
        "a card's [start] should offer the harness"
    );
}
