//! The settings page: the model dropdown, narrowed by what is typed in its filter.

use egui_kittest::{Harness, kittest::Queryable};

use crate::{
    api::AgentKind,
    native::{settings_pane::draw_model_picker, theme::ThemeMode},
    settings::Settings,
};

use super::{app_for, seeded_fixture};

/// The dropdown drawn on its own, set to `opencode`, with models handed to it rather than
/// asked of an `opencode` the machine running the tests may not have.
#[test]
fn typing_in_the_model_filter_narrows_the_models_listed() {
    let fixture = seeded_fixture("settings-pane");
    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.model.settings = Some(Settings::default());
    app.model.settings_page.opencode_models = Some(Ok(vec![
        "openai/gpt-6-sol".to_string(),
        "opencode/big-pickle".to_string(),
    ]));
    let palette = app.palette_of();

    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 500.0))
        .build_ui(move |ui| {
            draw_model_picker(&mut app, ui, &palette, AgentKind::OpenCode);
        });
    harness.run_steps(2);

    // Opening the dropdown lists every model, and gives its filter the keyboard.
    harness.get_by_value("OpenCode's default").click();
    harness.run_steps(3);
    assert!(
        harness.query_by_label("openai/gpt-6-sol").is_some(),
        "every model should be listed before anything is typed"
    );

    harness
        .input_mut()
        .events
        .push(egui::Event::Text("PICK".to_string()));
    harness.run_steps(3);

    assert!(harness.query_by_label("opencode/big-pickle").is_some());
    assert!(
        harness.query_by_label("openai/gpt-6-sol").is_none(),
        "a model that does not match the filter should be left out"
    );
}

/// `[+ harness]` adds a row, and what is typed into it is kept in the settings once the
/// keyboard leaves it - unless it is no harness, which the row says instead.
#[test]
fn a_harness_typed_into_a_new_row_is_saved_as_the_keyboard_leaves_it() {
    use std::sync::{Arc, Mutex};

    let fixture = seeded_fixture("settings-harnesses");
    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    app.model.settings = Some(Settings {
        harnesses: vec![crate::harnesses::Harness {
            name: "aider".to_string(),
            command: "aider --message-file {brief_file}".to_string(),
        }],
        ..Default::default()
    });
    let saved = Arc::new(Mutex::new(Vec::new()));
    let saved_in_ui = Arc::clone(&saved);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 300.0))
        .build_ui(move |ui| {
            crate::native::settings_pane::draw(&mut app, ui);
            *saved_in_ui.lock().expect("poisoned") = app
                .model
                .settings
                .as_ref()
                .map(|settings| settings.harnesses.clone())
                .unwrap_or_default();
        });
    harness.run_steps(2);
    assert!(
        harness.query_all_by_value("aider").next().is_some(),
        "a saved harness should have a row"
    );

    let type_text = |harness: &mut Harness<'_>, text: &str| {
        harness
            .input_mut()
            .events
            .push(egui::Event::Text(text.to_string()));
        harness.run_steps(2);
    };
    let press_tab = |harness: &mut Harness<'_>| {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
    };

    // The new row's name has the keyboard. A name moon has is refused, in the row's words.
    harness.get_by_label("[+ harness]").click();
    harness.run_steps(3);
    type_text(&mut harness, "claude");
    press_tab(&mut harness);
    type_text(&mut harness, "claude --model opus");
    press_tab(&mut harness);
    assert!(
        harness
            .query_by_label_contains("already what moon calls one of its own")
            .is_some(),
        "a name of moon's own should be refused"
    );
    assert_eq!(saved.lock().expect("poisoned").len(), 1);

    // Called something else, it is saved.
    // The name's box is the first of the two that say `claude`.
    harness
        .get_all_by_value("claude")
        .next()
        .expect("the row's name")
        .focus();
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.run_steps(2);
    type_text(&mut harness, "opus-plan");
    press_tab(&mut harness);
    if let Ok(dir) = std::env::var("MOON_HOOKS_SHOTS") {
        harness
            .render()
            .expect("the pane could not be rendered")
            .save(std::path::Path::new(&dir).join("harness-settings.png"))
            .expect("the screenshot could not be saved");
    }
    assert_eq!(
        saved.lock().expect("poisoned").last(),
        Some(&crate::harnesses::Harness {
            name: "opus-plan".to_string(),
            command: "claude --model opus".to_string(),
        })
    );
}
