//! The before/after view of a changed image.
//!
//! The server hands both sides over as `data:` URIs. egui has no loader for that scheme, so
//! each side is decoded here once and given
//! to egui as bytes under a `bytes://` URI derived from the image's own content - which is also
//! what makes egui drop the old texture when the picture changes.

use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use egui::{RichText, Ui, vec2};

use crate::{
    api::{ImageDiffView, image_formats::extension_of_mime_type},
    native::{
        app::App,
        model::hash_of,
        theme::{Palette, SMALL_SIZE},
        widgets,
    },
};

/// The share of the window an image may take vertically. Inside a scroll area a pane is as
/// tall as it likes, so the window is what the picture has to be measured against.
const TALLEST_IMAGE_FRACTION: f32 = 0.7;
/// Between the before and the after.
const IMAGE_GAP: f32 = 12.0;
/// Around the whole comparison. The hunk card has no inner margin of its own - a diff's lines
/// run edge to edge - so the pictures bring their own.
const IMAGE_PADDING: i8 = 10;
/// Between a side's label and its picture.
const LABEL_GAP: f32 = 4.0;

pub(crate) fn draw_image_diff(
    app: &mut App,
    ui: &mut Ui,
    file_path: &str,
    image: &ImageDiffView,
    palette: &Palette,
) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(IMAGE_PADDING))
        .show(ui, |ui| {
            draw_file_link(app, ui, file_path, palette);
            ui.add_space(8.0);

            // Each side takes half of what is going, scaled up to fill it - a favicon is
            // otherwise drawn at its own 16 points and is impossible to compare.
            let half = ((ui.available_width() - IMAGE_GAP) / 2.0).max(80.0);
            let tallest = ui.ctx().viewport_rect().height() * TALLEST_IMAGE_FRACTION;

            ui.horizontal_top(|ui| {
                for (label, source, left_out_bytes, missing) in [
                    (
                        "before",
                        &image.before_src,
                        image.before_left_out_bytes,
                        "(added)",
                    ),
                    (
                        "after",
                        &image.after_src,
                        image.after_left_out_bytes,
                        "(deleted)",
                    ),
                ] {
                    ui.vertical(|ui| {
                        ui.set_width(half);
                        ui.label(
                            RichText::new(label)
                                .size(SMALL_SIZE - 1.0)
                                .color(palette.muted),
                        );
                        ui.add_space(LABEL_GAP);
                        let longest_side = ui.ctx().input(|input| input.max_texture_side);
                        match source
                            .as_deref()
                            .map(|source| decode_image_side(app, source, longest_side))
                        {
                            Some(Ok(source)) => {
                                ui.add(
                                    egui::Image::new(source)
                                        .fit_to_exact_size(vec2(half, tallest))
                                        .maintain_aspect_ratio(true),
                                );
                            }
                            Some(Err(why)) => {
                                ui.label(RichText::new(why).color(palette.warn));
                            }
                            None => match left_out_bytes {
                                Some(bytes) => {
                                    ui.label(
                                        RichText::new(describe_left_out_image(bytes))
                                            .color(palette.muted),
                                    );
                                }
                                None => {
                                    ui.label(RichText::new(missing).color(palette.muted));
                                }
                            },
                        }
                    });
                    ui.add_space(IMAGE_GAP);
                }
            });
        });
}

/// The image's name, and beside it a way to open the file itself - the pane can only show it
/// at the size it fits, and comparing pixels needs a real viewer.
///
/// The name is a label rather than a button so it can be selected and copied. One widget
/// cannot both sweep a selection and act on a click, so opening it is its own button.
fn draw_file_link(app: &mut App, ui: &mut Ui, file_path: &str, palette: &Palette) {
    let full_path = app.repo_file_path(file_path).filter(|path| path.exists());

    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(RichText::new(file_path).size(SMALL_SIZE).color(
                if full_path.is_some() {
                    palette.accent
                } else {
                    palette.muted
                },
            ))
            .selectable(true),
        );

        // A review served from somewhere else has no local file to open.
        let Some(full_path) = full_path else {
            return;
        };
        if widgets::quiet_button(ui, "[open]")
            .on_hover_text(format!("open {}", full_path.display()))
            .clicked()
            && let Err(error) = webbrowser::open(&full_path.to_string_lossy())
        {
            app.model
                .error(format!("could not open {file_path}: {error}"));
        }
    });
}

/// What is said in place of a side too large for the repo's side to send.
fn describe_left_out_image(bytes: u64) -> String {
    format!(
        "({:.1} MB: too large to preview)",
        bytes as f64 / (1024.0 * 1024.0)
    )
}

/// An image the review has decoded, kept until the window closes.
pub(crate) struct DecodedImage {
    extension: &'static str,
    bytes: Arc<[u8]>,
    /// Width and height from the image's own header; `None` when no decoder of this build
    /// reads it, which egui then says for itself.
    size: Option<(u32, u32)>,
}

/// The image behind a `data:` URI, decoded on first sight - or why it is not drawn: it cannot
/// be read, or it is longer on a side than one texture of the graphics card, which egui would
/// take the window down over.
fn decode_image_side(
    app: &mut App,
    data_uri: &str,
    longest_side: usize,
) -> Result<egui::ImageSource<'static>, String> {
    let key = hash_of(data_uri);
    let decoded = app.decoded_images.entry(key).or_insert_with(|| {
        decode_image_data_uri(data_uri).map(|(extension, bytes)| DecodedImage {
            extension,
            size: read_pixel_size(&bytes),
            bytes,
        })
    });
    let decoded = decoded
        .as_ref()
        .ok_or_else(|| "(cannot read this image)".to_string())?;
    if let Some(too_long) = decoded
        .size
        .and_then(|size| describe_too_long_to_draw(size, longest_side))
    {
        return Err(too_long);
    }
    Ok(bytes_source(key, decoded.extension, &decoded.bytes))
}

/// What egui draws a decoded image from. `key` is the hash of the `data:` URI it arrived as.
pub(crate) fn bytes_source(
    key: u64,
    extension: &str,
    bytes: &Arc<[u8]>,
) -> egui::ImageSource<'static> {
    egui::ImageSource::Bytes {
        // Named after the content: two hunks showing the same image share one texture, and an
        // image that changes gets a URI egui has never seen.
        uri: format!("bytes://image-{key:016x}.{extension}").into(),
        bytes: egui::load::Bytes::Shared(Arc::clone(bytes)),
    }
}

/// Splits `data:<mime>;base64,<payload>` into the extension to name it by and its bytes. egui
/// sniffs the bytes for most formats, but the URI's extension is what routes an SVG to the
/// loader that can draw it.
pub(crate) fn decode_image_data_uri(data_uri: &str) -> Option<(&'static str, Arc<[u8]>)> {
    let body = data_uri.strip_prefix("data:")?;
    let (mime_type, payload) = body.split_once(";base64,")?;
    let extension = extension_of_mime_type(mime_type)?;
    let bytes = BASE64.decode(payload).ok()?;
    Some((extension, Arc::from(bytes)))
}

/// How wide and how tall a picture is in pixels, as its own header has it. `None` for bytes
/// no decoder of this build reads.
pub(crate) fn read_pixel_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
}

/// What is said in place of a picture with a side longer than the graphics card draws, and
/// `None` for one that fits.
pub(crate) fn describe_too_long_to_draw(
    (width, height): (u32, u32),
    longest_side: usize,
) -> Option<String> {
    (width.max(height) as usize > longest_side).then(|| {
        format!(
            "{width} × {height} pixels is more than this window can draw: {longest_side} a side at most"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_uri_gives_up_its_bytes_and_a_name_to_read_them_as() {
        let uri = format!(
            "data:image/png;base64,{}",
            BASE64.encode(b"not really a png")
        );

        let (extension, bytes) = decode_image_data_uri(&uri).expect("the uri should decode");

        assert_eq!(extension, "png");
        assert_eq!(&*bytes, b"not really a png");
    }

    #[test]
    fn an_svg_keeps_the_extension_its_loader_is_chosen_by() {
        let uri = format!("data:image/svg+xml;base64,{}", BASE64.encode(b"<svg/>"));

        let (extension, _) = decode_image_data_uri(&uri).expect("the uri should decode");

        assert_eq!(extension, "svg");
    }

    #[test]
    fn anything_that_is_not_a_base64_image_data_uri_is_refused() {
        assert!(decode_image_data_uri("https://example.com/cat.png").is_none());
        assert!(decode_image_data_uri("data:image/png,raw%20bytes").is_none());
        assert!(decode_image_data_uri("data:text/plain;base64,aGk=").is_none());
        assert!(decode_image_data_uri("data:image/png;base64,not base64!").is_none());
    }
}
