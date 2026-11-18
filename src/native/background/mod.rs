//! Background - the picture the task board can be painted over, and the objects set on it.
//!
//! A scene is one background picture plus a handful of separate objects (a radio, a
//! locomotive, a skier) that are placed on it at paint time rather than baked into it. That is
//! what lets the same scene be scattered over a pane of any shape, with its objects faded so
//! the cards over them stay readable - see [`layout`]. The pictures are made from vector art and
//! shipped as WebP in `assets/backgrounds`; [`catalog`] is the generated list of them with the
//! positions they were designed at.
//!
//! The board paints the scene itself, behind its columns - see [`paint::paint_scene`] - and
//! its cards are drawn translucent over it - see [`card_fill`]. Panes that hold code or a
//! terminal never see it: a picture behind text that is read for a living is a cost, not a
//! style.

mod catalog;
mod layout;
mod paint;

pub(crate) use crate::settings::BackgroundScene as Scene;
pub(crate) use paint::paint_scene;

use egui::Color32;

use crate::native::app::App;

/// Which corner of the pane an ornament is pinned to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// What an object is to the layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    /// Pinned to a corner of the pane at a size that follows the pane's width, so an ornate
    /// frame corner stays in its corner however the window is shaped.
    Corner(Corner),
    /// Part of the picture's decoration: stays where it was drawn, and is neither resized nor
    /// faded.
    Ornament,
    /// Free to be anywhere: spread over the pane as a fraction of its width and height, or
    /// lined up on the shelf.
    Scattered,
}

/// One object of a scene, as it was designed.
pub(super) struct ObjectSpec {
    pub id: &'static str,
    pub bytes: &'static [u8],
    pub kind: Kind,
    /// How big it was drawn, in the scene's canvas units.
    pub size: (f32, f32),
    /// Where its top-left was drawn, in the scene's canvas units.
    pub at: (f32, f32),
    /// How far it is turned when scattered, in degrees.
    pub tilt: f32,
}

/// A scene: its picture, its objects, and what it looks best with.
pub(super) struct SceneSpec {
    pub scene: Scene,
    /// The size of the canvas the objects were designed on.
    pub canvas: (f32, f32),
    pub scene_bytes: &'static [u8],
    pub objects: &'static [ObjectSpec],
    pub default_size: u8,
    pub default_fade: u8,
    pub default_dim: u8,
}

pub(super) fn spec_of(scene: Scene) -> &'static SceneSpec {
    match scene {
        Scene::Moon => &catalog::MOON,
        Scene::Trains => &catalog::TRAINS,
    }
}

/// Paint this project's scene over the whole window, first of all, so the parts of the window
/// that have no pane - the strip of tabs, the bar at the foot - show it too.
pub(crate) fn paint_window(app: &App, ui: &egui::Ui, ground: Color32) {
    if let Some(scene) = app.background_of_this_project() {
        let window = ui.ctx().content_rect();
        paint_scene(ui, window, window, scene, ground);
    }
}

/// How much taller a column's heading is over a scene than on the plain board, where it is
/// bare text: the padding of the pill it is drawn in, top and bottom.
pub(crate) fn heading_extra_height(app: &App) -> f32 {
    match app.background_of_this_project() {
        Some(_) => HEADING_PADDING_Y * 2.0,
        None => 0.0,
    }
}

/// The padding of a heading's pill: the strip's tabs are padded about the same.
pub(crate) const HEADING_PADDING: (i8, i8) = (8, 3);
const HEADING_PADDING_Y: f32 = HEADING_PADDING.1 as f32;

/// What a column's heading is backed with so it can be read over a picture, when the board has
/// one: the fill a tab in the strip is drawn with, mostly opaque, so a heading reads as a tab
/// does. `None` for the plain board.
pub(crate) fn heading_backing(
    app: &App,
    palette: &crate::native::theme::Palette,
) -> Option<Color32> {
    app.background_of_this_project()
        .map(|_| palette.control_bg.gamma_multiply(HEADING_BACKING_OPACITY))
}

/// How solid the backing of a column's heading is.
const HEADING_BACKING_OPACITY: f32 = 0.9;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_has_a_picture_and_objects_to_set_on_it() {
        for scene in Scene::ALL {
            let spec = spec_of(scene);
            assert!(!spec.scene_bytes.is_empty(), "{scene:?} has no picture");
            assert!(!spec.objects.is_empty(), "{scene:?} has no objects");
            for object in spec.objects {
                assert!(!object.bytes.is_empty(), "{} has no image", object.id);
            }
        }
    }

    #[test]
    fn every_picture_decodes() {
        for scene in Scene::ALL {
            let spec = spec_of(scene);
            image::load_from_memory(spec.scene_bytes).expect("expected the picture to decode");
            for object in spec.objects {
                image::load_from_memory(object.bytes)
                    .unwrap_or_else(|error| panic!("{} did not decode: {error}", object.id));
            }
        }
    }
}
