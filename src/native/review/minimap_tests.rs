//! Where a card's rows and comments are reported to be, and the layers the map is folded from.

use super::*;
use crate::native::review::diff::build_diff_lines;

#[test]
fn rows_drawn_one_under_the_other_are_one_stretch_and_a_comment_between_them_makes_two() {
    let mut card = CardBeingDrawn::at(100.0);
    card.line(4, 140.0, 19.0);
    card.line(5, 159.0, 19.0);
    card.note(178.0, 238.0);
    card.line(6, 242.0, 19.0);
    let shape = card.shape();

    let stretches: Vec<(f32, Range<usize>)> = shape
        .stretches
        .iter()
        .map(|stretch| (stretch.top, stretch.lines.clone()))
        .collect();
    assert_eq!(stretches, vec![(40.0, 4..6), (142.0, 6..7)]);
    assert_eq!(shape.notes, vec![Rangef::new(78.0, 138.0)]);
}

#[test]
fn a_card_s_lines_and_comment_go_in_the_layers_of_their_kind_where_the_card_put_them() {
    let lines = build_diff_lines(concat!(
        "@@ -1,3 +1,3 @@\n",
        " fn keep() {\n",
        "-    old();\n",
        "+    newer_name();\n",
        " }",
    ));
    // The card's toolbar takes its first 40 points, then a row every 20, and a comment
    // under the added line.
    let mut card = CardBeingDrawn::at(0.0);
    card.line(1, 40.0, 20.0);
    card.line(2, 60.0, 20.0);
    card.line(3, 80.0, 20.0);
    card.note(100.0, 160.0);
    card.line(4, 160.0, 20.0);

    let mut layers = Layers::default();
    let in_the_review = Card {
        hunk: 1,
        top: 1000.0,
        height: 200.0,
    };
    layers.card(&in_the_review, &card.shape(), &lines, 100, 20.0);

    let span = |line: &str| MinimapSpan::of(line, 100);
    assert_eq!(layers.cards, vec![1000.0..1200.0]);
    assert_eq!(
        layers.context,
        vec![(1040.0, span("fn keep() {")), (1160.0, span("}"))]
    );
    assert_eq!(layers.removed, vec![(1060.0, span("    old();"))]);
    assert_eq!(layers.removed_grounds, vec![1060.0..1080.0]);
    assert_eq!(layers.added, vec![(1080.0, span("    newer_name();"))]);
    assert_eq!(layers.added_grounds, vec![1080.0..1100.0]);
    assert_eq!(layers.notes, vec![1100.0..1160.0]);
}

#[test]
fn an_image_s_before_and_after_go_down_their_halves_of_the_map_a_span_every_half_row() {
    let mut card = CardBeingDrawn::at(100.0);
    card.picture(Side::Before, 150.0, 170.0);
    card.picture(Side::After, 150.0, 160.0);
    let mut layers = Layers::default();
    let in_the_review = Card {
        hunk: 1,
        top: 1000.0,
        height: 200.0,
    };
    layers.card(&in_the_review, &card.shape(), &[], 10, 10.0);

    let left = MinimapSpan::of("xxxxx", 10);
    let right = MinimapSpan::of("     xxxxx", 10);
    assert_eq!(
        layers.before_pictures,
        vec![
            (1050.0, left),
            (1055.0, left),
            (1060.0, left),
            (1065.0, left)
        ]
    );
    assert_eq!(
        layers.after_pictures,
        vec![(1050.0, right), (1055.0, right)]
    );
}

#[test]
fn a_picture_that_moved_is_a_card_drawn_differently() {
    let mut one = CardBeingDrawn::at(0.0);
    one.picture(Side::After, 40.0, 240.0);
    let mut nudged = CardBeingDrawn::at(0.0);
    nudged.picture(Side::After, 40.4, 240.4);
    let mut taller = CardBeingDrawn::at(0.0);
    taller.picture(Side::After, 40.0, 400.0);
    let one = one.shape();

    assert!(one.reads_as(&nudged.shape()));
    assert!(!one.reads_as(&taller.shape()));
    assert!(!one.reads_as(&CardShape::default()));
}
