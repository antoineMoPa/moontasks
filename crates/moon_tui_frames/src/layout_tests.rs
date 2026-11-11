//! The arrangement's own tests, as egui_frames has them.

use super::*;

/// Panes are whatever the application says they are; a name is enough to tell them apart.
fn layout() -> Layout<&'static str> {
    Layout::with_pane("review")
}

fn pane_of(layout: &Layout<&'static str>, name: &str) -> PaneId {
    layout
        .find_pane(|pane| *pane == name)
        .unwrap_or_else(|| panic!("expected a {name} pane"))
        .0
}

#[test]
fn an_arrangement_starts_as_one_frame_holding_one_pane() {
    let layout = layout();

    assert_eq!(layout.frame_ids().len(), 1);
    assert_eq!(layout.pane_count(), 1);
    assert!(layout.is_coherent());
}

#[test]
fn a_new_arrangement_has_a_frame_to_drop_things_on_and_nothing_in_it() {
    let layout: Layout<&str> = Layout::new();

    assert_eq!(layout.frame_ids().len(), 1);
    assert!(layout.is_empty());
    assert!(layout.is_coherent());
}

#[test]
fn names_are_never_handed_out_twice() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let first = layout.add_pane(frame, "shell", None);
    let second = layout.add_pane(frame, "shell", None);

    assert_ne!(first, second);
    assert_eq!(layout.pane_count(), 3);
}

#[test]
fn closing_the_pane_in_front_focuses_the_next_tab_of_the_strip() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let middle = layout.add_pane(frame, "shell", None);
    let last = layout.add_pane(frame, "file", None);
    layout.focus_pane(middle);

    layout.close_pane(middle);

    assert_eq!(layout.active_pane().map(|(pane, _)| pane), Some(last));
}

#[test]
fn closing_the_last_tab_of_the_strip_focuses_the_one_before_it() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let review = pane_of(&layout, "review");
    let last = layout.add_pane(frame, "shell", None);
    layout.focus_pane(last);

    layout.close_pane(last);

    assert_eq!(layout.active_pane().map(|(pane, _)| pane), Some(review));
}

#[test]
fn dropping_a_pane_on_an_edge_splits_the_frame() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let shell = layout.add_pane(frame, "shell", None);

    layout.move_pane_to_frame(shell, frame, DropSide::Right, None);

    assert_eq!(layout.frame_ids().len(), 2);
    assert!(matches!(layout.root(), LayoutNode::Split { .. }));
    assert!(layout.is_coherent());
}

#[test]
fn a_lone_tab_dropped_on_its_own_edge_changes_nothing() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let review = pane_of(&layout, "review");

    layout.move_pane_to_frame(review, frame, DropSide::Bottom, None);

    assert_eq!(layout.frame_ids().len(), 1);
    assert!(layout.contains(review));
}

#[test]
fn a_pane_dropped_against_the_workspace_takes_a_column_beside_everything() {
    // Two frames stacked one above the other, then a third pane against the right edge.
    let mut layout = layout();
    let frame = layout.active_frame();
    let stacked = layout.add_pane(frame, "shell", None);
    layout.move_pane_to_frame(stacked, frame, DropSide::Bottom, None);
    assert!(matches!(
        layout.root(),
        LayoutNode::Split {
            direction: SplitDirection::Column,
            ..
        }
    ));

    let moved = layout.add_pane(frame, "second shell", None);
    layout.move_pane_against_edge(moved, DropSide::Right, DEFAULT_EDGE_SHARE);

    let LayoutNode::Split {
        direction,
        children,
        ..
    } = layout.root()
    else {
        panic!("expected the workspace to be split");
    };
    assert_eq!(
        *direction,
        SplitDirection::Row,
        "the column runs down the side"
    );
    assert_eq!(children.len(), 2);
    let LayoutNode::Frame { frame: column } = &children[1] else {
        panic!("expected the new column to be one frame");
    };
    assert_eq!(
        layout.frame(*column).expect("expected the column").panes(),
        [moved],
        "the moved pane should be the whole of the new right-hand column"
    );
    assert!(layout.is_coherent());
}

#[test]
fn a_right_column_takes_its_share_from_the_existing_columns() {
    let mut layout = layout();

    layout.add_pane_against_edge(DropSide::Right, DEFAULT_EDGE_SHARE, "shell");

    let LayoutNode::Split { sizes, .. } = layout.root() else {
        panic!("expected the root to be a split");
    };
    assert_eq!(sizes.len(), 2);
    assert!((sizes.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    assert!((sizes[1] - DEFAULT_EDGE_SHARE).abs() < 1e-5);
}

#[test]
fn a_second_column_rescales_the_earlier_ones() {
    let mut layout = layout();
    layout.add_pane_against_edge(DropSide::Right, DEFAULT_EDGE_SHARE, "shell");

    layout.add_pane_against_edge(DropSide::Right, DEFAULT_EDGE_SHARE, "second shell");

    let LayoutNode::Split {
        sizes, children, ..
    } = layout.root()
    else {
        panic!("expected the root to be a split");
    };
    assert_eq!(children.len(), 3);
    assert!((sizes.iter().sum::<f32>() - 1.0).abs() < 1e-5);
}

#[test]
fn closing_the_last_pane_of_a_split_frame_removes_that_frame() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let shell = layout.add_pane_beside(frame, DropSide::Right, "shell");
    assert_eq!(layout.frame_ids().len(), 2);

    assert_eq!(layout.close_pane(shell), Some("shell"));

    assert_eq!(layout.frame_ids().len(), 1);
    assert!(layout.is_coherent());
}

#[test]
fn closing_the_last_pane_keeps_one_empty_frame_as_a_drop_target() {
    let mut layout = layout();
    let review = pane_of(&layout, "review");

    layout.close_pane(review);

    assert_eq!(layout.frame_ids().len(), 1);
    assert!(layout.is_empty());
    assert!(layout.is_coherent());
}

#[test]
fn reordering_tabs_within_a_frame_keeps_every_pane() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let review = pane_of(&layout, "review");
    let shell = layout.add_pane(frame, "shell", None);

    layout.move_pane_to_frame(shell, frame, DropSide::Tabs, Some(review));

    assert_eq!(
        layout.frame(frame).expect("expected the frame").panes(),
        [shell, review]
    );
    assert!(layout.is_coherent());
}

#[test]
fn a_tab_dragged_to_another_frames_strip_arrives_where_it_was_dropped() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let first = layout.add_pane_beside(frame, DropSide::Right, "first");
    let column = layout.active_frame();
    let second = layout.add_pane(column, "second", None);
    let moving = pane_of(&layout, "review");

    layout.move_pane_to_frame(moving, column, DropSide::Tabs, Some(second));

    assert_eq!(
        layout.frame(column).expect("expected the column").panes(),
        [first, moving, second]
    );
    assert_eq!(
        layout.frame_ids().len(),
        1,
        "the emptied frame went with it"
    );
}

/// A move is not a close and an open: applications key their own state on a pane's name,
/// so a dragged tab has to come out of the gesture as the same pane it went in as.
#[test]
fn a_moved_pane_keeps_its_name() {
    let mut layout = layout();
    let frame = layout.active_frame();
    let shell = layout.add_pane(frame, "shell", None);

    layout.move_pane_to_frame(shell, frame, DropSide::Right, None);
    assert!(layout.contains(shell), "after a split");

    layout.move_pane_against_edge(shell, DropSide::Bottom, DEFAULT_EDGE_SHARE);
    assert!(layout.contains(shell), "and after a move against the edge");

    let review_frame = layout
        .frame_of(pane_of(&layout, "review"))
        .expect("a frame");
    layout.move_pane_to_frame(shell, review_frame, DropSide::Tabs, None);
    assert!(layout.contains(shell), "and after joining another strip");
    assert_eq!(layout.pane_count(), 2, "and there is still only one of it");
}

#[test]
fn split_sizes_never_collapse_a_pane_to_nothing() {
    let mut layout = layout();
    let frame = layout.active_frame();
    layout.add_pane_beside(frame, DropSide::Right, "shell");

    layout.set_split_sizes(&[], &[0.0, 1.0]);

    let LayoutNode::Split { sizes, .. } = layout.root() else {
        panic!("expected the root to be a split");
    };
    assert!(sizes[0] >= MIN_SPLIT_FRACTION * 0.9);
    assert!((sizes.iter().sum::<f32>() - 1.0).abs() < 1e-5);
}

#[test]
fn frame_holding_keeps_panes_of_a_kind_together() {
    let mut layout = layout();
    let review_frame = layout.active_frame();
    layout.add_pane_beside(review_frame, DropSide::Right, "shell");
    let shell_frame = layout.active_frame();

    assert_eq!(
        layout.frame_holding(review_frame, |pane| *pane == "shell"),
        Some(shell_frame)
    );
    assert_eq!(
        layout.frame_holding(review_frame, |pane| *pane == "review"),
        Some(review_frame)
    );
}

#[test]
fn the_keyboard_walks_the_frames_in_the_order_they_are_laid_out() {
    let mut layout = layout();
    let first = layout.active_frame();
    layout.add_pane_beside(first, DropSide::Right, "shell");
    let second = layout.active_frame();

    layout.set_active_frame(first);
    assert_eq!(layout.focus_next_frame(), second);
    assert_eq!(layout.focus_next_frame(), first, "and round again");
}

#[test]
fn a_kept_shape_comes_back_as_empty_frames() {
    let mut layout = layout();
    let frame = layout.active_frame();
    layout.add_pane_beside(frame, DropSide::Right, "shell");
    assert_eq!(layout.frame_ids().len(), 2);

    let taken = layout.take_panes();

    assert_eq!(taken.len(), 2, "the panes are handed back to their owner");
    assert_eq!(layout.frame_ids().len(), 2, "the splits are the point");
    assert!(layout.is_empty());
    assert!(
        layout
            .frame_ids()
            .iter()
            .filter_map(|frame| layout.frame(*frame))
            .all(|frame| frame.panes().is_empty() && frame.active_pane().is_none())
    );
    assert!(layout.is_coherent());
}

#[test]
fn a_restored_shape_keeps_only_the_frames_something_landed_in() {
    let mut layout = layout();
    let frame = layout.active_frame();
    layout.add_pane_beside(frame, DropSide::Right, "shell");
    layout.take_panes();

    let primary = layout.primary_frame();
    layout.add_pane(primary, "review", None);
    layout.drop_empty_frames();

    assert_eq!(layout.frame_count(), 1, "the empty column should be gone");
    assert_eq!(layout.pane_count(), 1);
    assert!(layout.is_coherent());
}

#[test]
fn the_last_frame_stays_even_with_nothing_in_it() {
    let mut layout = layout();
    layout.take_panes();

    layout.drop_empty_frames();

    assert_eq!(
        layout.frame_ids().len(),
        1,
        "a workspace needs a drop target"
    );
}

/// A restored arrangement carries on naming things where the stored one left off: a name
/// handed out twice would put two frames in one box.
#[test]
fn names_from_a_restored_arrangement_are_not_handed_out_again() {
    let mut stored = layout();
    let frame = stored.active_frame();
    stored.add_pane_beside(frame, DropSide::Right, "shell");
    let known: Vec<FrameId> = stored.frame_ids();
    stored.take_panes();

    let refilled = stored.add_pane_beside(stored.primary_frame(), DropSide::Bottom, "review");
    let new_frame = stored.frame_of(refilled).expect("expected a frame");

    assert!(!known.contains(&new_frame), "{new_frame} was already taken");
    assert!(stored.is_coherent());
}

#[test]
fn a_frame_that_was_never_there_is_not_a_place_to_put_a_pane() {
    let mut layout = layout();
    let missing = FrameId(999);

    layout.add_pane(missing, "shell", None);

    assert_eq!(layout.pane_count(), 1, "nothing was added");
    assert!(layout.is_coherent());
}
