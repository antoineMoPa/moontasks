//! The × that dismisses a review, on its row on its task's card: it takes the review's file
//! away - see [`crate::moontasks::reviews`].

use egui::Ui;

use crate::{
    moontasks::reviews::ReviewFileView,
    native::{board::BoardAction, theme::Palette, widgets},
};

/// Dismiss a review: take its file away, from the folder it is in - not a namesake's.
pub(crate) fn dismiss_review(file: &ReviewFileView) -> BoardAction {
    BoardAction::DismissReview {
        review_id: file.id.clone(),
        from_status: file.status.clone(),
    }
}

/// Draw the × that dismisses a review.
pub(crate) fn draw_dismiss_mark(
    ui: &mut Ui,
    file: &ReviewFileView,
    palette: &Palette,
) -> egui::Response {
    let mark = widgets::close_button(ui, palette);
    mark.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, DISMISS_MARK_LABEL)
    });
    mark.on_hover_text(describe_dismiss_on_hover(file))
}

/// What the dismiss mark is called to a screen reader - and to a test looking for it.
pub(crate) const DISMISS_MARK_LABEL: &str = "dismiss review";

/// What the pointer on a dismiss mark is told: that the review goes, and which file with it.
fn describe_dismiss_on_hover(file: &ReviewFileView) -> String {
    format!("Dismiss this review - deletes {}", file.repo_path_of_file)
}
