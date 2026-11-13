//! A review of images too big to draw: the window says so instead of falling over.

use egui_kittest::kittest::Queryable as _;

use crate::native::theme::ThemeMode;

use super::{Fixture, app_for, harness_with_loaded_review};

/// A picture longer on one side than the graphics card can hold in one texture. Handing it to
/// egui took the whole window down as the review opened.
#[test]
fn an_image_wider_than_a_texture_is_described_instead_of_drawn() {
    let fixture = Fixture::new("review-wide-image");
    fixture.write("README.md", "hello\n");
    fixture.commit("init");
    let path = fixture.root.join("panorama.png");
    image::RgbaImage::from_pixel(40_000, 4, image::Rgba([10, 20, 30, 255]))
        .save(&path)
        .expect("failed to write the wide image");

    let app = app_for(&fixture.root, ThemeMode::Dark);
    let mut harness = harness_with_loaded_review(app, ThemeMode::Dark);
    harness.run_steps(5);

    assert!(
        harness
            .query_by_label_contains("more than this window can draw")
            .is_some(),
        "the review should say the picture is too wide to draw"
    );
}
