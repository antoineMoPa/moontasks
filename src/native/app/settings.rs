//! The window's side of `settings.json`: read from the server once, and changed one
//! [`SettingsChange`] at a time through it - see [`crate::settings`] for why the file is the
//! server's rather than the window's.

use crate::{
    native::{theme::ThemeMode, workspace_color::WorkspaceColor},
    settings::SettingsChange,
};

use super::App;

impl App {
    /// Ask the server for its settings. Until they come the launch screen has no recent
    /// projects to offer and nothing is written back, since there is nothing yet to compare
    /// a change against.
    pub(super) fn load_settings(&mut self) {
        self.tasks.spawn(
            |backend| backend.settings(),
            |model, result| match result {
                Ok(settings) => {
                    // The agent the person last picked, put back once the review says this
                    // machine still has it - see `App::apply_restored_agent`.
                    model.restored_agent = Some(settings.selected_agent);
                    model.settings = Some(settings);
                }
                Err(error) => model.error(format!("could not read the settings: {error}")),
            },
        );
    }

    /// Make one change to the settings: to the window's copy at once, and to the server's file
    /// behind it. A change to what the copy already says is not sent.
    pub(super) fn change_settings(&mut self, change: SettingsChange) {
        let Some(settings) = &mut self.model.settings else {
            return;
        };
        if !settings.apply(change.clone()) {
            return;
        }
        self.tasks.spawn(
            move |backend| backend.change_settings(change.clone()),
            |model, result| {
                if let Err(error) = result {
                    model.error(format!("could not save the settings: {error}"));
                }
            },
        );
    }

    /// The scene the board of the project this window is on is painted over, `None` for the
    /// plain board - and until there is a project and the settings have been read.
    pub(crate) fn background_of_this_project(&self) -> Option<crate::settings::BackgroundScene> {
        let project_path = self.model.project_path.as_deref()?;
        self.model.settings.as_ref()?.background(project_path)
    }

    /// Pick the scene for this project's board, or `None` for the plain board: shown now, and
    /// written to the settings file.
    pub(crate) fn pick_background(&mut self, scene: Option<crate::settings::BackgroundScene>) {
        let Some(project_path) = self.model.project_path.clone() else {
            return;
        };
        // The two exclude each other - see [`Self::set_workspace_color`].
        if scene.is_some() && self.model.workspace_color != WorkspaceColor::Plain {
            self.set_workspace_color(WorkspaceColor::Plain);
        }
        self.change_settings(SettingsChange::SetBackground {
            project_path,
            scene,
        });
    }

    /// Keep a harness of the person's - see [`crate::harnesses`] - over the one called `was`,
    /// or as a new one.
    pub(crate) fn save_harness(&mut self, was: Option<String>, harness: crate::harnesses::Harness) {
        self.change_settings(SettingsChange::SaveHarness { was, harness });
    }

    /// Take the harness of this name off the person's list.
    pub(crate) fn remove_harness(&mut self, name: String) {
        self.change_settings(SettingsChange::RemoveHarness(name));
    }

    /// Pick the model `agent` runs with, or `None` for its own default - see
    /// [`crate::settings::Settings::agent_models`].
    pub(crate) fn select_agent_model(
        &mut self,
        agent: crate::api::AgentKind,
        model: Option<String>,
    ) {
        self.change_settings(SettingsChange::SelectAgentModel { agent, model });
    }

    /// Offer the next quick task with the agent this one was started with - see
    /// [`crate::settings::Settings::quick_task_agent`].
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn remember_quick_task_agent(&mut self, agent: crate::api::AgentKind) {
        self.change_settings(SettingsChange::SelectQuickTaskAgent(agent));
    }

    /// Switch the window light or dark, and remember it in `settings.json` so the next launch
    /// opens the same way rather than on the system's theme.
    pub(crate) fn pick_theme(&mut self, theme: ThemeMode) {
        self.set_theme(theme);
        self.change_settings(SettingsChange::PickTheme(theme.into()));
    }

    /// Put back the theme last picked. The settings arrive a moment after the window does,
    /// so this runs each frame and does nothing until they are there - or while nothing has
    /// been picked yet, when the window stays on the system's theme.
    pub(super) fn follow_saved_theme(&mut self) {
        let Some(saved) = self
            .model
            .settings
            .as_ref()
            .and_then(|settings| settings.theme)
        else {
            return;
        };
        let theme = ThemeMode::from(saved);
        if theme != self.model.theme {
            self.set_theme(theme);
        }
    }

    /// Mark this window's project with a color, and keep `settings.json` in step.
    ///
    /// A window that is on no project yet - the launch screen - has nothing to mark: the
    /// color is remembered against the project's path, so there is nowhere to put it.
    pub(crate) fn set_workspace_color(&mut self, color: WorkspaceColor) {
        // A color and a scene are two ways of dressing the same window: a color picked takes a
        // scene off, as a scene picked takes the color off.
        if color != WorkspaceColor::Plain && self.background_of_this_project().is_some() {
            self.pick_background(None);
        }
        self.model.workspace_color = color;
        // The ground is baked into the style, so the whole window has to be restyled.
        self.needs_style = true;

        let Some(project_path) = self.model.project_path.clone() else {
            return;
        };
        self.change_settings(SettingsChange::MarkWorkspace {
            project_path,
            color,
        });
    }

    /// Paint the window in the color its project was last marked with. The project and the
    /// settings each arrive a moment after the window does - both are a round trip - so this
    /// runs each frame and does nothing until both are there.
    pub(super) fn follow_project_color(&mut self) {
        let (Some(project_path), Some(settings)) =
            (self.model.project_path.as_deref(), &self.model.settings)
        else {
            return;
        };
        let color = settings.workspace_color(project_path);
        if color == self.model.workspace_color {
            return;
        }
        self.model.workspace_color = color;
        self.needs_style = true;
    }

    /// Keep `settings.json` in step with the selector at the top of the review.
    ///
    /// Written when the choice changes rather than on a clock: the file is one line, and a
    /// selector nobody has touched should leave it exactly as the user last left it - or as
    /// they last edited it by hand.
    pub(crate) fn remember_selected_agent(&mut self) {
        // Before the restored agent has been put back, the session still reads as `None`, and
        // writing that would throw away the very choice being restored. Before the settings
        // have come, there is no restored agent yet to wait for.
        if self.model.settings.is_none() || self.model.restored_agent.is_some() {
            return;
        }
        let selected = self.selected_agent();
        self.change_settings(SettingsChange::SelectAgent(selected));
    }

    /// Write a project that has just opened to the head of the recent list, so the next launch
    /// screen offers it. Held until the settings have come, so the change is made to them.
    pub(super) fn remember_opened_project(&mut self) {
        if self.model.settings.is_none() {
            return;
        }
        let Some(path) = self.model.opened_project.take() else {
            return;
        };
        self.change_settings(SettingsChange::RememberProject(path));
    }
}
