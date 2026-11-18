//! Layout - where a scene's objects land on a pane.
//!
//! Plain arithmetic over rectangles, kept apart from painting so it can be tested: the
//! scene's objects were designed on a canvas of their own, and what a pane of any shape does
//! with them is decided here.
//!
//! The picture is laid on the pane like a CSS `cover`: scaled to fill it, and cropped at the
//! sides that do not fit. Objects that belong to a spot on the picture follow it. Objects
//! that belong to no spot are spread over the pane as fractions of its width and height
//! instead, so a tall narrow pane still has them all in view rather than cropped away with
//! the picture's edges. Corner ornaments are pinned to the pane's corners.

use egui::{Pos2, Rect, pos2, vec2};

use super::{Corner, Kind, ObjectSpec, SceneSpec};

/// How far a corner ornament stands off the corner it is pinned to.
const CORNER_MARGIN: f32 = 10.0;
/// A corner ornament is a share of the pane's width, kept between these so it is neither a
/// speck on a small pane nor a wall on a big one.
const CORNER_WIDTH_SHARE: f32 = 0.12;
const CORNER_WIDTH_RANGE: (f32, f32) = (56.0, 150.0);

/// Where one object goes: the rectangle it is drawn in, before it is turned about its
/// center by `tilt_degrees`, and how strongly it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placed {
    pub index: usize,
    pub rect: Rect,
    pub tilt_degrees: f32,
    /// Whether the object is faded with the scene's objects: the picture's own decoration
    /// and the corners are not.
    pub fades: bool,
}

/// How a picture of `canvas` units lies on `pane`: its scale and where its top-left lands.
pub(super) fn cover(canvas: (f32, f32), pane: Rect) -> (f32, Pos2) {
    let scale = (pane.width() / canvas.0).max(pane.height() / canvas.1);
    let origin = pane.min
        + vec2(
            (pane.width() - canvas.0 * scale) / 2.0,
            (pane.height() - canvas.1 * scale) / 2.0,
        );
    (scale, origin)
}

/// Every object of `spec` placed on `pane`. `size` is the objects' scale, 1.0 for as
/// designed.
pub(super) fn place_objects(spec: &SceneSpec, pane: Rect, size: f32) -> Vec<Placed> {
    let (scale, origin) = cover(spec.canvas, pane);
    let mut placed = Vec::with_capacity(spec.objects.len());

    for (index, object) in spec.objects.iter().enumerate() {
        match object.kind {
            Kind::Corner(corner) => placed.push(Placed {
                index,
                rect: corner_rect(object, corner, pane),
                tilt_degrees: 0.0,
                fades: false,
            }),
            Kind::Ornament => placed.push(Placed {
                index,
                rect: Rect::from_min_size(
                    origin + vec2(object.at.0, object.at.1) * scale,
                    vec2(object.size.0, object.size.1) * scale,
                ),
                tilt_degrees: 0.0,
                fades: false,
            }),
            Kind::Scattered => {
                let unit = (pane.width() * pane.height() / (spec.canvas.0 * spec.canvas.1)).sqrt();
                let own = vec2(object.size.0, object.size.1) * unit;
                // Where it was drawn, as a share of the room the canvas left it: an object
                // drawn against the canvas's right or bottom edge ends against the pane's.
                let room = vec2(
                    (spec.canvas.0 - object.size.0).max(1.0),
                    (spec.canvas.1 - object.size.1).max(1.0),
                );
                let share = vec2(
                    (object.at.0 / room.x).clamp(0.0, 1.0),
                    (object.at.1 / room.y).clamp(0.0, 1.0),
                );
                let min = pane.min
                    + vec2(
                        share.x * (pane.width() - own.x),
                        share.y * (pane.height() - own.y),
                    );
                placed.push(Placed {
                    index,
                    rect: Rect::from_center_size(
                        Rect::from_min_size(min, own).center(),
                        own * size,
                    ),
                    tilt_degrees: object.tilt,
                    fades: true,
                });
            }
        }
    }

    placed.sort_by_key(|placed| placed.index);
    placed
}

fn corner_rect(object: &ObjectSpec, corner: Corner, pane: Rect) -> Rect {
    let width =
        (pane.width() * CORNER_WIDTH_SHARE).clamp(CORNER_WIDTH_RANGE.0, CORNER_WIDTH_RANGE.1);
    let size = vec2(width, width * object.size.1 / object.size.0);
    let left = pane.left() + CORNER_MARGIN;
    let right = pane.right() - CORNER_MARGIN - size.x;
    let top = pane.top() + CORNER_MARGIN;
    let bottom = pane.bottom() - CORNER_MARGIN - size.y;
    let min = match corner {
        Corner::TopLeft => pos2(left, top),
        Corner::TopRight => pos2(right, top),
        Corner::BottomLeft => pos2(left, bottom),
        Corner::BottomRight => pos2(right, bottom),
    };
    Rect::from_min_size(min, size)
}

#[cfg(test)]
mod tests {
    use super::super::{Scene, spec_of};
    use super::*;

    fn pane(width: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(100.0, 40.0), vec2(width, height))
    }

    #[test]
    fn the_picture_covers_the_pane_and_is_cropped_at_the_sides_that_do_not_fit() {
        let (scale, origin) = cover((2000.0, 1000.0), pane(500.0, 500.0));

        assert_eq!(
            scale, 0.5,
            "a square pane is filled by the picture's height"
        );
        assert_eq!(origin, pos2(100.0 - 250.0, 40.0));
    }

    #[test]
    fn corner_ornaments_stay_in_their_corners_whatever_the_pane() {
        let spec = spec_of(Scene::Trains);
        for (width, height) in [(400.0, 900.0), (1400.0, 600.0), (3000.0, 2000.0)] {
            let pane = pane(width, height);
            {
                for placed in place_objects(spec, pane, 1.0) {
                    let Kind::Corner(corner) = spec.objects[placed.index].kind else {
                        continue;
                    };
                    let rect = placed.rect;
                    assert!(
                        pane.contains_rect(rect),
                        "{corner:?} left a {width}x{height} pane"
                    );
                    let near_left = rect.left() - pane.left() <= CORNER_MARGIN + 0.5;
                    let near_right = pane.right() - rect.right() <= CORNER_MARGIN + 0.5;
                    let near_top = rect.top() - pane.top() <= CORNER_MARGIN + 0.5;
                    let near_bottom = pane.bottom() - rect.bottom() <= CORNER_MARGIN + 0.5;
                    let pinned = match corner {
                        Corner::TopLeft => near_left && near_top,
                        Corner::TopRight => near_right && near_top,
                        Corner::BottomLeft => near_left && near_bottom,
                        Corner::BottomRight => near_right && near_bottom,
                    };
                    assert!(
                        pinned,
                        "{corner:?} drifted from its corner on {width}x{height}"
                    );
                }
            }
        }
    }

    #[test]
    fn corner_ornaments_do_not_follow_the_object_size() {
        let spec = spec_of(Scene::Trains);
        let pane = pane(1000.0, 700.0);

        let small = place_objects(spec, pane, 0.4);
        let large = place_objects(spec, pane, 2.0);

        for (small, large) in small.iter().zip(&large) {
            if matches!(
                spec.objects[small.index].kind,
                Kind::Corner(_) | Kind::Ornament
            ) {
                assert_eq!(small.rect, large.rect, "{}", spec.objects[small.index].id);
            }
        }
    }

    #[test]
    fn scattered_objects_all_stay_on_a_pane_of_any_shape() {
        for scene in [Scene::Moon, Scene::Trains] {
            let spec = spec_of(scene);
            for (width, height) in [(360.0, 1000.0), (900.0, 700.0), (2200.0, 500.0)] {
                let pane = pane(width, height);
                for placed in place_objects(spec, pane, 1.0) {
                    if spec.objects[placed.index].kind == Kind::Scattered {
                        assert!(
                            pane.expand(1.0).contains_rect(placed.rect),
                            "{} left a {width}x{height} pane: {:?}",
                            spec.objects[placed.index].id,
                            placed.rect
                        );
                    }
                }
            }
        }
    }
}
