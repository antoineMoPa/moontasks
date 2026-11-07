//! The tree of files beside a file tab, and back / forward through the files shown.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicIsize, Ordering},
};

use egui_kittest::Harness;

use crate::native::{panes::Pane, theme::ThemeMode};

use super::{Fixture, app_for, settle};

#[derive(Default, Clone)]
struct Seen {
    front: Option<String>,
    open: Vec<String>,
    listed: Vec<String>,
    root: Vec<(String, bool)>,
    history: Vec<String>,
    history_at: usize,
    tabs: usize,
}

/// Opening a file unfolds the tree down to it and lists what git ignores as ignored; a
/// second file goes after it in the history, and back and forward move between the two - the
/// tab of the first being closed in between does not matter.
#[test]
fn the_tree_follows_the_file_and_back_and_forward_move_along_the_history() {
    let fixture = Fixture::new("file-tree");
    fixture.write(".gitignore", "/target\n");
    fixture.write("src/native/theme.rs", "pub fn theme() {}\n");
    fixture.write("src/lib.rs", "pub fn lib() {}\n");
    fixture.write("target/debug/out", "x\n");
    fixture.write("README.md", "# read me\n");
    fixture.commit("Add the files");

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let seen = Arc::new(Mutex::new(Seen::default()));
    let seen_in_ui = Arc::clone(&seen);
    let to_open = Arc::new(Mutex::new(vec!["src/native/theme.rs".to_string()]));
    let to_open_in_ui = Arc::clone(&to_open);
    let step = Arc::new(AtomicIsize::new(0));
    let step_in_ui = Arc::clone(&step);
    let close_front = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let close_front_in_ui = Arc::clone(&close_front);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 760.0))
        .wgpu()
        .build_ui(move |ui| {
            if matches!(app.model.stage, crate::native::model::Stage::Ready) {
                let session_id = app.model.root_session_id.clone();
                let mut waiting = to_open_in_ui.lock().expect("poisoned");
                if !waiting.is_empty() && app.pending_action.is_none() {
                    let path = waiting.remove(0);
                    app.open_file_pane_at(&session_id, &path, None);
                }
            }
            match step_in_ui.swap(0, Ordering::Relaxed) {
                0 => {}
                by => app.step_through_files(by),
            }
            if close_front_in_ui.swap(false, Ordering::Relaxed)
                && let Some(pane_id) = app.active_pane_id()
            {
                app.close_pane(pane_id);
            }
            app.draw(ui);

            let session_id = app.model.root_session_id.clone();
            let (open, listed) = app.model.file_tree.probe_for_test(&session_id);
            *seen_in_ui.lock().expect("poisoned") = Seen {
                front: match app.active_pane() {
                    Some((_, Pane::File { file_path, .. })) => Some(file_path.clone()),
                    _ => None,
                },
                open,
                listed,
                root: app.model.file_tree.entries_for_test(&session_id, ""),
                history: app
                    .model
                    .file_tree
                    .history
                    .entries()
                    .iter()
                    .map(|shown| shown.file_path.clone())
                    .collect(),
                history_at: app.model.file_tree.history.at(),
                tabs: app
                    .model
                    .layout
                    .panes()
                    .filter(|(_, pane)| matches!(pane, Pane::File { .. }))
                    .count(),
            };
        });
    let seen_now = || seen.lock().expect("poisoned").clone();

    // The first file is open, its folders unfolded and listed, and the root says `target` is
    // what git ignores.
    assert!(
        settle(&mut harness, || {
            let seen = seen_now();
            seen.front.as_deref() == Some("src/native/theme.rs")
                && seen.listed.contains(&"src/native".to_string())
        }),
        "the tree never listed down to the file: front {:?}, open {:?}, listed {:?}",
        seen_now().front,
        seen_now().open,
        seen_now().listed
    );
    let seen = seen_now();
    assert_eq!(seen.open, vec!["src", "src/native"]);
    assert!(seen.listed.contains(&String::new()));
    assert!(
        seen.root.contains(&("target".to_string(), true)),
        "{:?}",
        seen.root
    );
    assert!(seen.root.contains(&("src".to_string(), false)));
    assert!(!seen.root.iter().any(|(name, _)| name == ".git"));

    // A second file goes after it.
    to_open
        .lock()
        .expect("poisoned")
        .push("src/lib.rs".to_string());
    assert!(
        settle(&mut harness, || seen_now().front.as_deref()
            == Some("src/lib.rs")),
        "the second file never came forward"
    );
    // The history hears of a file the frame after its tab comes forward.
    harness.run_steps(2);
    assert_eq!(
        seen_now().history,
        vec!["src/native/theme.rs", "src/lib.rs"]
    );

    // Back to the first.
    step.store(-1, Ordering::Relaxed);
    assert!(
        settle(&mut harness, || {
            seen_now().front.as_deref() == Some("src/native/theme.rs")
        }),
        "back never showed the first file"
    );
    harness.run_steps(3);
    let seen = seen_now();
    assert_eq!(seen.history_at, 0);
    assert_eq!(seen.history.len(), 2, "going back adds nothing");

    // Close that tab, and forward still reaches the second, which is open; then back
    // reopens the one that was closed.
    close_front.store(true, Ordering::Relaxed);
    harness.run_steps(3);
    step.store(1, Ordering::Relaxed);
    assert!(
        settle(&mut harness, || seen_now().front.as_deref()
            == Some("src/lib.rs")),
        "forward never showed the second file"
    );
    step.store(-1, Ordering::Relaxed);
    assert!(
        settle(&mut harness, || {
            seen_now().front.as_deref() == Some("src/native/theme.rs")
        }),
        "back never reopened the closed file"
    );
    assert_eq!(seen_now().tabs, 2);
}
