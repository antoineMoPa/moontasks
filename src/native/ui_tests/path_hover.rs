//! A file's path, wherever the window writes one, says where the file is on disk when the
//! pointer rests on it: a row has room for a name or a path inside the repo, and which file
//! that is - to paste in a shell, or to tell two repos' `README.md` apart - is its hover.
//!
//! Asked of the two a file tab shows at once, which between them go through both ways a
//! hover is given: the path on the tab's own header, a label, and the file's row in the tree
//! beside it, which is painted.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable};

use crate::native::theme::ThemeMode;

use super::{Fixture, app_for, settle};

/// Rest the pointer at `at` until a tooltip saying `said` is up.
fn rest_the_pointer_until_told(harness: &mut Harness<'_>, at: egui::Pos2, said: &str) -> bool {
    // Off whatever it was on first, so the tooltip that comes up is this place's.
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(600.0, 500.0)));
    harness.run_steps(2);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    let deadline = Instant::now() + Duration::from_millis(1500);
    settle(harness, || Instant::now() >= deadline);
    harness.query_by_label(said).is_some()
}

fn save_screenshot(harness: &mut Harness<'_>, name: &str) {
    if let Ok(dir) = std::env::var("MOON_SCREENSHOT_DIR") {
        let image = harness.render().expect("the window could not be rendered");
        image
            .save(std::path::Path::new(&dir).join(format!("{name}.png")))
            .expect("the screenshot could not be saved");
    }
}

#[test]
fn a_path_says_where_its_file_is_on_disk_when_pointed_at() {
    let fixture = Fixture::new("path-hover");
    fixture.write("src/native/theme.rs", "pub fn theme() {}\n");
    fixture.commit("Add the file");
    // As the repo names itself, which on a Mac is not how its temporary folder was asked for.
    let root = fixture
        .root
        .canonicalize()
        .expect("the fixture's repo is on disk");
    let absolute = root.join("src/native/theme.rs").display().to_string();
    let absolute_folder = root.join("src/native").display().to_string();

    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let opened = Arc::new(AtomicBool::new(false));
    let opened_in_ui = Arc::clone(&opened);
    let listed = Arc::new(Mutex::new(Vec::<String>::new()));
    let listed_in_ui = Arc::clone(&listed);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 760.0))
        .wgpu()
        .build_ui(move |ui| {
            if !opened_in_ui.load(Ordering::Relaxed)
                && matches!(app.model.stage, crate::native::model::Stage::Ready)
            {
                let session_id = app.model.root_session_id.clone();
                app.open_file_pane(&session_id, "src/native/theme.rs");
                opened_in_ui.store(true, Ordering::Relaxed);
            }
            app.draw(ui);
            let session_id = app.model.root_session_id.clone();
            *listed_in_ui.lock().expect("poisoned") =
                app.model.file_tree.probe_for_test(&session_id).1;
        });

    assert!(
        settle(&mut harness, || {
            listed
                .lock()
                .expect("poisoned")
                .contains(&"src/native".to_string())
        }),
        "the tree never listed down to the file"
    );
    harness.run_steps(3);
    assert!(
        harness.query_by_label(&absolute).is_none(),
        "the path from the disk's root is written before anything is pointed at"
    );

    // The path on the tab's header, which reads from the repo.
    let header = harness.get_by_label("src/native/theme.rs").rect().center();
    assert!(
        rest_the_pointer_until_told(&mut harness, header, &absolute),
        "the header's path never said {absolute}"
    );
    save_screenshot(&mut harness, "path-hover-file-header");

    // The file's row in the tree, which reads as its name alone, and the folder's above it.
    // Painted rows, so they are pointed at where the tree puts them: the third row and the
    // second, under the filter box.
    assert!(
        rest_the_pointer_until_told(&mut harness, egui::pos2(80.0, 146.0), &absolute),
        "the tree's row never said {absolute}"
    );
    save_screenshot(&mut harness, "path-hover-tree-row");
    assert!(
        rest_the_pointer_until_told(&mut harness, egui::pos2(60.0, 124.0), &absolute_folder),
        "the tree's folder never said {absolute_folder}"
    );
}

/// The same of a review: the folder over the names in its sidebar, and the file's name over
/// its diff.
#[test]
fn a_review_says_where_its_folders_and_files_are_on_disk_when_pointed_at() {
    let fixture = super::seeded_fixture("path-hover-review");
    let root = fixture
        .root
        .canonicalize()
        .expect("the fixture's repo is on disk");
    let folder = root.join("src").display().to_string();
    let file = root.join("src/lib.rs").display().to_string();

    let app = app_for(&fixture.root, ThemeMode::Dark);
    let mut harness = super::harness_with_loaded_review(app, ThemeMode::Dark);

    let folder_heading = harness.get_by_label("src").rect().center();
    assert!(
        rest_the_pointer_until_told(&mut harness, folder_heading, &folder),
        "the sidebar's folder never said {folder}"
    );
    save_screenshot(&mut harness, "path-hover-review-folder");

    let file_heading = harness.get_by_label("src/lib.rs").rect().center();
    assert!(
        rest_the_pointer_until_told(&mut harness, file_heading, &file),
        "the diff's file name never said {file}"
    );
    save_screenshot(&mut harness, "path-hover-review-diff-file");
}
