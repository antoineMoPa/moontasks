//! The rendered page of a markdown file - HTML, drawn by the system's webview in the pane.
//!
//! Markdown is written to be read as a web page, so the preview is one: the tab's text (unsaved
//! edits included) rendered by pulldown-cmark into one self-contained HTML page, which
//! [`crate::native::webview_pane`] lays over the pane. The pictures the file writes beside
//! itself are files of the repo, which may be on another machine, so their bytes come through
//! the backend and go in the page as data URIs. A picture written as an address - `https://…`
//! - is left to the webview.
//!
//! Where there is no webview (anything but macOS, or the window in a browser), the pane offers
//! the same page in the browser instead.

use std::collections::{HashMap, HashSet};

use egui_frames::PaneId;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag};

use crate::native::{app::App, model::hash_of, widgets};

/// The page of one markdown tab, kept on its editor.
#[derive(Default)]
pub(crate) struct MarkdownPage {
    /// The page as last rendered; `None` until the first.
    pub(crate) html: Option<String>,
    /// Bumped each time the page is rendered again, so the webview knows to load it.
    pub(crate) arrival: u64,
    /// What the page was rendered from - the text, the theme and the pictures in hand - so
    /// it is rendered again only when one of them changes.
    rendered_from: Option<u64>,
    /// Each picture of the repo the text writes, by how it writes it, as a data URI.
    image_srcs: HashMap<String, String>,
    /// The pictures sent for, arrived or not, so each is read once.
    asked: HashSet<String>,
    /// Bumped as each picture arrives.
    images_arrived: u64,
}

impl MarkdownPage {
    /// Let go of the pictures, so they are read again: one written since is the one shown.
    pub(super) fn forget_pictures(&mut self) {
        self.image_srcs.clear();
        self.asked.clear();
        self.rendered_from = None;
    }
}

/// What the page is styled with: GitHub's readme measure, tables that wrap their cells into
/// the column, light or dark as the window is, on the pane's own background.
const PAGE_STYLE: &str = r#"
:root { color-scheme: light; --fg: #1f2328; --muted: #59636e;
  --line: #d1d9e0; --soft: rgba(127,127,127,.08); --link: #0969da; }
:root[data-theme="dark"] { color-scheme: dark; --fg: #e6edf3; --muted: #9198a1;
  --line: #3d444d; --link: #4493f8; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--fg);
  font: 16px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; }
main { max-width: 900px; margin: 0 auto; padding: 32px 16px 64px; overflow-wrap: break-word; }
a { color: var(--link); }
h1, h2 { border-bottom: 1px solid var(--line); padding-bottom: .3em; }
img, video { max-width: 100%; height: auto; }
code { font: 85% ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--soft);
  padding: .2em .4em; border-radius: 6px; }
pre { background: var(--soft); padding: 16px; border-radius: 6px; overflow-x: auto; }
pre code { padding: 0; background: none; }
blockquote { margin: 0; padding: 0 1em; color: var(--muted); border-left: .25em solid var(--line); }
table { border-collapse: collapse; width: 100%; }
th, td { border: 1px solid var(--line); padding: 6px 13px; vertical-align: top;
  overflow-wrap: anywhere; min-width: 4.5em; }
tr:nth-child(2n) { background: var(--soft); }
hr { border: 0; border-top: 1px solid var(--line); }
"#;

/// The pictures a markdown file writes as files of the repo, each as it is written in the
/// text and where it is from the repo's root. Pictures with a scheme (`https://`, `data:`)
/// are not files of the repo; neither is one that climbs out of it.
pub(super) fn collect_local_image_paths(
    markdown: &str,
    markdown_path: &str,
) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for event in Parser::new_ext(markdown, markdown_options()) {
        let Event::Start(Tag::Image { dest_url, .. }) = event else {
            continue;
        };
        if has_a_scheme(&dest_url) || found.iter().any(|(written, _)| *written == *dest_url) {
            continue;
        }
        if let Some(path) = path_beside(markdown_path, &dest_url) {
            found.push((dest_url.to_string(), path));
        }
    }
    found
}

/// The whole HTML page of a markdown file, its pictures swapped for the data URIs in
/// `image_srcs` (keyed by how the text writes each).
pub(super) fn render_markdown_page_html(
    title: &str,
    markdown: &str,
    image_srcs: &HashMap<String, String>,
    look: &PageLook,
) -> String {
    let events = Parser::new_ext(markdown, markdown_options()).map(|event| match event {
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = match image_srcs.get(dest_url.as_ref()) {
                Some(src) => CowStr::from(src.clone()),
                None => dest_url,
            };
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        other => other,
    });
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, events);
    format!(
        "<!doctype html>\n<html data-theme=\"{theme}\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title><style>{PAGE_STYLE}:root {{ --bg: {}; }}</style></head>\
         <body><main>\n{body}</main></body></html>\n",
        escape_html(title),
        look.background,
        theme = if look.dark { "dark" } else { "light" },
    )
}

/// How the page looks, taken from the window it is drawn in.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct PageLook {
    pub(crate) dark: bool,
    /// The pane's background, as CSS: `#rrggbb`.
    pub(crate) background: String,
}

impl PageLook {
    pub(crate) fn of_visuals(visuals: &egui::Visuals) -> Self {
        let [r, g, b, _] = visuals.panel_fill.to_array();
        Self {
            dark: visuals.dark_mode,
            background: format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

impl App {
    /// Render a markdown tab's page again if its text, the window's look or its pictures have
    /// changed since, and send for the pictures of the repo it writes that are not in hand.
    pub(crate) fn refresh_markdown_page(
        &mut self,
        pane_id: PaneId,
        session_id: &str,
        look: &PageLook,
    ) {
        let Some(editor) = self.model.file_editors.get_mut(&pane_id) else {
            return;
        };
        let text = editor.code.text();
        let page = &mut editor.markdown_page;
        let from = hash_of(&format!(
            "{text}\n{}\n{}\n{}",
            look.dark, look.background, page.images_arrived
        ));
        if page.rendered_from == Some(from) {
            return;
        }
        let title = widgets::file_name_of(&editor.file_path);
        page.html = Some(render_markdown_page_html(
            title,
            text,
            &page.image_srcs,
            look,
        ));
        page.arrival += 1;
        page.rendered_from = Some(from);

        let mut send_for = Vec::new();
        for (written, path) in collect_local_image_paths(text, &editor.file_path) {
            if page.asked.insert(written.clone()) {
                send_for.push((written, path));
            }
        }
        for (written, path) in send_for {
            let session_id = session_id.to_string();
            self.tasks.spawn(
                move |backend| backend.file_content(&session_id, &path),
                move |model, read| {
                    // A picture that cannot be read is left as written: the page shows its
                    // broken-image mark and alt text, which says as much.
                    let Some(src) = read.ok().and_then(|read| read.image_src) else {
                        return;
                    };
                    if let Some(editor) = model.file_editors.get_mut(&pane_id) {
                        editor.markdown_page.image_srcs.insert(written, src);
                        editor.markdown_page.images_arrived += 1;
                    }
                },
            );
        }
    }

    /// Open a markdown tab's page, as last rendered, in the browser.
    pub(crate) fn open_markdown_in_browser(&mut self, pane_id: PaneId) {
        let Some(editor) = self.model.file_editors.get(&pane_id) else {
            return;
        };
        let Some(html) = &editor.markdown_page.html else {
            return;
        };
        if let Err(error) = open_html_page_in_browser(&editor.file_path, html) {
            let message = format!("could not open {}: {error}", editor.file_path);
            self.model.error(message);
        }
    }
}

/// Write the page where the browser can read it, and open it there. The same markdown file
/// always lands on the same page, so opening it again shows it as it is now.
#[cfg(not(target_arch = "wasm32"))]
fn open_html_page_in_browser(file_path: &str, html: &str) -> anyhow::Result<()> {
    let folder = std::env::temp_dir().join("moon-markdown");
    std::fs::create_dir_all(&folder)?;
    let name = widgets::file_name_of(file_path).trim_end_matches(".md");
    let page = folder.join(format!("{name}-{:016x}.html", hash_of(file_path)));
    std::fs::write(&page, html)?;
    webbrowser::open(&page.to_string_lossy())?;
    Ok(())
}

/// In a browser's window, the page is a blob the window opens in a tab of its own.
#[cfg(target_arch = "wasm32")]
fn open_html_page_in_browser(_file_path: &str, html: &str) -> anyhow::Result<()> {
    let _ = hash_of;
    let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(html));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/html");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)
        .map_err(|_| anyhow::anyhow!("the page could not be made"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| anyhow::anyhow!("the page could not be made"))?;
    let window = web_sys::window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    match window.open_with_url_and_target(&url, "_blank") {
        Ok(Some(_)) => Ok(()),
        _ => Err(anyhow::anyhow!("the browser blocked the new tab")),
    }
}

fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
}

/// Whether a picture is written as an address of its own - `https://…`, `data:…` - rather
/// than as a path.
fn has_a_scheme(written: &str) -> bool {
    match written.split_once(':') {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => written.starts_with("//"),
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Where in the repo a picture is, from how the markdown file writes it: against the folder
/// the file is in, or against the repo's root when it is written with a slash in front -
/// which is how GitHub reads the same line. `None` for one that climbs out of the repo.
fn path_beside(markdown_path: &str, written: &str) -> Option<String> {
    let written = percent_decoded(written);
    let (folder, picture) = match written.strip_prefix('/') {
        Some(from_the_root) => ("", from_the_root),
        None => (widgets::directory_of(markdown_path), written.as_str()),
    };
    let mut path: Vec<&str> = Vec::new();
    for part in folder.split('/').chain(picture.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                path.pop()?;
            }
            part => path.push(part),
        }
    }
    Some(path.join("/"))
}

/// A picture's destination as the path it names: markdown writes a space in one as `%20`.
fn percent_decoded(written: &str) -> String {
    let bytes = written.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| written.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                at += 3;
            }
            None => {
                decoded.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_found_beside_the_markdown_file_that_writes_it() {
        for (markdown, written, path) in [
            ("README.md", "logo.png", Some("logo.png")),
            ("README.md", "./assets/logo.png", Some("assets/logo.png")),
            (
                "docs/guide/intro.md",
                "img/board.png",
                Some("docs/guide/img/board.png"),
            ),
            (
                "docs/guide/intro.md",
                "../img/board.png",
                Some("docs/img/board.png"),
            ),
            // From the repo's root, as GitHub reads it.
            (
                "docs/guide/intro.md",
                "/assets/logo.png",
                Some("assets/logo.png"),
            ),
            (
                "docs/intro.md",
                "a%20screen%20shot.png",
                Some("docs/a screen shot.png"),
            ),
            ("docs/intro.md", "../../elsewhere/secret.png", None),
        ] {
            assert_eq!(
                path_beside(markdown, written).as_deref(),
                path,
                "{written} in {markdown}"
            );
        }
    }

    #[test]
    fn only_pictures_of_the_repo_are_collected_once_each() {
        let markdown = "![a](img/a.gif) ![b](https://x.com/b.gif) ![a again](img/a.gif)\n\
                        ![c](data:image/png;base64,AA==) ![d](../../out.png)";
        assert_eq!(
            collect_local_image_paths(markdown, "docs/notes.md"),
            vec![("img/a.gif".to_string(), "docs/img/a.gif".to_string())]
        );
    }

    #[test]
    fn the_page_swaps_repo_pictures_for_data_and_keeps_addresses() {
        let markdown = "# Plan <1>\n\n| a | b |\n|---|---|\n| one | two |\n\n\
                        ![dot](img/dot.gif) ![web](https://x.com/w.gif)\n";
        let srcs = HashMap::from([(
            "img/dot.gif".to_string(),
            "data:image/gif;base64,R0lG".to_string(),
        )]);
        let look = PageLook {
            dark: true,
            background: "#101010".to_string(),
        };
        let html = render_markdown_page_html("notes.md", markdown, &srcs, &look);
        assert!(html.contains("<title>notes.md</title>"));
        assert!(html.contains(r#"data-theme="dark""#) && html.contains("--bg: #101010"));
        assert!(html.contains("<table>"));
        assert!(html.contains(r#"src="data:image/gif;base64,R0lG""#));
        assert!(html.contains(r#"src="https://x.com/w.gif""#));
        assert!(html.contains("Plan &lt;1&gt;"));
    }
}
