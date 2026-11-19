//! The light/dark switch outlives the window: it is written to `settings.json` and read back
//! by the next launch, rather than starting over on the system's theme each time.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use egui_kittest::Harness;

use crate::native::{palette::CommandAction, theme::ThemeMode};

use super::{app_for, seeded_fixture};

#[test]
fn a_picked_theme_is_what_the_next_launch_opens_on() {
    // Arrange: a previous run picked light, and the system is dark.
    crate::settings::store(&crate::settings::Settings {
        theme: Some(crate::settings::Theme::Light),
        ..Default::default()
    })
    .expect("expected the settings to be written");
    let fixture = seeded_fixture("theme-saved");
    let mut app = app_for(&fixture.root, ThemeMode::Dark);

    let settings_read = Arc::new(AtomicBool::new(false));
    let settings_read_in_ui = Arc::clone(&settings_read);
    let switch_theme = Arc::new(AtomicBool::new(false));
    let switch_theme_in_ui = Arc::clone(&switch_theme);
    let theme = Arc::new(Mutex::new(ThemeMode::Dark));
    let theme_in_ui = Arc::clone(&theme);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            if switch_theme_in_ui.swap(false, Ordering::Relaxed) {
                app.pending_action = Some(CommandAction::ToggleTheme);
            }
            app.draw(ui);
            settings_read_in_ui.store(app.model.settings.is_some(), Ordering::Relaxed);
            *theme_in_ui.lock().expect("the theme") = app.model.theme;
        });

    super::workspace_color::step_until(
        &mut harness,
        &settings_read,
        "the window should have read the settings",
    );
    harness.run_steps(2);
    assert_eq!(
        *theme.lock().expect("the theme"),
        ThemeMode::Light,
        "the window should open on the theme last picked, not the system's"
    );

    // Act: switch to dark, as ⌘J does.
    switch_theme.store(true, Ordering::Relaxed);
    harness.run_steps(3);

    assert_eq!(
        *theme.lock().expect("the theme"),
        ThemeMode::Dark,
        "the switch should stick rather than be pulled back to the saved theme"
    );
    assert_eq!(
        crate::settings::load().theme,
        Some(crate::settings::Theme::Dark),
        "the switch should have been written to the settings"
    );
}
