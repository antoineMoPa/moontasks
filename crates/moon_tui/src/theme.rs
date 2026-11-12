//! This module holds every color the TUI is drawn in - the frames' too, which it hands them -
//! so the screen keeps one look: dim chrome, the accent for focus and the main action, red
//! only for danger and failure.

use moon_tui_frames::FramesStyle;
use ratatui::style::Color;

/// Marks focus, the main action, and a lit button.
pub const ACCENT: Color = Color::Yellow;
/// Colors text drawn on the accent.
pub const ON_ACCENT: Color = Color::Black;
/// Colors a lit button that is already drawn on the accent.
pub const ACCENT_LIT: Color = Color::LightYellow;
/// Colors chrome: borders, ages, buttons at rest, outcomes.
pub const DIM: Color = Color::DarkGray;
/// Colors danger and failure, and nothing else.
pub const DANGER: Color = Color::Red;
/// Shades the picked task.
pub const PICKED: Color = Color::Indexed(237);
/// Shades what the pointer is over.
pub const HOVER: Color = Color::Indexed(236);
/// Shades the picked task under the pointer, a step past its own shade.
pub const HOVER_ON_PICKED: Color = Color::Indexed(239);
/// Grounds a tab strip, and a watched agent's line.
pub const STRIP: Color = Color::Indexed(235);
/// Marks an agent or a shell going here.
pub const GOING: Color = Color::Green;
/// Marks an agent another moon holds, and names of agents and shells.
pub const HANDLE: Color = Color::Cyan;
/// Marks a review waiting.
pub const REVIEW: Color = Color::Yellow;

/// Lists the colors a tag can be drawn in: which one is settled by the tag's own letters, so a
/// tag is the same color on every task, as it is in the window.
const TAG_COLORS: [Color; 6] = [
    Color::Magenta,
    Color::Cyan,
    Color::Blue,
    Color::Green,
    Color::Red,
    Color::LightYellow,
];

pub fn tag_color_of(tag: &str) -> Color {
    let sum: usize = tag.bytes().map(usize::from).sum();
    TAG_COLORS[sum % TAG_COLORS.len()]
}

/// Returns the frames' colors, the TUI's own.
pub fn frames_style() -> FramesStyle {
    FramesStyle {
        accent: ACCENT,
        on_accent: ON_ACCENT,
        strip: STRIP,
        front_tab: HOVER_ON_PICKED,
        dim: DIM,
        drop_zone: Color::Indexed(24),
        hover: Color::Indexed(238),
    }
}
