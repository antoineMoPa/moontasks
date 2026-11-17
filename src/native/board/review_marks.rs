//! The review cards the lane has marked, and what a click does to them - the lane's own copy of
//! [`super::selection`], so a group of reviews is gathered and dragged the way a group of cards is.
//!
//! cmd+click puts one review card in or out, shift+click takes the run between the anchor and
//! the card clicked down its status, and a plain click marks that card alone (and still opens
//! its review). A card dragged while marked carries every marked card with it.

use egui::Modifiers;

use crate::native::model::{BoardState, MarkedReview};

pub(crate) fn is_review_marked(board: &BoardState, review: &MarkedReview) -> bool {
    board.marked_reviews.contains(review)
}

/// Let every marked review go - a click beside the cards, or Escape.
pub(crate) fn let_go_of_all_reviews(board: &mut BoardState) {
    board.marked_reviews.clear();
    board.review_mark_anchor = None;
}

/// What a click on a review card does to the marks. `column` is the cards of its status as the
/// lane draws them, top to bottom, which is what a shift+click reads its run from.
pub(crate) fn mark_reviews_on_click(
    board: &mut BoardState,
    review: &MarkedReview,
    column: &[MarkedReview],
    modifiers: Modifiers,
) {
    if modifiers.command {
        if !board.marked_reviews.remove(review) {
            board.marked_reviews.insert(review.clone());
        }
        board.review_mark_anchor = Some(review.clone());
    } else if modifiers.shift {
        let run = run_of_reviews_between(column, board.review_mark_anchor.as_ref(), review);
        board.marked_reviews = run.iter().cloned().collect();
        if board.review_mark_anchor.is_none() {
            board.review_mark_anchor = Some(review.clone());
        }
    } else {
        mark_only_this_review(board, review.clone());
    }
}

fn mark_only_this_review(board: &mut BoardState, review: MarkedReview) {
    board.marked_reviews.clear();
    board.marked_reviews.insert(review.clone());
    board.review_mark_anchor = Some(review);
}

/// The cards between the anchor and the card clicked, both ends in. An anchor in another status
/// (or none) starts the run at the card clicked.
fn run_of_reviews_between<'a>(
    column: &'a [MarkedReview],
    anchor: Option<&MarkedReview>,
    review: &MarkedReview,
) -> &'a [MarkedReview] {
    let Some(clicked) = column.iter().position(|card| card == review) else {
        return &[];
    };
    let from = anchor
        .and_then(|anchor| column.iter().position(|card| card == anchor))
        .unwrap_or(clicked);
    &column[from.min(clicked)..=from.max(clicked)]
}

/// The reviews a drag begun on this card carries, in lane order (`lane` is every card the lane
/// draws). A marked card brings the marks; one that is not marked goes alone and becomes the
/// mark - unless cmd or shift is held, which adds it to the group.
pub(crate) fn collect_carried_reviews(
    board: &mut BoardState,
    primary: &MarkedReview,
    lane: &[MarkedReview],
    modifiers: Modifiers,
) -> Vec<MarkedReview> {
    if modifiers.command || modifiers.shift {
        board.marked_reviews.insert(primary.clone());
        board.review_mark_anchor = Some(primary.clone());
    } else if !is_review_marked(board, primary) {
        mark_only_this_review(board, primary.clone());
    }
    lane.iter()
        .filter(|card| is_review_marked(board, card))
        .cloned()
        .collect()
}

/// The carried reviews have landed in `status`: their marks follow them there, so what was
/// carried shows where it went.
pub(crate) fn move_review_marks_to_status(
    board: &mut BoardState,
    carried: &[MarkedReview],
    status: &str,
) {
    for review in carried {
        board.marked_reviews.remove(review);
    }
    board
        .marked_reviews
        .extend(carried.iter().map(|review| MarkedReview {
            status: status.to_string(),
            id: review.id.clone(),
        }));
    board.review_mark_anchor = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(status: &str, id: &str) -> MarkedReview {
        MarkedReview {
            status: status.to_string(),
            id: id.to_string(),
        }
    }

    fn marks(board: &BoardState) -> Vec<String> {
        let mut marked: Vec<String> = board
            .marked_reviews
            .iter()
            .map(|review| format!("{}/{}", review.status, review.id))
            .collect();
        marked.sort();
        marked
    }

    fn column() -> Vec<MarkedReview> {
        ["a", "b", "c"].map(|id| review("requested", id)).to_vec()
    }

    #[test]
    fn cmd_toggles_one_card_and_shift_takes_the_run() {
        let mut board = BoardState::default();
        let column = column();
        mark_reviews_on_click(&mut board, &column[0], &column, Modifiers::COMMAND);
        mark_reviews_on_click(&mut board, &column[2], &column, Modifiers::SHIFT);
        assert_eq!(marks(&board), ["requested/a", "requested/b", "requested/c"]);

        mark_reviews_on_click(&mut board, &column[1], &column, Modifiers::COMMAND);
        assert_eq!(marks(&board), ["requested/a", "requested/c"]);

        mark_reviews_on_click(&mut board, &column[1], &column, Modifiers::NONE);
        assert_eq!(
            marks(&board),
            ["requested/b"],
            "a plain click marks that card alone"
        );
    }

    #[test]
    fn a_marked_card_carries_the_marks_and_an_unmarked_one_carries_itself() {
        let mut board = BoardState::default();
        let lane = column();
        mark_reviews_on_click(&mut board, &lane[0], &lane, Modifiers::COMMAND);
        mark_reviews_on_click(&mut board, &lane[2], &lane, Modifiers::COMMAND);

        let carried = collect_carried_reviews(&mut board, &lane[2], &lane, Modifiers::NONE);
        assert_eq!(carried, [lane[0].clone(), lane[2].clone()]);

        let carried = collect_carried_reviews(&mut board, &lane[1], &lane, Modifiers::NONE);
        assert_eq!(carried, [lane[1].clone()]);
        assert_eq!(marks(&board), ["requested/b"]);
    }

    #[test]
    fn marks_follow_the_reviews_to_the_status_they_land_in() {
        let mut board = BoardState::default();
        let lane = column();
        mark_reviews_on_click(&mut board, &lane[0], &lane, Modifiers::COMMAND);
        mark_reviews_on_click(&mut board, &lane[1], &lane, Modifiers::COMMAND);
        move_review_marks_to_status(&mut board, &lane[..2], "pushed");
        assert_eq!(marks(&board), ["pushed/a", "pushed/b"]);
    }
}
