//! This module holds the colors frames are drawn in.

use ratatui::style::Color;

/// Holds the colors of tab strips, dividers and drop zones.
#[derive(Clone, Copy, Debug)]
pub struct FramesStyle {
    /// Colors the tab in front of the frame the keyboard is in, and the outline of a drop zone.
    pub accent: Color,
    /// Colors text on the accent.
    pub on_accent: Color,
    /// Colors a tab strip's ground.
    pub strip: Color,
    /// Colors the tab in front of a frame the keyboard is not in.
    pub front_tab: Color,
    /// Colors the tabs behind, and the lines between frames.
    pub dim: Color,
    /// Shades the drop zones.
    pub drop_zone: Color,
}

impl Default for FramesStyle {
    fn default() -> Self {
        Self {
            accent: Color::Yellow,
            on_accent: Color::Black,
            strip: Color::Indexed(235),
            front_tab: Color::Indexed(239),
            dim: Color::DarkGray,
            drop_zone: Color::Indexed(24),
        }
    }
}
