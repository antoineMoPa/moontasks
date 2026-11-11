//! This module draws an arrangement: the splits, each frame's tab strip with its tabs sliding
//! to where they belong, the pane in front of each frame, and - while a tab is carried - the zone
//! it would land in.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    DEFAULT_EDGE_SHARE, DropSide, FrameId, FramesStyle, Layout, LayoutNode, PaneId, SplitDirection,
};

/// Sets the widest a tab's title is drawn.
const LONGEST_TAB_TITLE: usize = 28;
/// Sets how much of the way to where it belongs a sliding tab goes each time it is drawn.
const SLIDE_STEP: f32 = 0.45;

/// Holds what a tab says.
#[derive(Clone, Debug)]
pub struct Tab {
    title: String,
    closable: bool,
    marker: Option<(String, ratatui::style::Color)>,
}

impl Tab {
    /// Makes a tab with a title and a close button.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            closable: true,
            marker: None,
        }
    }

    /// Sets whether the tab has a close button, which is how a click asks for it to close.
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    /// Adds a mark drawn before the title, in its own color: a dot for something running.
    pub fn with_marker(mut self, marker: impl Into<String>, color: ratatui::style::Color) -> Self {
        self.marker = Some((marker.into(), color));
        self
    }

    fn label(&self) -> String {
        let title: String = if self.title.chars().count() > LONGEST_TAB_TITLE {
            let kept: String = self.title.chars().take(LONGEST_TAB_TITLE - 1).collect();
            format!("{kept}…")
        } else {
            self.title.clone()
        };
        match &self.marker {
            Some((marker, _)) => format!(" {marker} {title} "),
            None => format!(" {title} "),
        }
    }

    fn width(&self) -> u16 {
        self.label().chars().count() as u16 + if self.closable { 2 } else { 0 }
    }
}

/// Describes what the application draws: each pane's tab, and the pane itself.
pub trait PaneView<P> {
    /// Returns what a pane's tab says.
    fn tab(&mut self, id: PaneId, pane: &P) -> Tab;

    /// Draws the pane in front of a frame into `area`. `focused` is whether its frame is the
    /// one the keyboard is in.
    fn draw_pane(&mut self, frame: &mut Frame, area: Rect, id: PaneId, pane: &P, focused: bool);
}

/// Says what the mouse asked of the application, which the arrangement cannot decide for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramesEvent {
    /// Reports a click on the tab's close button, or a middle click on the tab.
    PaneCloseRequested(PaneId),
    /// A tab was clicked: its pane is in front, and its frame has the keyboard.
    PaneFocused(PaneId),
    /// A tab was dropped somewhere new.
    PaneMoved(PaneId),
}

/// Says what became of a mouse event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PointerOutcome {
    /// The frames used it: a tab, a line between frames, a strip.
    Taken(Vec<FramesEvent>),
    /// It is the pane's: inside its body, or a drag that started there. Its frame now has the
    /// keyboard.
    InPane {
        /// Holds the pane in front of the frame it landed in.
        pane: PaneId,
        /// Holds where that pane is drawn.
        area: Rect,
    },
    /// Means it landed nowhere the frames draw.
    Missed,
}

/// Says where a carried tab would land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DropTarget {
    /// Lands on a frame: among its tabs (before one of them, or at the end), or against one of
    /// its edges. `onto_body` is a drop on the middle of its body, which joins its tabs.
    Frame {
        frame: FrameId,
        side: DropSide,
        before: Option<PaneId>,
        onto_body: bool,
    },
    /// Lands against the workspace's outer edge.
    Edge(DropSide),
}

pub(crate) struct DrawnFrame {
    pub id: FrameId,
    pub strip: Rect,
    pub body: Rect,
    pub front: Option<PaneId>,
}

pub(crate) struct DrawnTab {
    pub pane: PaneId,
    pub frame: FrameId,
    /// Holds where it is drawn now, sliding or not.
    pub rect: Rect,
    pub close: Option<Rect>,
}

pub(crate) struct Divider {
    pub path: Vec<usize>,
    /// The line is between child `index` and child `index + 1`.
    pub index: usize,
    pub direction: SplitDirection,
    pub line: Rect,
    pub split: Rect,
    pub sizes: Vec<f32>,
}

/// Records where everything was drawn last, which is what the mouse is measured against.
#[derive(Default)]
pub(crate) struct Drawn {
    pub workspace: Rect,
    pub frames: Vec<DrawnFrame>,
    pub tabs: Vec<DrawnTab>,
    pub dividers: Vec<Divider>,
    /// Holds how wide each pane's tab was drawn.
    pub tab_widths: HashMap<PaneId, u16>,
}

pub(crate) struct TabDrag {
    pub pane: PaneId,
    pub from: Position,
    pub at: Position,
    pub moving: bool,
}

pub(crate) enum Gesture {
    Tab(TabDrag),
    Divider(usize),
    /// Marks a press inside a pane, whose drag is the pane's own until it is let go.
    InPane(PaneId, Rect),
}

/// Draws a [`Layout`] and turns the mouse into changes to it. Keep one across frames: it
/// remembers where things were drawn, what is being dragged, and where each tab is sliding.
#[derive(Default)]
pub struct Frames {
    pub(crate) style: FramesStyle,
    pub(crate) edge_share: f32,
    pub(crate) drawn: Drawn,
    pub(crate) gesture: Option<Gesture>,
    /// Holds where each tab is drawn, in columns from its strip's start, on its way to where it
    /// belongs.
    pub(crate) slides: HashMap<PaneId, f32>,
}

impl Frames {
    /// Makes frames in the default colors.
    pub fn new() -> Self {
        Self {
            edge_share: DEFAULT_EDGE_SHARE,
            ..Self::default()
        }
    }

    /// Gives the frames other colors.
    pub fn with_style(mut self, style: FramesStyle) -> Self {
        self.style = style;
        self
    }

    /// Says whether a tab is on its way somewhere: drawing again soon shows it getting there.
    pub fn is_moving(&self) -> bool {
        matches!(
            self.gesture,
            Some(Gesture::Tab(TabDrag { moving: true, .. }))
        ) || self.drawn.tabs.iter().any(|tab| {
            self.slides
                .get(&tab.pane)
                .is_some_and(|x| (*x - (tab.rect.x as f32)).abs() > 0.01)
        })
    }

    /// Returns where a pane was last drawn, if it is in front of its frame.
    pub fn pane_area(&self, pane: PaneId) -> Option<Rect> {
        self.drawn
            .frames
            .iter()
            .find(|frame| frame.front == Some(pane))
            .map(|frame| frame.body)
    }

    /// Draws the arrangement into `area`.
    pub fn draw<P, V: PaneView<P>>(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        layout: &Layout<P>,
        view: &mut V,
    ) {
        if self.edge_share == 0.0 {
            self.edge_share = DEFAULT_EDGE_SHARE;
        }
        let previous = std::mem::take(&mut self.drawn);
        let mut drawn = Drawn {
            workspace: area,
            ..Drawn::default()
        };
        let mut leaves = Vec::new();
        lay_out(
            layout.root(),
            area,
            &mut Vec::new(),
            &mut leaves,
            &mut drawn.dividers,
        );
        // Where the carried tab would land is measured against what was drawn last, which is
        // what the pointer was moving over.
        self.drawn = previous;
        let target = self
            .carried()
            .and_then(|(pane, at)| self.drop_target_at(at, pane));
        let carried = self.carried().map(|(pane, _)| pane);

        for divider in &drawn.dividers {
            let (symbol, line) = match divider.direction {
                SplitDirection::Row => ("│", divider.line),
                SplitDirection::Column => ("─", divider.line),
            };
            let buffer = frame.buffer_mut();
            for y in line.top()..line.bottom() {
                for x in line.left()..line.right() {
                    buffer[(x, y)]
                        .set_symbol(symbol)
                        .set_style(Style::new().fg(self.style.dim));
                }
            }
        }

        for (frame_id, rect) in leaves {
            let Some(layout_frame) = layout.frame(frame_id) else {
                continue;
            };
            let strip = Rect::new(rect.x, rect.y, rect.width, 1.min(rect.height));
            let body = Rect::new(
                rect.x,
                rect.y + strip.height,
                rect.width,
                rect.height - strip.height,
            );
            let keyboard_here = layout.active_frame() == frame_id;
            let front = layout_frame.active_pane();
            self.draw_the_strip(
                frame,
                &mut drawn,
                layout,
                view,
                frame_id,
                strip,
                keyboard_here,
                carried,
                target,
            );
            // A carried tab's pane stays drawn where it is until it is let go somewhere else.
            if let Some(pane) = front
                && let Some(content) = layout.pane(pane)
            {
                view.draw_pane(frame, body, pane, content, keyboard_here);
            }
            drawn.frames.push(DrawnFrame {
                id: frame_id,
                strip,
                body,
                front,
            });
        }

        self.drawn = drawn;
        if let Some(target) = target {
            self.draw_the_drop_zone(frame, target);
        }
        if let Some((pane, at)) = self.carried() {
            self.draw_the_carried_tab(frame, layout, view, pane, at);
        }
    }

    /// Returns the tab being dragged, once it has moved far enough to be a drag, and the
    /// pointer.
    pub(crate) fn carried(&self) -> Option<(PaneId, Position)> {
        match &self.gesture {
            Some(Gesture::Tab(drag)) if drag.moving => Some((drag.pane, drag.at)),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_the_strip<P, V: PaneView<P>>(
        &mut self,
        frame: &mut Frame,
        drawn: &mut Drawn,
        layout: &Layout<P>,
        view: &mut V,
        frame_id: FrameId,
        strip: Rect,
        keyboard_here: bool,
        carried: Option<PaneId>,
        target: Option<DropTarget>,
    ) {
        let buffer = frame.buffer_mut();
        for x in strip.left()..strip.right() {
            buffer[(x, strip.y)]
                .set_symbol(" ")
                .set_style(Style::new().bg(self.style.strip));
        }
        let Some(layout_frame) = layout.frame(frame_id) else {
            return;
        };
        let front = layout_frame.active_pane();
        // The carried tab leaves its place, and a gap opens where it would land.
        let mut order: Vec<Option<PaneId>> = layout_frame
            .panes()
            .iter()
            .copied()
            .filter(|pane| Some(*pane) != carried)
            .map(Some)
            .collect();
        if let Some(DropTarget::Frame {
            frame: into,
            side: DropSide::Tabs,
            before,
            onto_body: false,
        }) = target
            && into == frame_id
        {
            let at = before
                .and_then(|before| order.iter().position(|pane| *pane == Some(before)))
                .unwrap_or(order.len());
            order.insert(at, None);
        }
        let gap = carried
            .and_then(|pane| self.drawn.tab_widths.get(&pane).copied())
            .unwrap_or(8);

        let mut x = 0u16;
        for slot in order {
            let Some(pane) = slot else {
                x += gap;
                continue;
            };
            let Some(content) = layout.pane(pane) else {
                continue;
            };
            let tab = view.tab(pane, content);
            let width = tab.width();
            drawn.tab_widths.insert(pane, width);
            let belongs_at = x as f32;
            let slid = self.slides.entry(pane).or_insert(belongs_at);
            *slid += (belongs_at - *slid) * SLIDE_STEP;
            if (belongs_at - *slid).abs() < 0.5 {
                *slid = belongs_at;
            }
            let drawn_at = strip.x + slid.round().max(0.0) as u16;
            x += width;
            if drawn_at >= strip.right() {
                continue;
            }
            let width = width.min(strip.right() - drawn_at);
            let rect = Rect::new(drawn_at, strip.y, width, 1);
            let is_front = front == Some(pane);
            let style = match (is_front, keyboard_here) {
                (true, true) => Style::new()
                    .fg(self.style.on_accent)
                    .bg(self.style.accent)
                    .add_modifier(Modifier::BOLD),
                (true, false) => Style::new().bg(self.style.front_tab),
                (false, _) => Style::new().fg(self.style.dim).bg(self.style.strip),
            };
            let mut spans = Vec::new();
            match &tab.marker {
                Some((marker, color)) if !(is_front && keyboard_here) => {
                    spans.push(Span::styled(format!(" {marker}"), style.fg(*color)));
                    spans.push(Span::styled(
                        tab.label()[1 + marker.len()..].to_string(),
                        style,
                    ));
                }
                _ => spans.push(Span::styled(tab.label(), style)),
            }
            let close = tab.closable.then(|| {
                spans.push(Span::styled("× ", style));
                Rect::new(rect.x + tab.width() - 2, strip.y, 1, 1)
            });
            frame.render_widget(Line::from(spans), rect);
            drawn.tabs.push(DrawnTab {
                pane,
                frame: frame_id,
                rect,
                close: close.filter(|close| close.x < strip.right()),
            });
        }
    }

    fn draw_the_drop_zone(&self, frame: &mut Frame, target: DropTarget) {
        let Some(zone) = self.zone_of(target) else {
            return;
        };
        let zone = zone.intersection(frame.area());
        let buffer = frame.buffer_mut();
        for y in zone.top()..zone.bottom() {
            for x in zone.left()..zone.right() {
                buffer[(x, y)].set_bg(self.style.drop_zone);
            }
        }
        // Its outline is drawn too, so the zone reads over whatever the pane drew in that color.
        for x in zone.left()..zone.right() {
            buffer[(x, zone.top())].set_fg(self.style.accent);
            buffer[(x, zone.bottom().saturating_sub(1))].set_fg(self.style.accent);
        }
    }

    /// Returns where a drop would put the carried tab, shaded: the half of a frame a new one
    /// would take, the whole body it would join, or the column against the workspace's edge.
    pub(crate) fn zone_of(&self, target: DropTarget) -> Option<Rect> {
        match target {
            DropTarget::Frame {
                frame,
                side,
                onto_body,
                ..
            } => {
                let body = self
                    .drawn
                    .frames
                    .iter()
                    .find(|drawn| drawn.id == frame)?
                    .body;
                Some(match side {
                    DropSide::Tabs if onto_body => body,
                    DropSide::Tabs => return None,
                    DropSide::Left => Rect::new(body.x, body.y, body.width / 2, body.height),
                    DropSide::Right => Rect::new(
                        body.x + body.width / 2,
                        body.y,
                        body.width - body.width / 2,
                        body.height,
                    ),
                    DropSide::Top => Rect::new(body.x, body.y, body.width, body.height / 2),
                    DropSide::Bottom => Rect::new(
                        body.x,
                        body.y + body.height / 2,
                        body.width,
                        body.height - body.height / 2,
                    ),
                })
            }
            DropTarget::Edge(side) => {
                let all = self.drawn.workspace;
                let width = (all.width as f32 * self.edge_share) as u16;
                let height = (all.height as f32 * self.edge_share) as u16;
                Some(match side {
                    DropSide::Left => Rect::new(all.x, all.y, width, all.height),
                    DropSide::Right => Rect::new(all.right() - width, all.y, width, all.height),
                    DropSide::Top => Rect::new(all.x, all.y, all.width, height),
                    _ => Rect::new(all.x, all.bottom() - height, all.width, height),
                })
            }
        }
    }

    fn draw_the_carried_tab<P, V: PaneView<P>>(
        &self,
        frame: &mut Frame,
        layout: &Layout<P>,
        view: &mut V,
        pane: PaneId,
        at: Position,
    ) {
        let Some(content) = layout.pane(pane) else {
            return;
        };
        let label = view.tab(pane, content).label();
        let all = self.drawn.workspace;
        let width = (label.chars().count() as u16).min(all.width);
        let x =
            at.x.saturating_sub(width / 2)
                .clamp(all.x, all.right() - width);
        let y = at.y.clamp(all.y, all.bottom().saturating_sub(1));
        frame.render_widget(
            Line::styled(
                label,
                Style::new()
                    .fg(self.style.on_accent)
                    .bg(self.style.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Rect::new(x, y, width, 1),
        );
    }
}

/// Works out where each frame of a tree goes in `area`, and the lines between them.
fn lay_out(
    node: &LayoutNode,
    area: Rect,
    path: &mut Vec<usize>,
    leaves: &mut Vec<(FrameId, Rect)>,
    dividers: &mut Vec<Divider>,
) {
    match node {
        LayoutNode::Frame { frame } => leaves.push((*frame, area)),
        LayoutNode::Split {
            direction,
            children,
            sizes,
        } => {
            let count = children.len() as u16;
            let length = match direction {
                SplitDirection::Row => area.width,
                SplitDirection::Column => area.height,
            };
            let room = length.saturating_sub(count.saturating_sub(1));
            let mut start = 0u16;
            for (index, child) in children.iter().enumerate() {
                let last = index + 1 == children.len();
                let share = sizes.get(index).copied().unwrap_or(0.0);
                let size = if last {
                    length.saturating_sub(start)
                } else {
                    ((room as f32 * share).round() as u16).max(1)
                };
                let child_area = match direction {
                    SplitDirection::Row => Rect::new(area.x + start, area.y, size, area.height),
                    SplitDirection::Column => Rect::new(area.x, area.y + start, area.width, size),
                };
                path.push(index);
                lay_out(child, child_area, path, leaves, dividers);
                path.pop();
                start += size;
                if !last {
                    let line = match direction {
                        SplitDirection::Row => Rect::new(area.x + start, area.y, 1, area.height),
                        SplitDirection::Column => Rect::new(area.x, area.y + start, area.width, 1),
                    };
                    dividers.push(Divider {
                        path: path.clone(),
                        index,
                        direction: *direction,
                        line,
                        split: area,
                        sizes: sizes.clone(),
                    });
                    start += 1;
                }
            }
        }
    }
}
