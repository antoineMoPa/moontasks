//! Drawing the tree down the left of a file tab: the arrows back and forward, the filter, the
//! rows, and the edge that is dragged to make it wider.

use egui::{
    Align, Color32, FontId, Layout, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Ui,
    UiBuilder, pos2, text::LayoutJob, vec2,
};
use egui_frames::PaneId;

use crate::native::{
    app::App,
    bindings::{self, Action},
    review::sidebar::stage_status_color,
    theme::{Palette, SMALL_SIZE, UI_SIZE},
    widgets,
};

use super::{
    NARROWEST,
    rows::{self, Row, RowKind},
};

const ROW_HEIGHT: f32 = 22.0;
/// How far each folder level moves its rows in.
const INDENT: f32 = 12.0;
/// Between the tree's edges and what is in it.
const SIDE: f32 = 10.0;
/// The row of buttons at the bottom.
const FOOTER_HEIGHT: f32 = 24.0;
/// The widest the tree may take of the pane, so the file always keeps the larger part.
const AT_MOST_OF_THE_PANE: f32 = 0.6;

/// What was asked for by a press in the tree, done once it has been drawn.
enum Asked {
    Toggle(String),
    Open(String),
    Step(isize),
    GoTo(usize),
    CollapseAll,
    ToggleIgnored,
}

/// How one row is drawn, beyond what the row itself says.
struct Look {
    current: bool,
    dot: Option<Color32>,
    counts: Option<(usize, usize)>,
}

impl App {
    /// Draw the tree in a strip down the left of `full`, and give back the rest of `full`,
    /// which is the file's.
    pub(crate) fn draw_file_tree(
        &mut self,
        ui: &mut Ui,
        full: Rect,
        pane_id: PaneId,
        session_id: &str,
        file_path: &str,
    ) -> Rect {
        let palette = self.palette_of();
        self.follow_tree_changes(session_id);
        self.search_tree_filter(session_id);

        let widest = (full.width() * AT_MOST_OF_THE_PANE).max(NARROWEST);
        let width = self.model.file_tree.width.clamp(NARROWEST, widest);
        let tree_rect = Rect::from_min_size(full.min, vec2(width, full.height()));
        ui.painter().rect_filled(tree_rect, 0.0, palette.panel);

        let mut tree_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(("file-tree", pane_id))
                .max_rect(tree_rect.shrink2(vec2(0.0, 4.0)))
                .layout(Layout::top_down(Align::Min)),
        );
        tree_ui.set_clip_rect(tree_rect.intersect(ui.clip_rect()));
        let asked = self.draw_tree_contents(&mut tree_ui, &palette, pane_id, session_id, file_path);

        // The edge last, so it is what a press on it reaches rather than the rows under it.
        let handle = Rect::from_x_y_ranges(
            tree_rect.right() - 3.0..=tree_rect.right() + 3.0,
            tree_rect.y_range(),
        );
        let edge = ui.interact(
            handle,
            egui::Id::new(("file-tree-edge", pane_id)),
            Sense::drag(),
        );
        let edge_ink = if edge.hovered() || edge.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            palette.accent
        } else {
            palette.line
        };
        ui.painter().vline(
            tree_rect.right(),
            tree_rect.y_range(),
            Stroke::new(1.0, edge_ink),
        );
        if edge.dragged() {
            self.model.file_tree.width = (width + edge.drag_delta().x).clamp(NARROWEST, widest);
        }

        for asked in asked {
            self.do_in_tree(session_id, asked);
        }
        Rect::from_min_max(pos2(tree_rect.right() + 1.0, full.top()), full.max)
    }

    fn do_in_tree(&mut self, session_id: &str, asked: Asked) {
        let tree = &mut self.model.file_tree;
        match asked {
            Asked::Toggle(folder) => {
                if let Some(repo) = tree.repos.get_mut(session_id)
                    && !repo.open.remove(&folder)
                {
                    repo.open.insert(folder);
                }
            }
            Asked::Open(file_path) => self.open_file_pane_at(session_id, &file_path, None),
            Asked::Step(by) => self.step_through_files(by),
            Asked::GoTo(index) => self.go_to_file_in_history(index),
            Asked::CollapseAll => {
                if let Some(repo) = tree.repos.get_mut(session_id) {
                    repo.open.clear();
                }
            }
            Asked::ToggleIgnored => tree.show_ignored = !tree.show_ignored,
        }
    }

    fn draw_tree_contents(
        &mut self,
        ui: &mut Ui,
        palette: &Palette,
        pane_id: PaneId,
        session_id: &str,
        file_path: &str,
    ) -> Vec<Asked> {
        let mut asked = Vec::new();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(SIDE - 3.0);
            self.draw_history_arrows(ui, palette, &mut asked);
            ui.add_space(4.0);
            ui.label(
                RichText::new("FILES")
                    .size(SMALL_SIZE - 0.5)
                    .color(palette.muted),
            );
            if let Some(repo) = self.model.file_tree.repos.get(session_id) {
                let (added, removed) = (repo.changes.added, repo.changes.removed);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(SIDE);
                    widgets::line_counts(ui, added, removed, SMALL_SIZE - 0.5, palette);
                });
            }
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(SIDE);
            let filter = egui::TextEdit::singleline(&mut self.model.file_tree.filter)
                .id_salt(("file-tree-filter", pane_id))
                .hint_text("filter…")
                .font(FontId::proportional(SMALL_SIZE + 0.5))
                .desired_width(ui.available_width() - SIDE);
            let typed = ui.add(filter);
            if typed.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.model.file_tree.filter.clear();
            }
        });
        ui.add_space(4.0);

        let show_ignored = self.model.file_tree.show_ignored;
        let filtering = !self.model.file_tree.filter.trim().is_empty();
        let mut to_ask = Vec::new();
        let repo = self
            .model
            .file_tree
            .repos
            .entry(session_id.to_string())
            .or_default();
        let (rows, note) = match (&repo.filtered, filtering) {
            (Some(filtered), true) => match (&filtered.paths, &filtered.error) {
                (_, Some(error)) => (Vec::new(), Some(error.clone())),
                (None, None) => (Vec::new(), Some("searching…".to_string())),
                (Some(paths), None) if paths.is_empty() => {
                    (Vec::new(), Some("no file matches".to_string()))
                }
                (Some(paths), None) => (
                    rows::filtered_rows(paths),
                    filtered
                        .cut_short
                        .then(|| format!("first {} shown - keep typing", paths.len())),
                ),
            },
            _ => (
                rows::tree_rows(repo, show_ignored, &super::Folder::is_stale, &mut to_ask),
                None,
            ),
        };

        let rows_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
        let font = FontId::proportional(UI_SIZE - 0.5);
        egui::ScrollArea::vertical()
            .id_salt(("file-tree-rows", pane_id))
            .auto_shrink([false, false])
            .max_height(rows_height)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for row in &rows {
                    let change = repo.changes.files.get(&row.path).copied();
                    let look = Look {
                        current: row.path == file_path,
                        dot: match row.kind {
                            RowKind::File => {
                                change.map(|change| stage_status_color(change.status, palette))
                            }
                            RowKind::Folder { .. } if repo.changes.folders.contains(&row.path) => {
                                Some(palette.unstaged)
                            }
                            _ => None,
                        },
                        counts: change.map(|change| (change.added, change.removed)),
                    };
                    let response = draw_row(ui, row, &look, palette, &font);
                    // Scrolled to by the tab showing that file, which is the one in front
                    // when it is revealed.
                    if file_path == row.path
                        && repo.scroll_to_revealed
                        && repo.revealed.as_deref() == Some(row.path.as_str())
                    {
                        response.scroll_to_me(Some(Align::Center));
                        repo.scroll_to_revealed = false;
                    }
                    if response.clicked() {
                        match row.kind {
                            RowKind::Folder { .. } if !filtering => {
                                asked.push(Asked::Toggle(row.path.clone()));
                            }
                            RowKind::File => asked.push(Asked::Open(row.path.clone())),
                            _ => {}
                        }
                    }
                    if matches!(row.kind, RowKind::File) {
                        response.on_hover_text(&row.path);
                    }
                }
                if let Some(note) = &note {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(SIDE);
                        ui.label(RichText::new(note).size(SMALL_SIZE).color(palette.muted));
                    });
                }
            });

        widgets::divider(ui, palette);
        ui.horizontal(|ui| {
            ui.add_space(SIDE - 4.0);
            if widgets::quiet_button(ui, "[collapse all]")
                .on_hover_text("Fold every folder")
                .clicked()
            {
                asked.push(Asked::CollapseAll);
            }
            let ignored_label = match show_ignored {
                true => "[hide ignored]",
                false => "[show ignored]",
            };
            if widgets::quiet_button(ui, ignored_label)
                .on_hover_text(
                    "Files git ignores - target/, node_modules/ - shown dimmed, or left out",
                )
                .clicked()
            {
                asked.push(Asked::ToggleIgnored);
            }
        });

        for folder in to_ask {
            self.list_tree_folder(session_id, &folder);
        }
        asked
    }

    /// `‹ ›`: back and forward through the files shown. Each says where it goes and its keys;
    /// a right-click on either lists the whole history to jump to.
    fn draw_history_arrows(&self, ui: &mut Ui, palette: &Palette, asked: &mut Vec<Asked>) {
        let history = &self.model.file_tree.history;
        let arrows = [
            (
                -1.0,
                -1,
                history.back(),
                Action::GoBack,
                "Back to",
                "Nothing to go back to",
            ),
            (
                1.0,
                1,
                history.forward(),
                Action::GoForward,
                "Forward to",
                "Nothing to go forward to",
            ),
        ];
        for (direction, by, target, action, goes, nowhere) in arrows {
            let keys = bindings::chord_of(action)
                .map(bindings::describe)
                .unwrap_or_default();
            let ink = match target {
                Some(_) => palette.ink,
                None => palette.line,
            };
            let hover = match target {
                Some(shown) => format!("{goes} {}  {keys}", shown.name()),
                None => format!("{nowhere}  {keys}"),
            };
            // Drawn rather than typed: a `‹` sits at the height the font gives it, which is
            // not the middle of the row `FILES` is in.
            let (rect, response) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::click());
            let (tip, middle) = (rect.center().x + direction * 1.5, rect.center().y);
            let stroke = Stroke::new(if response.hovered() { 1.8 } else { 1.4 }, ink);
            ui.painter().add(Shape::line(
                vec![
                    pos2(tip - direction * 3.5, middle - 4.0),
                    pos2(tip, middle),
                    pos2(tip - direction * 3.5, middle + 4.0),
                ],
                stroke,
            ));
            let response = widgets::clickable(response)
                .on_hover_text(format!("{hover}\nRight-click for the list"));
            if response.clicked() && target.is_some() {
                asked.push(Asked::Step(by));
            }
            response.context_menu(|ui| {
                let entries = history.entries();
                if entries.is_empty() {
                    ui.label(RichText::new("No file shown yet").color(palette.muted));
                }
                for (index, shown) in entries.iter().enumerate().rev() {
                    let folder = widgets::directory_of(&shown.file_path);
                    let label = match folder.is_empty() {
                        true => shown.name().to_string(),
                        false => format!("{}   {folder}", shown.name()),
                    };
                    if ui.selectable_label(index == history.at(), label).clicked() {
                        asked.push(Asked::GoTo(index));
                        ui.close();
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new(format!("{} of {}", entries.len(), super::history::LIMIT))
                        .size(SMALL_SIZE)
                        .color(palette.muted),
                );
            });
        }
    }
}

fn draw_row(ui: &mut Ui, row: &Row, look: &Look, palette: &Palette, font: &FontId) -> Response {
    let sense = match row.kind {
        RowKind::Note => Sense::hover(),
        _ => Sense::click(),
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), sense);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    if look.current {
        painter.rect_filled(rect, 0.0, palette.control_active_bg);
    } else if response.hovered() && !matches!(row.kind, RowKind::Note) {
        painter.rect_filled(rect, 0.0, palette.row_hover_bg);
    }

    let middle = rect.center().y;
    let mut x = rect.left() + SIDE + row.depth as f32 * INDENT;
    if let RowKind::Folder { open } = row.kind {
        painter.add(Shape::convex_polygon(
            chevron(x, middle, open),
            palette.muted,
            Stroke::NONE,
        ));
    }
    x += 12.0;
    if let Some(dot) = look.dot {
        painter.circle_filled(pos2(x + 3.0, middle), 3.0, dot);
        x += 10.0;
    }

    // The counts at the right end, so the name is cut short before them rather than under.
    let mut right = rect.right() - SIDE;
    if let Some((added, removed)) = look.counts {
        let small = FontId::proportional(SMALL_SIZE - 0.5);
        for (sign, count, ink) in [('−', removed, palette.removed), ('+', added, palette.added)] {
            if count == 0 {
                continue;
            }
            let galley = painter.layout_no_wrap(format!("{sign}{count}"), small.clone(), ink);
            right -= galley.size().x;
            painter.galley(pos2(right, middle - galley.size().y / 2.0), galley, ink);
            right -= 4.0;
        }
    }

    let mut ink = match (&row.kind, look.current) {
        (RowKind::Note, _) => palette.muted,
        (_, true) => palette.accent,
        (RowKind::Folder { .. }, false) => palette.muted,
        (RowKind::File, false) => palette.ink,
    };
    if row.ignored {
        ink = ink.gamma_multiply(0.5);
    }
    let mut job = LayoutJob::simple_singleline(row.name.clone(), font.clone(), ink);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - 2.0 - x).max(12.0));
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    painter.galley(pos2(x, middle - galley.size().y / 2.0), galley, ink);
    response
}

/// `▶` for a folded folder, `▼` for an open one, drawn rather than typed so it does not hang
/// on a font having the glyph.
fn chevron(x: f32, middle: f32, open: bool) -> Vec<Pos2> {
    match open {
        true => vec![
            pos2(x, middle - 2.5),
            pos2(x + 8.0, middle - 2.5),
            pos2(x + 4.0, middle + 2.5),
        ],
        false => vec![
            pos2(x + 1.5, middle - 4.0),
            pos2(x + 6.5, middle),
            pos2(x + 1.5, middle + 4.0),
        ],
    }
}
