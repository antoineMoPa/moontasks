//! The settings pane's harnesses: the person's own things to start on a task, each a name and
//! the command it runs - see [`crate::harnesses`].
//!
//! A row is two boxes and a mark that takes it away. What is typed is kept in the row until
//! the keyboard leaves its box, and saved then - one [`crate::settings::SettingsChange`] - if
//! it makes a harness; a row that does not says why under itself and is saved once it does.

use egui::{RichText, TextEdit, Ui};

use crate::{
    harnesses::{HARNESS_PLACEHOLDERS, Harness, check_harness},
    native::{
        app::App,
        theme::{Palette, SMALL_SIZE},
        widgets,
    },
};

const NAME_WIDTH: f32 = 110.0;
const COMMAND_WIDTH: f32 = 340.0;

/// One harness as it is being typed.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct HarnessRow {
    /// The name it is saved under, `None` for a row not saved yet.
    saved_as: Option<String>,
    name: String,
    command: String,
    /// Why what is typed was not saved.
    error: Option<String>,
    /// Whether its name is to take the keyboard on the next frame, as a new row's does.
    focus_name: bool,
}

impl HarnessRow {
    fn of_saved(harness: &Harness) -> Self {
        Self {
            saved_as: Some(harness.name.clone()),
            name: harness.name.clone(),
            command: harness.command.clone(),
            error: None,
            focus_name: false,
        }
    }

    fn typed(&self) -> Harness {
        Harness {
            name: self.name.trim().to_string(),
            command: self.command.trim().to_string(),
        }
    }

    /// Whether the row says something the settings do not: typed and not saved yet.
    fn differs_from(&self, saved: &[Harness]) -> bool {
        let Some(saved_as) = &self.saved_as else {
            return true;
        };
        !saved
            .iter()
            .any(|harness| harness.name == *saved_as && *harness == self.typed())
    }
}

/// The rows to draw: the ones being typed, while any of them says something the settings do
/// not, and otherwise the settings' own - so a harness saved from another window shows up.
fn rows_to_draw(app: &mut App) -> Vec<HarnessRow> {
    let saved = crate::native::board::harnesses_of(app).to_vec();
    let rows = std::mem::take(&mut app.model.settings_page.harness_rows);
    let in_step = rows.len() == saved.len();
    if in_step && rows.iter().any(|row| row.differs_from(&saved)) || rows.len() > saved.len() {
        return rows;
    }
    saved.iter().map(HarnessRow::of_saved).collect()
}

pub(super) fn draw(app: &mut App, ui: &mut Ui, palette: &Palette) {
    ui.label(
        RichText::new("Harnesses")
            .size(SMALL_SIZE)
            .color(palette.muted),
    );
    widgets::divider(ui, palette);
    ui.add_space(8.0);

    let mut rows = rows_to_draw(app);
    let mut removed = None;
    for (index, row) in rows.iter_mut().enumerate() {
        let mut left = false;
        ui.horizontal(|ui| {
            let name = ui.add(
                TextEdit::singleline(&mut row.name)
                    .hint_text("name")
                    .desired_width(NAME_WIDTH),
            );
            if row.focus_name {
                name.request_focus();
                row.focus_name = false;
            }
            let command = ui
                .add(
                    TextEdit::singleline(&mut row.command)
                        .hint_text("command")
                        .desired_width(COMMAND_WIDTH),
                )
                .on_hover_text(format!(
                    "{} are filled in as it starts",
                    HARNESS_PLACEHOLDERS.join(" ")
                ));
            left = name.lost_focus() || command.lost_focus();
            if widgets::close_mark(ui, palette, true)
                .on_hover_text("Take this harness off the list")
                .clicked()
            {
                removed = Some(index);
            }
        });
        if left {
            save_typed_row(app, row);
        }
        if let Some(error) = &row.error {
            ui.label(RichText::new(error).size(SMALL_SIZE).color(palette.warn));
        }
    }
    if let Some(index) = removed {
        let row = rows.remove(index);
        if let Some(name) = row.saved_as {
            app.remove_harness(name);
        }
    }

    ui.add_space(4.0);
    if widgets::quiet_button_colored(ui, "[+ harness]", palette.accent)
        .on_hover_text("Add a name and the command it runs")
        .clicked()
    {
        rows.push(HarnessRow {
            focus_name: true,
            ..HarnessRow::default()
        });
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "a name and the command it runs - offered in [start], hooks and the palette's \
             quick task",
        )
        .size(SMALL_SIZE)
        .color(palette.muted),
    );
    app.model.settings_page.harness_rows = rows;
}

/// Save what a row holds as the keyboard leaves it, or say why it is not a harness. A row with
/// nothing typed in one of its boxes is still being written, and says nothing.
fn save_typed_row(app: &mut App, row: &mut HarnessRow) {
    let harness = row.typed();
    if harness.name.is_empty() || harness.command.is_empty() {
        row.error = None;
        return;
    }
    let name_taken = crate::native::board::harnesses_of(app)
        .iter()
        .any(|saved| saved.name == harness.name && row.saved_as.as_ref() != Some(&saved.name));
    row.error = match check_harness(&harness) {
        Err(error) => Some(error),
        Ok(()) if name_taken => Some(format!("another harness is called {}", harness.name)),
        Ok(()) => None,
    };
    if row.error.is_none() {
        app.save_harness(row.saved_as.clone(), harness.clone());
        row.saved_as = Some(harness.name);
    }
}
