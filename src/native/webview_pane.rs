//! Webview pane - shows an agent's visualization, or a markdown file's rendered page, in the
//! system's webview, WKWebView on macOS.
//!
//! The page is not drawn by egui. A webview is a native view of its own, put in the window as
//! a child of the view eframe draws into, and laid over the pane: egui only keeps the room for
//! it. So the two halves of a pane meet once a frame, after the workspace is drawn - the pane
//! writes down the rect it was given, and [`App::place_webviews`] moves the webview onto it,
//! hides the ones whose pane was not drawn, and lets go of the ones whose pane is gone.
//!
//! A native view is drawn over the whole wgpu surface, whatever egui paints there after it.
//! That is why the webviews are hidden while the palette or the find bar is up: those are
//! drawn over the panes, and would be drawn under the page.
//!
//! The page is the visualization's - see [`crate::native::visualizations`] - or, on a file
//! tab showing its preview, the markdown file's - see `file_pane::markdown_page`. A webview is
//! only made once its page has arrived, and loads the page again each time a newer one does.

use std::collections::HashMap;

use egui::{Align2, FontId, Rect, Sense, Ui};
use egui_frames::PaneId;

use crate::native::{app::App, theme};

/// The webviews of the window, and where their panes were drawn this frame.
#[derive(Default)]
pub(crate) struct Webviews {
    /// The rect each webview pane was drawn in this frame, in points. Emptied as the
    /// webviews are placed, so a pane missing from it on the next frame was not drawn.
    drawn: HashMap<PaneId, Rect>,
    /// Each webview pane's webview.
    #[cfg(target_os = "macos")]
    views: HashMap<PaneId, PlacedView>,
}

#[cfg(target_os = "macos")]
struct PlacedView {
    view: wry::WebView,
    /// Where it was last put - a webview is only moved when its pane has moved.
    rect: Rect,
    /// Which page it shows, by its arrival - see [`crate::native::visualizations::Page`].
    arrival: u64,
}

/// What a webview pane shows: its page, by when it arrived, and whether it is a markdown file's.
#[cfg(target_os = "macos")]
struct Shown<'a> {
    html: &'a str,
    arrival: u64,
    markdown: bool,
}

#[cfg(target_os = "macos")]
impl Webviews {
    /// Hide every webview, for a space that is being put down: the next frame is not drawn
    /// from this space's panes, so nothing else would hide them.
    pub(crate) fn hide_all(&mut self) {
        for placed in self.views.values() {
            placed
                .view
                .set_visible(false)
                .expect("a webview can always be hidden");
        }
    }
}

pub(crate) fn draw(app: &mut App, ui: &mut Ui, pane_id: PaneId) {
    let waiting = if cfg!(target_os = "macos") {
        "loading the visualization"
    } else {
        "visualizations are only shown on macOS for now"
    };
    reserve_room(app, ui, pane_id, waiting);
}

/// Keep the rest of `ui` for the pane's webview, saying `waiting` under it until its page
/// covers the words.
pub(crate) fn reserve_room(app: &mut App, ui: &mut Ui, pane_id: PaneId, waiting: &str) {
    // Only what can be seen of the pane: a webview is not clipped by the frame around it.
    let rect = ui.max_rect().intersect(ui.clip_rect());
    ui.allocate_rect(rect, Sense::hover());
    if cfg!(target_os = "macos") {
        app.webviews.drawn.insert(pane_id, rect);
    }
    // Under where the page will be, and covered by it once it has arrived.
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        waiting,
        FontId::proportional(theme::UI_SIZE),
        app.palette_of().muted,
    );
}

impl App {
    /// Put each webview where its pane was drawn this frame, make the webview of a pane whose
    /// page has arrived, load a newer page into one that has an older, hide the ones whose pane
    /// was not drawn - a tab behind another - and drop the ones whose pane is closed.
    ///
    /// After the workspace is drawn, since that is where the rects come from, and with the
    /// window's handle, which is only lent out to `eframe::App::ui`.
    #[cfg(target_os = "macos")]
    pub(crate) fn place_webviews(&mut self, window: &eframe::Frame, ctx: &egui::Context) {
        let drawn = std::mem::take(&mut self.webviews.drawn);
        let model = &self.model;
        let open: HashMap<PaneId, Option<Shown>> = model
            .layout
            .panes()
            .filter_map(|(pane_id, pane)| match pane {
                crate::native::panes::Pane::Visualization { fragment_path } => {
                    let shown = model
                        .visualizations
                        .pages
                        .get(fragment_path)
                        .map(|page| Shown {
                            html: page.html.as_str(),
                            arrival: page.arrival,
                            markdown: false,
                        });
                    Some((pane_id, shown))
                }
                crate::native::panes::Pane::File { .. } => {
                    let page = &model.file_editors.get(&pane_id)?.markdown_page;
                    let shown = page.html.as_deref().map(|html| Shown {
                        html,
                        arrival: page.arrival,
                        markdown: true,
                    });
                    Some((pane_id, shown))
                }
                _ => None,
            })
            .collect();
        // Dropping a webview takes it out of the window.
        self.webviews
            .views
            .retain(|pane_id, _| open.contains_key(pane_id));

        // Drawn over the panes: a page left showing would cover them.
        let covered = model.palette.is_open() || model.find.is_some();
        let pixels_per_point = ctx.pixels_per_point();
        for (pane_id, placed) in &self.webviews.views {
            if covered || !drawn.contains_key(pane_id) {
                placed
                    .view
                    .set_visible(false)
                    .expect("a webview can always be hidden");
            }
        }
        for (pane_id, rect) in drawn {
            // A pane drawn this frame can be closed later in the same frame - its tab's close
            // button is handled after the pane was drawn - and there is nothing to place for it.
            let Some(Some(page)) = open.get(&pane_id) else {
                continue;
            };
            let bounds = physical_bounds(rect, pixels_per_point);
            let placed = self.webviews.views.entry(pane_id).or_insert_with(|| {
                let mut builder = wry::WebViewBuilder::new();
                if page.markdown {
                    builder = builder.with_navigation_handler(navigate_within_the_page);
                }
                let view = builder
                    .with_html(page.html)
                    // The first click on a window in the background lands on what it clicked,
                    // as it does on the rest of the window's panes, rather than only bringing
                    // the window forward.
                    .with_accept_first_mouse(true)
                    .with_bounds(bounds)
                    .build_as_child(window)
                    .expect("the window is an AppKit window, which a webview is made a child of");
                PlacedView {
                    view,
                    rect,
                    arrival: page.arrival,
                }
            });
            if placed.arrival != page.arrival {
                placed
                    .view
                    .load_html(page.html)
                    .expect("a webview can always be handed a page");
                placed.arrival = page.arrival;
            }
            if placed.rect != rect {
                placed
                    .view
                    .set_bounds(bounds)
                    .expect("a child webview can always be moved");
                placed.rect = rect;
            }
            if !covered {
                placed
                    .view
                    .set_visible(true)
                    .expect("a webview can always be shown");
            }
        }
    }
}

/// Whether a markdown page may go where a click on a link takes it: to a heading of its own,
/// yes; to a web address, no - that opens in the browser, and the pane keeps the file.
#[cfg(target_os = "macos")]
fn navigate_within_the_page(url: String) -> bool {
    let leaves = url.starts_with("http://") || url.starts_with("https://");
    if leaves {
        // Nowhere to say it failed from in here; the click simply does nothing.
        let _ = webbrowser::open(&url);
    }
    !leaves
}

/// A rect of the window in points, as the webview is placed: in physical pixels from the
/// window's top left. wry turns it into the flipped, logical coordinates AppKit wants.
#[cfg(target_os = "macos")]
fn physical_bounds(rect: Rect, pixels_per_point: f32) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::PhysicalPosition::new(
            (rect.min.x * pixels_per_point).round() as i32,
            (rect.min.y * pixels_per_point).round() as i32,
        )
        .into(),
        size: wry::dpi::PhysicalSize::new(
            (rect.width() * pixels_per_point).round() as u32,
            (rect.height() * pixels_per_point).round() as u32,
        )
        .into(),
    }
}
