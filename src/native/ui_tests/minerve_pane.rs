//! A Minerve shell on a window that reads this machine: moon's `__minerve --serve`, drawn with
//! minerve's own view, ending when its shell is ended.
//!
//! Runs the `moon` built beside the test binary (`cargo build --bin moon` first), as the person
//! running the tests: their minerve keys and MCP config are read, though nothing is sent.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable as _};

use crate::{
    api::AgentKind,
    native::{
        app::App,
        panes::{OpenPaneRequest, Pane},
        theme::ThemeMode,
    },
};

use super::{app_for, seeded_fixture};

/// What the test does next, run inside the frame since the harness owns the app.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Open,
    WaitForMinerve,
    Typing,
    EndShell,
    WaitForHangUp,
    Done,
}

fn minerve_shell(app: &App) -> Option<String> {
    app.model.layout.panes().find_map(|(_, pane)| match pane {
        Pane::Terminal {
            terminal_id,
            command: Some(AgentKind::Minerve),
            ..
        } => Some(terminal_id.clone()),
        _ => None,
    })
}

fn connected(app: &App, terminal_id: &str) -> bool {
    app.model
        .minerve_views
        .get(terminal_id)
        .is_some_and(minerve_gui::MinerveView::is_connected)
}

fn wait_until(harness: &mut Harness<'_>, step: &Mutex<Step>, wanted: Step, within: Duration) {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline && *step.lock().unwrap() != wanted {
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn draft(draft: &Mutex<String>) -> String {
    draft.lock().unwrap().clone()
}

fn save_picture(harness: &mut Harness<'_>) {
    if let Ok(path) = std::env::var("MOON_MINERVE_PANE_PNG") {
        let window = harness.render().expect("the window should render");
        window.save(path).expect("the picture should save");
    }
}

#[test]
fn a_minerve_shell_is_drawn_with_minerves_view_and_ends_with_its_shell() {
    let fixture = seeded_fixture("minerve-pane");
    // `MOON_MINERVE_PANE_LIGHT` draws the picture in the light theme.
    let theme = match std::env::var_os("MOON_MINERVE_PANE_LIGHT") {
        Some(_) => ThemeMode::Light,
        None => ThemeMode::Dark,
    };
    let mut app = app_for(&fixture.root, theme);
    let step = Arc::new(Mutex::new(Step::Open));
    let step_in_ui = Arc::clone(&step);
    // What is in minerve's box, read off each frame since the harness owns the app.
    let draft_in_ui = Arc::new(Mutex::new(String::new()));
    let draft_of_view = Arc::clone(&draft_in_ui);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .wgpu()
        .build_ui(move |ui| {
            let mut step = step_in_ui.lock().unwrap();
            match *step {
                Step::Open if app.model.root_repo_path().is_some() => {
                    app.open_pane(OpenPaneRequest::Terminal {
                        command: Some(AgentKind::Minerve),
                    });
                    *step = Step::WaitForMinerve;
                }
                Step::WaitForMinerve => {
                    if let Some(terminal_id) = minerve_shell(&app)
                        && connected(&app, &terminal_id)
                    {
                        *step = Step::Typing;
                    }
                }
                Step::EndShell => {
                    let terminal_id = minerve_shell(&app).expect("the Minerve shell is open");
                    let session_id = app.model.root_session_id.clone();
                    app.backend()
                        .close_terminal(&session_id, &terminal_id)
                        .expect("the shell should end");
                    *step = Step::WaitForHangUp;
                }
                // The ended shell's tab goes, or shows minerve gone.
                Step::WaitForHangUp => {
                    if minerve_shell(&app).is_none_or(|terminal_id| !connected(&app, &terminal_id))
                    {
                        *step = Step::Done;
                    }
                }
                _ => {}
            }
            drop(step);
            app.draw(ui);
            if let Some(terminal_id) = minerve_shell(&app)
                && let Some(view) = app.model.minerve_views.get(&terminal_id)
            {
                *draft_of_view.lock().unwrap() = view.draft().to_string();
            }
        });

    wait_until(&mut harness, &step, Step::Typing, Duration::from_secs(40));
    if *step.lock().unwrap() != Step::Typing {
        save_picture(&mut harness);
        panic!("minerve never came up in its shell's tab");
    }
    // Typed into minerve's own box, not the shell's pty: a click on the box gives it the
    // keyboard, which an embedded view does not take by itself.
    harness.run_steps(3);
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .click();
    harness.run_steps(2);
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("/mc".to_string()));
    harness.run_steps(3);
    assert_eq!(
        draft(&draft_in_ui),
        "/mc",
        "typing should reach minerve's box"
    );
    // The slash menu's rows are buttons: a click puts the command in the box.
    harness.get_by_label_contains("/mcp").click();
    harness.run_steps(3);
    assert_eq!(
        draft(&draft_in_ui),
        "/mcp",
        "clicking a suggestion should pick it"
    );
    save_picture(&mut harness);

    *step.lock().unwrap() = Step::EndShell;
    wait_until(&mut harness, &step, Step::Done, Duration::from_secs(20));
    assert_eq!(
        *step.lock().unwrap(),
        Step::Done,
        "the view should hear minerve go when its shell is ended"
    );
}

/// What the quick task's Minerve was told, if it has been: the first thing a person said in
/// its conversation.
fn prompt_the_quick_task_minerve_got(app: &App, title: &str) -> Option<String> {
    let session_id = app.model.root_session_id.clone();
    let tasks = app.backend().list_tasks(&session_id).ok()?;
    let terminal_id = tasks
        .iter()
        .find(|task| task.title == title)?
        .resources
        .iter()
        .find_map(|resource| resource.terminal_id.clone())?;
    let conversation = app
        .backend()
        .terminal_conversation(&session_id, &terminal_id)
        .ok()?;
    conversation
        .messages
        .iter()
        .find(|message| message.role == crate::agent_conversation::AgentMessageRole::User)
        .and_then(|message| message.content.clone())
}

/// A quick task started with Minerve from the palette tells it the title, which starts it on
/// the work - over its control socket, since a Minerve drawn in a window has no box on its pty.
///
/// Sends a prompt with the keys of the person running the tests, so it only runs when asked
/// to: `MOON_MINERVE_SENDS_A_PROMPT=1`.
#[test]
fn a_quick_task_started_with_minerve_is_told_its_title() {
    if std::env::var_os("MOON_MINERVE_SENDS_A_PROMPT").is_none() {
        return;
    }
    const TITLE: &str = "reply with the single word pong and do nothing else";
    let fixture = seeded_fixture("minerve-quick-task");
    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let started = Arc::new(Mutex::new(false));
    let started_in_ui = Arc::clone(&started);
    let got = Arc::new(Mutex::new(None));
    let got_in_ui = Arc::clone(&got);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .wgpu()
        .build_ui(move |ui| {
            let mut started = started_in_ui.lock().unwrap();
            if !*started && app.model.root_repo_path().is_some() {
                crate::native::board::quick_task::start(
                    &mut app,
                    TITLE.to_string(),
                    crate::native::board::quick_task::QuickStart::Agent(AgentKind::Minerve),
                );
                *started = true;
            }
            drop(started);
            app.draw(ui);
            *got_in_ui.lock().unwrap() = prompt_the_quick_task_minerve_got(&app, TITLE);
        });

    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline && got.lock().unwrap().is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let got = got.lock().unwrap().clone();
    assert!(
        got.as_deref().is_some_and(|prompt| prompt.contains(TITLE)),
        "minerve should have been told the card's title, and got {got:?}"
    );
}
