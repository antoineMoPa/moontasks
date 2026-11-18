//! The board's background scene: painted behind the columns, with the cards over it.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use egui_kittest::Harness;

use crate::{
    native::{background::Scene, theme::ThemeMode},
    settings::Settings,
};

use super::{app_for, seeded_fixture};

/// What the test asks the window to show: 0 is the plain board, then each scene in turn.
const PLAIN: u8 = 0;

fn scene_of(choice: u8) -> Option<Scene> {
    (choice != PLAIN).then(|| Scene::ALL[usize::from(choice) - 1])
}

/// The board with a scene behind it looks like the scene, and the plain board is what it was.
/// With `MOON_BACKGROUND_PNG_DIR` set the frames are written there, to be looked at.
#[test]
fn a_scene_is_painted_behind_the_columns_and_the_plain_board_is_untouched() {
    let fixture = seeded_fixture("board-background");
    for (id, title) in [
        ("write-the-parser-1111", "Write the parser"),
        ("fix-the-login-page-2222", "Fix the login page"),
        ("ship-it-3333", "Ship it"),
    ] {
        fixture.write(
            &format!(".moontasks/{id}/metadata.json"),
            &format!(
                "{{\n  \"title\": \"{title}\",\n  \"status\": \"todo\",\n  \
                 \"created_at_unix\": 1700000000,\n  \"resources\": []\n}}\n"
            ),
        );
    }

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let ready = Arc::new(AtomicBool::new(false));
    let ready_in_ui = Arc::clone(&ready);
    let choice = Arc::new(AtomicU8::new(PLAIN));
    let choice_in_ui = Arc::clone(&choice);
    let open_review = Arc::new(AtomicBool::new(false));
    let open_review_in_ui = Arc::clone(&open_review);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            let scene = scene_of(choice_in_ui.load(Ordering::Relaxed));
            if open_review_in_ui.swap(false, Ordering::Relaxed) {
                app.open_pane(crate::native::panes::OpenPaneRequest::Review {
                    session_id: app.model.root_session_id.clone(),
                    title: "review".to_string(),
                });
            }
            if let Some(project_path) = app.model.project_path.clone() {
                app.model
                    .settings
                    .get_or_insert_with(Settings::default)
                    .set_background(&project_path, scene);
            }
            app.draw(ui);
            ready_in_ui.store(
                app.model.board.loaded && app.model.board.tasks.len() == 3,
                Ordering::Relaxed,
            );
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
        "the board never read its tasks"
    );

    let shot = |harness: &mut Harness<'_>, name: &str| {
        harness.run_steps(3);
        let image = harness.render().expect("expected the frame to render");
        if let Ok(dir) = std::env::var("MOON_BACKGROUND_PNG_DIR") {
            std::fs::create_dir_all(&dir).expect("expected the directory");
            image
                .save(std::path::Path::new(&dir).join(format!("{name}.png")))
                .expect("expected the image to save");
        }
        image
    };

    let plain = shot(&mut harness, "plain");
    let mut scenes = Vec::new();
    for (index, scene) in Scene::ALL.into_iter().enumerate() {
        choice.store(index as u8 + 1, Ordering::Relaxed);
        scenes.push((
            scene,
            shot(&mut harness, &format!("{scene:?}").to_lowercase()),
        ));
    }

    // The review tab, with the moon behind it.
    {
        choice.store(1, Ordering::Relaxed);
        open_review.store(true, Ordering::Relaxed);
        let _review = shot(&mut harness, "moon-review");
    }

    // A point in the open pane, below the cards: the plain board is its flat ground there,
    // and each scene is something else.
    let probe = |image: &image::RgbaImage| *image.get_pixel(900, 700);
    let ground = probe(&plain);
    for (scene, image) in &scenes {
        let differing = image
            .pixels()
            .zip(plain.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            differing > image.pixels().len() / 4,
            "{scene:?} barely changed the board: {differing} pixels differ"
        );
    }
    assert_ne!(
        probe(&scenes[1].1),
        ground,
        "the trains' paper is not the board's ground"
    );
}

/// The project's settings page offers the scenes under the workspace color, and a pick is
/// that project's and nobody else's.
#[test]
fn the_project_pane_picks_the_scene_of_this_project_below_the_workspace_color() {
    use egui_kittest::kittest::Queryable as _;

    let fixture = seeded_fixture("board-background-picker");
    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let seen = Arc::new(std::sync::Mutex::new((None, None)));
    let seen_in_ui = Arc::clone(&seen);
    let mark: Arc<std::sync::Mutex<Option<crate::settings::WorkspaceColor>>> =
        Arc::new(std::sync::Mutex::new(None));
    let mark_in_ui = Arc::clone(&mark);
    let color_seen = Arc::new(std::sync::Mutex::new(
        crate::settings::WorkspaceColor::Plain,
    ));
    let color_seen_in_ui = Arc::clone(&color_seen);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(crate::native::panes::OpenPaneRequest::Project);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            if app.model.settings.is_none() {
                app.model.settings = Some(Settings::default());
            }
            if let Some(color) = mark_in_ui.lock().expect("mark").take() {
                app.pending_action =
                    Some(crate::native::palette::CommandAction::MarkWorkspace(color));
            }
            app.draw(ui);
            *color_seen_in_ui.lock().expect("color") = app.model.workspace_color;
            *seen_in_ui.lock().expect("seen") = (
                app.background_of_this_project(),
                app.model.project_path.clone(),
            );
        });

    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        harness.step();
        if harness.query_by_label("board background").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let workspace_color = harness
        .query_by_label("workspace color")
        .expect("the workspace color row")
        .rect();
    let board_background = harness
        .query_by_label("board background")
        .expect("the board background row")
        .rect();
    assert!(
        board_background.top() > workspace_color.top(),
        "the scenes are below the workspace color"
    );
    assert_eq!(
        seen.lock().expect("seen").0,
        None,
        "the plain board to begin with"
    );

    harness.run_steps(2);
    let plain = harness.render().expect("expected the page to render");
    harness.get_by_label("trains").click();
    harness.run_steps(3);
    assert_eq!(seen.lock().expect("seen").0, Some(Scene::Trains));
    let with_scene = harness.render().expect("expected the page to render");
    if let Ok(dir) = std::env::var("MOON_BACKGROUND_PNG_DIR") {
        with_scene
            .save(std::path::Path::new(&dir).join("moon-project-pane.png"))
            .expect("expected the image to save");
    }
    let differing = plain
        .pixels()
        .zip(with_scene.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > plain.pixels().len() / 4,
        "the scene should show behind the project's page: {differing} pixels differ"
    );
    let (_, project_path) = seen.lock().expect("seen").clone();
    assert!(project_path.is_some(), "the pick is kept against a project");

    // A workspace color and a scene exclude each other: marking the window takes the scene
    // off, and picking a scene takes the color off.
    *mark.lock().expect("mark") = Some(crate::settings::WorkspaceColor::Teal);
    harness.run_steps(4);
    assert_eq!(
        seen.lock().expect("seen").0,
        None,
        "a color takes the scene off"
    );
    assert_eq!(
        *color_seen.lock().expect("color"),
        crate::settings::WorkspaceColor::Teal
    );
    harness.get_by_label("moon").click();
    harness.run_steps(4);
    assert_eq!(seen.lock().expect("seen").0, Some(Scene::Moon));
    assert_eq!(
        *color_seen.lock().expect("color"),
        crate::settings::WorkspaceColor::Plain,
        "a scene takes the color off"
    );

    harness.get_by_label("none").click();
    harness.run_steps(3);
    assert_eq!(
        seen.lock().expect("seen").0,
        None,
        "none is the plain board again"
    );
}

/// Every kind of pane lets the scene through its ground - none of them is a solid sheet over
/// it. With `MOON_BACKGROUND_PNG_DIR` set each pane's frame is written there, to be looked at.
#[test]
fn the_panes_of_a_project_with_a_scene_let_it_show_through() {
    use crate::native::panes::OpenPaneRequest as Open;

    let fixture = seeded_fixture("board-background-panes");
    fixture.write(
        ".moontasks/write-the-parser-1111/metadata.json",
        "{\n  \"title\": \"Write the parser\",\n  \"status\": \"todo\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.set_theme(ThemeMode::Dark);
    let request: Arc<std::sync::Mutex<Option<usize>>> = Arc::new(std::sync::Mutex::new(None));
    let request_in_ui = Arc::clone(&request);
    let ready = Arc::new(AtomicBool::new(false));
    let ready_in_ui = Arc::clone(&ready);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                app.open_pane(Open::Tasks);
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            if let Some(project_path) = app.model.project_path.clone() {
                app.model
                    .settings
                    .get_or_insert_with(Settings::default)
                    .set_background(&project_path, Some(Scene::Moon));
            }
            if let Some(which) = request_in_ui.lock().expect("request").take() {
                let session_id = app.model.root_session_id.clone();
                let opens = [
                    Open::Reviews,
                    Open::Project,
                    Open::Settings,
                    Open::Messages,
                    Open::Submodules,
                    Open::Commit {
                        session_id: session_id.clone(),
                    },
                    Open::File {
                        session_id: session_id.clone(),
                        file_path: "src/lib.rs".to_string(),
                        at: None,
                    },
                    Open::TaskStart {
                        task_id: "write-the-parser-1111".to_string(),
                        title: "Write the parser".to_string(),
                    },
                    Open::Terminal { command: None },
                ];
                app.open_pane(opens[which].clone());
            }
            app.draw(ui);
            ready_in_ui.store(
                app.model.board.loaded && app.model.board.tasks.len() == 1,
                Ordering::Relaxed,
            );
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
        "the board never read its task"
    );

    for (which, name) in [
        "reviews",
        "project",
        "settings",
        "messages",
        "submodules",
        "commit",
        "file",
        "start",
        "shell",
    ]
    .into_iter()
    .enumerate()
    {
        *request.lock().expect("request") = Some(which);
        harness.run_steps(4);
        let image = harness.render().expect("expected the frame to render");
        if let Ok(dir) = std::env::var("MOON_BACKGROUND_PNG_DIR") {
            image
                .save(std::path::Path::new(&dir).join(format!("pane-{name}.png")))
                .expect("expected the image to save");
        }
    }
}

/// A project's scene is still there after the window is closed and opened again: it is read
/// back from the settings file against the project's path.
#[test]
fn a_projects_scene_is_still_there_after_a_restart() {
    let fixture = seeded_fixture("board-background-restart");

    // The first window: picks the trains for the project, which writes the file.
    let mut first = app_for(&fixture.root, ThemeMode::Dark);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut harness = {
        let picked = Arc::new(AtomicBool::new(false));
        let picked_in_ui = Arc::clone(&picked);
        let stored_in_ui = Arc::new(std::sync::Mutex::new(false));
        let mut harness = Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .with_theme(egui::Theme::Dark)
            .wgpu()
            .build_ui(move |ui| {
                first.draw(ui);
                if first.model.project_path.is_some()
                    && first.model.settings.is_some()
                    && !picked_in_ui.load(Ordering::Relaxed)
                {
                    first.pick_background(Some(Scene::Trains));
                    picked_in_ui.store(true, Ordering::Relaxed);
                }
                *stored_in_ui.lock().expect("stored") = picked_in_ui.load(Ordering::Relaxed);
            });
        while Instant::now() < deadline && !picked.load(Ordering::Relaxed) {
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            picked.load(Ordering::Relaxed),
            "the first window never picked"
        );
        harness
    };
    // The write is made on a worker thread.
    for _ in 0..50 {
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(harness);

    // The second window: nothing is told, it has to read the file.
    let mut second = app_for(&fixture.root, ThemeMode::Dark);
    let seen = Arc::new(std::sync::Mutex::new(None));
    let seen_in_ui = Arc::clone(&seen);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            second.draw(ui);
            *seen_in_ui.lock().expect("seen") = second.background_of_this_project();
        });
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && seen.lock().expect("seen").is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        *seen.lock().expect("seen"),
        Some(Scene::Trains),
        "the scene was not there after a restart"
    );
}
