//! A Minerve shell drawn with minerve's own view ([`minerve_gui::MinerveView`]) instead of its
//! terminal.
//!
//! On a window that reads this machine, a Minerve shell - from the palette, or a task's run -
//! is started as `moon __minerve --serve` (see [`crate::terminal::for_this_window`]): moon
//! itself, so one binary ships both, and a shell like any agent's, so the card's `stop` and
//! `resume`, its attention and `inspect` work as they do for the terminal one. Its pty has
//! nothing on it; the view is fed minerve's lines through the shell's control socket - see
//! `agent_control` in [`crate::terminal`].
//!
//! A view lasts while its shell's tab is the one in front of its frame. A task's shell outlives
//! its tab, and the view put back on it is given the chat as it stands, without the boot clip.

use std::collections::{HashMap, HashSet};

use egui::{Color32, Ui};

use crate::native::{app::App, panes::Pane};

/// The views of the Minerve shells, by terminal id.
#[derive(Default)]
pub(crate) struct MinerveViews {
    views: HashMap<String, minerve_gui::MinerveView>,
    /// Shells that are not a Minerve started for a view - a terminal one, or one on another
    /// machine - drawn as terminals, and not asked again.
    terminals: HashSet<String>,
    /// Shells a view has been drawn for: one put back on them skips the boot clip.
    seen: HashSet<String>,
    /// Shells whose Minerve has answered a view: one that fails later shows that in its view.
    answered: HashSet<String>,
}

impl MinerveViews {
    #[cfg(test)]
    pub(crate) fn get(&self, terminal_id: &str) -> Option<&minerve_gui::MinerveView> {
        self.views.get(terminal_id)
    }
}

/// Draws the Minerve shell `terminal_id` with minerve's own view, and says whether it did:
/// `false` for one to be drawn as a terminal - not a Minerve started for a view, or one that
/// failed before it ever answered, whose terminal has what it said on the way out.
pub(crate) fn draw(app: &mut App, ui: &mut Ui, terminal_id: &str) -> bool {
    let views = &app.model.minerve_views;
    if views.terminals.contains(terminal_id) {
        return false;
    }
    if !views.views.contains_key(terminal_id) {
        let Some((link, opening_line)) = app.backend().minerve_link(terminal_id) else {
            app.model.minerve_views.terminals.insert(terminal_id.to_string());
            return false;
        };
        let options = minerve_gui::Options {
            skip_boot: !app.model.minerve_views.seen.insert(terminal_id.to_string()),
            prefill: opening_line,
            embedded: true,
            ..Default::default()
        };
        let view = minerve_gui::MinerveView::attached(ui.ctx(), link, options);
        app.model.minerve_views.views.insert(terminal_id.to_string(), view);
    }
    let colors = colors_of(&app.palette_of());
    let views = &mut app.model.minerve_views;
    let view = views.views.get_mut(terminal_id).expect("just made");
    if view.is_connected() {
        views.answered.insert(terminal_id.to_string());
    } else if view.has_failed() && !views.answered.contains(terminal_id) {
        views.views.remove(terminal_id);
        views.terminals.insert(terminal_id.to_string());
        return false;
    }
    view.set_colors(colors);
    view.show(ui);
    true
}

/// Minerve's view in moon's palette, light or dark, so its tab sits with the others.
fn colors_of(palette: &crate::native::theme::Palette) -> minerve_gui::Colors {
    let dark = palette.mode == crate::native::theme::ThemeMode::Dark;
    // Dimmer shades are pulled toward the panel less on a light one, where they fade faster.
    let fade = |t: f32| if dark { t } else { t * 0.5 };
    minerve_gui::Colors {
        dark,
        background: palette.panel,
        surface: palette.composer_bg,
        bar: palette.header_bg,
        hairline: palette.line,
        sep: palette.line,
        dim: lerp_color(palette.muted, palette.panel, fade(0.45)),
        dark_gray: lerp_color(palette.muted, palette.panel, fade(0.15)),
        gray: palette.muted,
        name: lerp_color(palette.ink, palette.muted, 0.5),
        prose: palette.ink,
        bright: palette.ink,
        code_background: palette.code_bg,
        selection: lerp_color(palette.accent_2, palette.panel, 0.7),
        accent: palette.accent,
        magenta: palette.warn,
        green: palette.accent_2,
        // The default amber is too pale on a light panel.
        yellow: match dark {
            true => minerve_gui::Colors::default().yellow,
            false => Color32::from_rgb(0x9a, 0x6a, 0x00),
        },
    }
}

/// `a` moved `t` of the way to `b`.
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(channel(a.r(), b.r()), channel(a.g(), b.g()), channel(a.b(), b.b()))
}

impl App {
    /// Lets go of the views no frame has in front: the shell goes on (a task's) or has ended
    /// with its tab, and a view put back is fed the chat as it stands. A view behind another
    /// tab would only pile up what minerve says. Before the workspace is drawn.
    pub(crate) fn let_go_of_hidden_minerve_views(&mut self) {
        let layout = &self.model.layout;
        let shown: HashSet<String> = layout
            .frame_ids()
            .into_iter()
            .filter_map(|frame| layout.frame(frame).and_then(egui_frames::Frame::active_pane))
            .filter_map(|pane| match layout.pane(pane) {
                Some(Pane::Terminal { terminal_id, .. }) => Some(terminal_id.clone()),
                _ => None,
            })
            .collect();
        self.model.minerve_views.views.retain(|terminal_id, _| shown.contains(terminal_id));
    }
}
