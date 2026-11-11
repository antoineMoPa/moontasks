//! This crate gives [ratatui](https://ratatui.rs) tabs, splits and draggable panes - the
//! arrangement [egui_frames](https://github.com/antoineMoPa/egui_frames) gives a window, in a
//! terminal.
//!
//! A workspace is a tree of splits whose leaves are *frames*. A frame holds a strip of tabs
//! and draws whichever of its panes is in front. With the mouse, tabs slide along their strip
//! to make room as one is dragged, drop on another frame's strip, drop against a frame's edge
//! to split it in two, or against the workspace's outer edge to take a column of their own;
//! the lines between frames drag to resize them.
//!
//! - [`Layout<P>`] is the arrangement, the same one egui_frames keeps: plain data, `P` being
//!   whatever a pane is in your application.
//! - [`Frames`] draws it into a ratatui frame and turns the mouse into the next one.
//!
//! ```no_run
//! use moon_tui_frames::{Frames, Layout, PaneId, PaneView, PointerOutcome, Tab};
//! use ratatui::{Frame, layout::Rect, text::Line};
//!
//! struct Notes;
//!
//! impl PaneView<String> for Notes {
//!     fn tab(&mut self, _pane: PaneId, name: &String) -> Tab {
//!         Tab::new(name)
//!     }
//!
//!     fn draw_pane(&mut self, frame: &mut Frame, area: Rect, _: PaneId, name: &String, _: bool) {
//!         frame.render_widget(Line::raw(format!("the contents of {name}")), area);
//!     }
//! }
//!
//! fn draw(frame: &mut Frame, frames: &mut Frames, layout: &Layout<String>) {
//!     frames.draw(frame, frame.area(), layout, &mut Notes);
//! }
//! ```
//!
//! What a pane *is* stays the application's: where new ones open, what closing one costs,
//! and what a click inside one does - [`Frames::handle_the_mouse`] says which pane a click
//! landed in and leaves it there.

#![forbid(unsafe_code)]

mod frames;
#[cfg(test)]
mod frames_tests;
mod id;
mod layout;
mod pointer;
mod style;

pub use frames::{Frames, FramesEvent, PaneView, PointerOutcome, Tab};
pub use id::{FrameId, PaneId};
pub use layout::{DEFAULT_EDGE_SHARE, DropSide, Frame, Layout, LayoutNode, SplitDirection};
pub use style::FramesStyle;
