//! This module handles the mouse on the frames: a click on a tab brings it to the front, its
//! `×` asks for it to close, a drag carries it to another strip or against an edge, and a drag on
//! the line between two frames resizes them. Everything else is the pane's.

use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::{Position, Rect},
};

use crate::{
    DropSide, Frames, FramesEvent, Layout, PaneId, PointerOutcome, SplitDirection,
    frames::{DropTarget, Gesture, TabDrag},
};

/// Sets how far a pressed tab has to travel, in columns, to be carried rather than clicked.
const DRAG_STARTS_AFTER: u16 = 1;
/// Sets how much of a frame's body, from each edge, splits it rather than joining its tabs.
const EDGE_ZONE: f32 = 0.3;

impl Frames {
    /// Takes a mouse event: the frames' own, or handed back as the pane's.
    pub fn handle_the_mouse<P>(
        &mut self,
        mouse: MouseEvent,
        layout: &mut Layout<P>,
    ) -> PointerOutcome {
        let at = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.press(at, layout),
            MouseEventKind::Down(MouseButton::Middle) => match self.tab_at(at) {
                Some(pane) => PointerOutcome::Taken(vec![FramesEvent::PaneCloseRequested(pane)]),
                None => self.in_the_pane_at(at, layout),
            },
            MouseEventKind::Drag(MouseButton::Left) => self.drag(at, layout),
            MouseEventKind::Up(MouseButton::Left) => self.release(at, layout),
            _ => match &self.gesture {
                Some(Gesture::InPane(pane, area)) => PointerOutcome::InPane {
                    pane: *pane,
                    area: *area,
                },
                _ => self.pane_under(at),
            },
        }
    }

    fn press<P>(&mut self, at: Position, layout: &mut Layout<P>) -> PointerOutcome {
        self.gesture = None;
        if let Some(tab) = self.drawn.tabs.iter().find(|tab| tab.rect.contains(at)) {
            let pane = tab.pane;
            if tab.close.is_some_and(|close| close.contains(at)) {
                return PointerOutcome::Taken(vec![FramesEvent::PaneCloseRequested(pane)]);
            }
            layout.focus_pane(pane);
            self.gesture = Some(Gesture::Tab(TabDrag {
                pane,
                from: at,
                at,
                moving: false,
            }));
            return PointerOutcome::Taken(vec![FramesEvent::PaneFocused(pane)]);
        }
        if let Some(index) = self
            .drawn
            .dividers
            .iter()
            .position(|divider| divider.line.contains(at))
        {
            self.gesture = Some(Gesture::Divider(index));
            return PointerOutcome::Taken(Vec::new());
        }
        if let Some(frame) = self
            .drawn
            .frames
            .iter()
            .find(|frame| frame.strip.contains(at))
        {
            layout.set_active_frame(frame.id);
            return PointerOutcome::Taken(Vec::new());
        }
        let outcome = self.in_the_pane_at(at, layout);
        if let PointerOutcome::InPane { pane, area } = outcome {
            self.gesture = Some(Gesture::InPane(pane, area));
        }
        outcome
    }

    fn drag<P>(&mut self, at: Position, layout: &mut Layout<P>) -> PointerOutcome {
        match &mut self.gesture {
            Some(Gesture::Tab(drag)) => {
                drag.at = at;
                if at.x.abs_diff(drag.from.x) > DRAG_STARTS_AFTER || at.y != drag.from.y {
                    drag.moving = true;
                }
                PointerOutcome::Taken(Vec::new())
            }
            Some(Gesture::Divider(index)) => {
                let index = *index;
                self.resize_along_the_divider(index, at, layout);
                PointerOutcome::Taken(Vec::new())
            }
            Some(Gesture::InPane(pane, area)) => PointerOutcome::InPane {
                pane: *pane,
                area: *area,
            },
            None => self.pane_under(at),
        }
    }

    fn release<P>(&mut self, at: Position, layout: &mut Layout<P>) -> PointerOutcome {
        match self.gesture.take() {
            Some(Gesture::Tab(drag)) if drag.moving => {
                let Some(target) = self.drop_target_at(at, drag.pane) else {
                    return PointerOutcome::Taken(Vec::new());
                };
                // It slides into its new place from where it was let go.
                if let Some(strip) = self.strip_of_target(target) {
                    self.slides
                        .insert(drag.pane, at.x.saturating_sub(strip.x) as f32);
                } else {
                    self.slides.remove(&drag.pane);
                }
                drop_the_pane(layout, drag.pane, target, self.edge_share);
                layout.focus_pane(drag.pane);
                PointerOutcome::Taken(vec![FramesEvent::PaneMoved(drag.pane)])
            }
            Some(Gesture::Tab(_) | Gesture::Divider(_)) => PointerOutcome::Taken(Vec::new()),
            Some(Gesture::InPane(pane, area)) => PointerOutcome::InPane { pane, area },
            None => self.pane_under(at),
        }
    }

    fn strip_of_target(&self, target: DropTarget) -> Option<Rect> {
        match target {
            DropTarget::Frame {
                frame,
                side: DropSide::Tabs,
                ..
            } => self
                .drawn
                .frames
                .iter()
                .find(|drawn| drawn.id == frame)
                .map(|drawn| drawn.strip),
            _ => None,
        }
    }

    fn tab_at(&self, at: Position) -> Option<PaneId> {
        self.drawn
            .tabs
            .iter()
            .find(|tab| tab.rect.contains(at))
            .map(|tab| tab.pane)
    }

    /// Returns the pane whose body `at` is in, and gives its frame the keyboard.
    fn in_the_pane_at<P>(&self, at: Position, layout: &mut Layout<P>) -> PointerOutcome {
        let Some(frame) = self
            .drawn
            .frames
            .iter()
            .find(|frame| frame.body.contains(at))
        else {
            return PointerOutcome::Missed;
        };
        layout.set_active_frame(frame.id);
        match frame.front {
            Some(pane) => PointerOutcome::InPane {
                pane,
                area: frame.body,
            },
            None => PointerOutcome::Taken(Vec::new()),
        }
    }

    fn pane_under(&self, at: Position) -> PointerOutcome {
        self.drawn
            .frames
            .iter()
            .find(|frame| frame.body.contains(at))
            .and_then(|frame| {
                frame.front.map(|pane| PointerOutcome::InPane {
                    pane,
                    area: frame.body,
                })
            })
            .unwrap_or(PointerOutcome::Missed)
    }

    fn resize_along_the_divider<P>(&mut self, index: usize, at: Position, layout: &mut Layout<P>) {
        let Some(divider) = self.drawn.dividers.get(index) else {
            return;
        };
        let (start, length, pointer) = match divider.direction {
            SplitDirection::Row => (divider.split.x, divider.split.width, at.x),
            SplitDirection::Column => (divider.split.y, divider.split.height, at.y),
        };
        if length == 0 {
            return;
        }
        let fraction = (pointer.saturating_sub(start) as f32 / length as f32).clamp(0.0, 1.0);
        let mut sizes = divider.sizes.clone();
        let before: f32 = sizes[..divider.index].iter().sum();
        let pair = sizes[divider.index] + sizes[divider.index + 1];
        let first = (fraction - before).clamp(0.05, pair - 0.05);
        sizes[divider.index] = first;
        sizes[divider.index + 1] = pair - first;
        layout.set_split_sizes(&divider.path, &sizes);
        // What is drawn next is measured from these.
        if let Some(divider) = self.drawn.dividers.get_mut(index) {
            divider.sizes = sizes;
        }
    }

    /// Finds where a tab carried to `at` would land.
    pub(crate) fn drop_target_at(&self, at: Position, carried: PaneId) -> Option<DropTarget> {
        let workspace = self.drawn.workspace;
        if let Some(frame) = self
            .drawn
            .frames
            .iter()
            .find(|frame| frame.strip.contains(at))
        {
            // It goes before the first tab whose middle is right of the pointer, measured where
            // the tabs would be with the carried one out of the strip.
            let mut x = frame.strip.x;
            let mut before = None;
            for tab in self
                .drawn
                .tabs
                .iter()
                .filter(|tab| tab.frame == frame.id && tab.pane != carried)
            {
                let width = self.drawn.tab_widths.get(&tab.pane).copied().unwrap_or(0);
                if at.x < x + width / 2 {
                    before = Some(tab.pane);
                    break;
                }
                x += width;
            }
            return Some(DropTarget::Frame {
                frame: frame.id,
                side: DropSide::Tabs,
                before,
                onto_body: false,
            });
        }
        // The workspace's own edges lie past every frame.
        if at.x == workspace.left() && self.drawn.frames.len() > 1 {
            return Some(DropTarget::Edge(DropSide::Left));
        }
        if at.x + 1 >= workspace.right() && self.drawn.frames.len() > 1 {
            return Some(DropTarget::Edge(DropSide::Right));
        }
        if at.y + 1 >= workspace.bottom() && self.drawn.frames.len() > 1 {
            return Some(DropTarget::Edge(DropSide::Bottom));
        }
        let frame = self
            .drawn
            .frames
            .iter()
            .find(|frame| frame.body.contains(at))?;
        let body = frame.body;
        let across = (at.x - body.x) as f32 / body.width.max(1) as f32;
        let down = (at.y - body.y) as f32 / body.height.max(1) as f32;
        let nearest = [
            (across, DropSide::Left),
            (1.0 - across, DropSide::Right),
            (down, DropSide::Top),
            (1.0 - down, DropSide::Bottom),
        ]
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let side = if nearest.0 < EDGE_ZONE {
            nearest.1
        } else {
            DropSide::Tabs
        };
        Some(DropTarget::Frame {
            frame: frame.id,
            side,
            before: None,
            onto_body: side == DropSide::Tabs,
        })
    }
}

/// Puts a carried pane where it was dropped.
fn drop_the_pane<P>(layout: &mut Layout<P>, pane: PaneId, target: DropTarget, edge_share: f32) {
    match target {
        DropTarget::Frame {
            frame, onto_body, ..
        } if onto_body && layout.frame_of(pane) == Some(frame) => {}
        DropTarget::Frame {
            frame,
            side,
            before,
            ..
        } => layout.move_pane_to_frame(pane, frame, side, before),
        DropTarget::Edge(side) => layout.move_pane_against_edge(pane, side, edge_share),
    }
}
