//! Painting a scene: the picture, the dimming over it, and the objects set on it.
//!
//! The pictures are decoded once, into textures kept in egui's memory for as long as the
//! window lives, so a frame with a scene costs one textured quad for the picture and one per
//! object.

use std::sync::Arc;

use egui::{
    Color32, ColorImage, Context, Id, Mesh, Pos2, Rect, Shape, TextureHandle, TextureOptions, Ui,
    emath::Rot2, epaint::Vertex, pos2, vec2,
};

use super::{Scene, SceneSpec, layout, spec_of};

/// A scene's pictures, as textures.
struct Textures {
    scene: TextureHandle,
    objects: Vec<ObjectTextures>,
}

/// Transparent pixels added around each object's picture. A tilted quad's own edges are not
/// anti-aliased, so an object whose picture runs to its border showed stair-stepped sides;
/// with a clear margin the quad's edge falls on transparent texels and the texture filter
/// blends the picture's real edge instead.
const OBJECT_PADDING: usize = 2;

/// An object's picture at halving sizes, largest first: egui's renderer makes no mipmaps,
/// and a picture drawn at less than half its size through plain linear filtering skips texels
/// and shimmers at its edges. Painting picks the smallest level still at least as wide as
/// the object shows on screen.
struct ObjectTextures {
    levels: Vec<PaddedTexture>,
}

/// One level of an object's picture, with the share of each side that is padding.
struct PaddedTexture {
    texture: TextureHandle,
    /// The picture's own width in texels, without the padding.
    width: f32,
    /// Padding over picture, per axis: how much wider and taller than its rect the quad goes.
    grow: egui::Vec2,
}

/// No level is made narrower or shorter than this many texels.
const SMALLEST_LEVEL: usize = 16;

impl ObjectTextures {
    fn pick_level_for_width(&self, shown_width: f32) -> &PaddedTexture {
        self.levels
            .iter()
            .rev()
            .find(|level| level.width >= shown_width)
            .unwrap_or(&self.levels[0])
    }
}

/// `image` at half its size, each texel the mean of four. The pixels are premultiplied, so
/// clear texels do not bleed their color into the picture's edge.
fn halve_picture(image: &ColorImage) -> ColorImage {
    let [width, height] = image.size;
    let (half_width, half_height) = ((width / 2).max(1), (height / 2).max(1));
    let texel = |x: usize, y: usize| image.pixels[y.min(height - 1) * width + x.min(width - 1)];
    let mut half = ColorImage::filled([half_width, half_height], Color32::TRANSPARENT);
    for y in 0..half_height {
        for x in 0..half_width {
            let four = [
                texel(2 * x, 2 * y),
                texel(2 * x + 1, 2 * y),
                texel(2 * x, 2 * y + 1),
                texel(2 * x + 1, 2 * y + 1),
            ];
            let mean = |channel: fn(&Color32) -> u8| -> u8 {
                let sum: u32 = four.iter().map(|texel| u32::from(channel(texel))).sum();
                ((sum + 2) / 4) as u8
            };
            half.pixels[y * half_width + x] = Color32::from_rgba_premultiplied(
                mean(Color32::r),
                mean(Color32::g),
                mean(Color32::b),
                mean(Color32::a),
            );
        }
    }
    half
}

/// `bytes` decoded. A picture that will not decode is a build mistake the tests catch; here
/// it costs the object, not the window.
fn decode_picture(bytes: &[u8]) -> ColorImage {
    match image::load_from_memory(bytes) {
        Ok(image) => {
            let rgba = image.to_rgba8();
            ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            )
        }
        Err(_) => ColorImage::filled([1, 1], Color32::TRANSPARENT),
    }
}

/// `image` with `padding` transparent pixels on every side.
fn pad_picture(image: &ColorImage, padding: usize) -> ColorImage {
    let [width, height] = image.size;
    let padded_width = width + 2 * padding;
    let mut padded = ColorImage::filled([padded_width, height + 2 * padding], Color32::TRANSPARENT);
    for y in 0..height {
        let from = y * width;
        let to = (y + padding) * padded_width + padding;
        padded.pixels[to..to + width].copy_from_slice(&image.pixels[from..from + width]);
    }
    padded
}

fn load_scene_texture(ctx: &Context, name: String, bytes: &[u8]) -> TextureHandle {
    ctx.load_texture(name, decode_picture(bytes), TextureOptions::LINEAR)
}

fn load_object_textures(ctx: &Context, name: String, bytes: &[u8]) -> ObjectTextures {
    let mut picture = decode_picture(bytes);
    let mut levels = Vec::new();
    loop {
        let [width, height] = picture.size;
        let grow = vec2(
            (2 * OBJECT_PADDING) as f32 / width as f32,
            (2 * OBJECT_PADDING) as f32 / height as f32,
        );
        let texture = ctx.load_texture(
            format!("{name}-{}", levels.len()),
            pad_picture(&picture, OBJECT_PADDING),
            TextureOptions::LINEAR,
        );
        levels.push(PaddedTexture {
            texture,
            width: width as f32,
            grow,
        });
        if width.min(height) / 2 < SMALLEST_LEVEL {
            return ObjectTextures { levels };
        }
        picture = halve_picture(&picture);
    }
}

fn textures_of(ctx: &Context, spec: &'static SceneSpec) -> Arc<Textures> {
    let id = Id::new(("moon-background-textures", spec.scene.label()));
    if let Some(textures) = ctx.data(|data| data.get_temp::<Arc<Textures>>(id)) {
        return textures;
    }
    let label = spec.scene.label();
    let textures = Arc::new(Textures {
        scene: load_scene_texture(ctx, format!("background-{label}"), spec.scene_bytes),
        objects: spec
            .objects
            .iter()
            .map(|object| {
                load_object_textures(
                    ctx,
                    format!("background-{label}-{}", object.id),
                    object.bytes,
                )
            })
            .collect(),
    });
    ctx.data_mut(|data| data.insert_temp(id, textures.clone()));
    textures
}

/// Paint `scene` over the whole `window`, but only inside `clip`, under whatever is drawn
/// after. The picture is laid on the window rather than on the pane, so every pane that paints
/// it - and the window's own chrome - shows the same picture in the same place. `ground` is the
/// palette's own ground: the dimming is that color laid over the picture, so a dimmed scene
/// sinks toward the board it replaces rather than toward black.
pub(crate) fn paint_scene(ui: &Ui, clip: Rect, window: Rect, scene: Scene, ground: Color32) {
    let spec = spec_of(scene);
    let textures = textures_of(ui.ctx(), spec);
    let painter = ui.painter().with_clip_rect(clip);

    painter.rect_filled(window, 0.0, ground);
    painter.image(
        textures.scene.id(),
        window,
        cover_uv(spec, window),
        Color32::WHITE,
    );
    if spec.default_dim > 0 {
        painter.rect_filled(
            window,
            0.0,
            // Opaque first: the ground handed in is already see-through, and dimming with it
            // would weaken the dim by that much again.
            ground
                .to_opaque()
                .gamma_multiply(f32::from(spec.default_dim) / 100.0),
        );
    }

    let fade =
        Color32::from_white_alpha((f32::from(spec.default_fade) / 100.0 * 255.0).round() as u8);
    for placed in layout::place_objects(spec, window, f32::from(spec.default_size) / 100.0) {
        let tint = if placed.fades { fade } else { Color32::WHITE };
        let shown_width = placed.rect.width() * ui.ctx().pixels_per_point();
        let object = textures.objects[placed.index].pick_level_for_width(shown_width);
        // The quad grows by the padding, so the picture itself keeps its placed size.
        let rect = placed.rect.expand2(placed.rect.size() * object.grow / 2.0);
        painter.add(Shape::mesh(turned_quad(
            object.texture.id(),
            rect,
            placed.tilt_degrees,
            tint,
        )));
    }
}

/// The part of the picture that shows on `pane`, as a rectangle of texture coordinates.
fn cover_uv(spec: &SceneSpec, pane: Rect) -> Rect {
    let (scale, _) = layout::cover(spec.canvas, pane);
    let shown = vec2(
        pane.width() / (scale * spec.canvas.0),
        pane.height() / (scale * spec.canvas.1),
    );
    let margin = (vec2(1.0, 1.0) - shown) / 2.0;
    Rect::from_min_max(
        pos2(margin.x, margin.y),
        pos2(1.0 - margin.x, 1.0 - margin.y),
    )
}

/// `rect` turned about its center, as a textured quad.
fn turned_quad(texture: egui::TextureId, rect: Rect, degrees: f32, tint: Color32) -> Mesh {
    let turn = Rot2::from_angle(degrees.to_radians());
    let center = rect.center();
    let half = rect.size() / 2.0;
    let corner = |dx: f32, dy: f32, u: f32, v: f32| -> Vertex {
        let pos: Pos2 = center + turn * vec2(dx * half.x, dy * half.y);
        Vertex {
            pos,
            uv: pos2(u, v),
            color: tint,
        }
    };
    let mut mesh = Mesh::with_texture(texture);
    mesh.vertices.extend([
        corner(-1.0, -1.0, 0.0, 0.0),
        corner(1.0, -1.0, 1.0, 0.0),
        corner(1.0, 1.0, 1.0, 1.0),
        corner(-1.0, 1.0, 0.0, 1.0),
    ]);
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::background::Scene;

    #[test]
    fn a_padded_picture_keeps_its_pixels_in_the_middle_of_a_clear_frame() {
        let picture = ColorImage::filled([3, 2], Color32::RED);

        let padded = pad_picture(&picture, 2);

        assert_eq!(padded.size, [7, 6]);
        assert_eq!(padded.pixels[0], Color32::TRANSPARENT);
        assert_eq!(padded.pixels[2 * 7 + 2], Color32::RED);
        assert_eq!(padded.pixels[3 * 7 + 4], Color32::RED);
        assert_eq!(padded.pixels[3 * 7 + 5], Color32::TRANSPARENT);
        assert_eq!(padded.pixels[4 * 7 + 2], Color32::TRANSPARENT);
    }

    #[test]
    fn a_halved_picture_averages_its_four_texels_without_bleeding_clear_ones() {
        let mut picture = ColorImage::filled([2, 2], Color32::TRANSPARENT);
        picture.pixels[0] = Color32::from_rgba_premultiplied(200, 0, 0, 200);

        let half = halve_picture(&picture);

        assert_eq!(half.size, [1, 1]);
        assert_eq!(
            half.pixels[0],
            Color32::from_rgba_premultiplied(50, 0, 0, 50)
        );
    }

    #[test]
    fn a_wide_picture_on_a_square_pane_shows_its_middle() {
        let spec = spec_of(Scene::Moon);
        let pane = Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 500.0));

        let uv = cover_uv(spec, pane);

        assert!(
            (uv.height() - 1.0).abs() < 1e-4,
            "all of the picture's height shows"
        );
        assert!(uv.width() < 1.0, "the sides are cropped");
        assert!((uv.center().x - 0.5).abs() < 1e-4, "from the middle");
    }

    #[test]
    fn a_picture_turned_a_quarter_still_fills_the_same_center() {
        let rect = Rect::from_center_size(pos2(50.0, 50.0), vec2(40.0, 20.0));

        let mesh = turned_quad(egui::TextureId::default(), rect, 90.0, Color32::WHITE);

        let center_x = mesh.vertices.iter().map(|vertex| vertex.pos.x).sum::<f32>() / 4.0;
        let center_y = mesh.vertices.iter().map(|vertex| vertex.pos.y).sum::<f32>() / 4.0;
        assert!((center_x - 50.0).abs() < 1e-3 && (center_y - 50.0).abs() < 1e-3);
        let width = mesh.vertices[0].pos.x.max(mesh.vertices[2].pos.x)
            - mesh.vertices[0].pos.x.min(mesh.vertices[2].pos.x);
        assert!(
            (width - 20.0).abs() < 1e-3,
            "a turned quarter swaps its sides: {width}"
        );
    }
}
