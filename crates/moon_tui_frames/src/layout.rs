//! What the arrangement *is*: a tree of splits whose leaves are frames, and a frame's tabs.
//!
//! Nothing here draws anything or reads a pointer, which is what makes an arrangement
//! something an application can store, restore and reason about on its own.

use std::collections::HashMap;

use crate::{FrameId, PaneId};

/// How much of a split's space the smallest child is allowed to be squeezed down to.
const MIN_SPLIT_FRACTION: f32 = 0.1;

/// How much of the workspace a new column or row against its edge takes by default.
pub const DEFAULT_EDGE_SHARE: f32 = 0.35;

/// Which way a split divides the space it was given.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitDirection {
    /// Children side by side, dividing the width.
    Row,
    /// Children stacked, dividing the height.
    Column,
}

/// Where a pane lands when it is dropped on a frame: on its tab strip, or against one of its
/// edges, which splits that frame in two.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropSide {
    /// Among the frame's tabs.
    Tabs,
    /// A new frame to the left of it.
    Left,
    /// A new frame to the right of it.
    Right,
    /// A new frame above it.
    Top,
    /// A new frame below it.
    Bottom,
}

impl DropSide {
    fn direction(self) -> Option<SplitDirection> {
        match self {
            Self::Tabs => None,
            Self::Left | Self::Right => Some(SplitDirection::Row),
            Self::Top | Self::Bottom => Some(SplitDirection::Column),
        }
    }

    /// Whether a new frame goes before (0) or after (1) the frame it was dropped on.
    fn offset(self) -> usize {
        match self {
            Self::Left | Self::Top => 0,
            _ => 1,
        }
    }
}

/// The arrangement's tree: frames at the leaves, splits in between.
#[derive(Clone, Debug)]
pub enum LayoutNode {
    /// One frame.
    Frame {
        /// Which frame.
        frame: FrameId,
    },
    /// A row or column of children, and the share of the space each one takes.
    Split {
        /// Which way the space is divided.
        direction: SplitDirection,
        /// The children, in the order they are laid out.
        children: Vec<LayoutNode>,
        /// One share per child, summing to one.
        sizes: Vec<f32>,
    },
}

impl LayoutNode {
    /// Every frame of this subtree, in the order it is laid out.
    pub fn frames(&self) -> Vec<FrameId> {
        match self {
            Self::Frame { frame } => vec![*frame],
            Self::Split { children, .. } => children.iter().flat_map(Self::frames).collect(),
        }
    }
}

/// One box of the arrangement: a tab strip, and whichever of its panes is in front.
#[derive(Clone, Debug)]
pub struct Frame {
    id: FrameId,
    panes: Vec<PaneId>,
    active_pane: Option<PaneId>,
}

impl Frame {
    /// This frame's name.
    pub fn id(&self) -> FrameId {
        self.id
    }

    /// Its panes, in the order their tabs are drawn.
    pub fn panes(&self) -> &[PaneId] {
        &self.panes
    }

    /// The pane in front, which is the one whose body the frame draws.
    pub fn active_pane(&self) -> Option<PaneId> {
        self.active_pane
    }

    fn new(id: FrameId) -> Self {
        Self {
            id,
            panes: Vec::new(),
            active_pane: None,
        }
    }
}

/// An arrangement of panes: what is open, where, and which of it is in front.
///
/// `P` is whatever a pane is in your application — an enum of kinds, a document handle, an
/// index into your own state. The arrangement holds it and hands it back when it is drawn;
/// none of the layout logic looks inside it.
///
/// Every method that changes the arrangement keeps it coherent: a frame emptied of its last
/// pane is taken out of the tree, the shares of a split always add up, and the workspace
/// always has at least one frame to drop a pane on.
#[derive(Clone, Debug)]
pub struct Layout<P> {
    root: LayoutNode,
    frames: HashMap<FrameId, Frame>,
    panes: HashMap<PaneId, P>,
    active_frame: FrameId,
    /// Names are handed out from here rather than from a global counter, so a restored
    /// arrangement carries on where it left off instead of colliding with what it restored.
    next_id: u64,
}

impl<P> Default for Layout<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> Layout<P> {
    /// An arrangement of one empty frame.
    pub fn new() -> Self {
        let frame = FrameId(1);
        Self {
            root: LayoutNode::Frame { frame },
            frames: HashMap::from([(frame, Frame::new(frame))]),
            panes: HashMap::new(),
            active_frame: frame,
            next_id: 2,
        }
    }

    /// An arrangement of one frame holding one pane.
    pub fn with_pane(pane: P) -> Self {
        let mut layout = Self::new();
        let frame = layout.active_frame;
        layout.add_pane(frame, pane, None);
        layout
    }

    /// The tree, for an application that wants to walk the arrangement itself.
    pub fn root(&self) -> &LayoutNode {
        &self.root
    }

    /// Every frame, in the order it is laid out: left to right, top to bottom.
    pub fn frame_ids(&self) -> Vec<FrameId> {
        self.root.frames()
    }

    /// One frame, if it is part of this arrangement.
    pub fn frame(&self, frame: FrameId) -> Option<&Frame> {
        self.frames.get(&frame)
    }

    /// How many frames there are.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The top-left frame, whose tab strip is the one an application usually hangs its own
    /// controls off.
    pub fn primary_frame(&self) -> FrameId {
        self.frame_ids()
            .first()
            .copied()
            .unwrap_or(self.active_frame)
    }

    /// The frame the keyboard is talking to.
    pub fn active_frame(&self) -> FrameId {
        self.active_frame
    }

    /// Hand the keyboard to a frame.
    pub fn set_active_frame(&mut self, frame: FrameId) {
        if self.frames.contains_key(&frame) {
            self.active_frame = frame;
        }
    }

    /// Hand the keyboard to the next frame, wrapping round at the end.
    ///
    /// Frames come back in the order they are laid out, so this walks the workspace the way it
    /// looks rather than the way its tree happens to be built.
    pub fn focus_next_frame(&mut self) -> FrameId {
        let frames = self.frame_ids();
        let at = frames
            .iter()
            .position(|frame| *frame == self.active_frame)
            .unwrap_or(0);
        if let Some(next) = frames.get((at + 1) % frames.len().max(1)) {
            self.active_frame = *next;
        }
        self.active_frame
    }

    /// Every open pane. The order is not meaningful; a frame's own [`panes`](Frame::panes) is.
    pub fn panes(&self) -> impl Iterator<Item = (PaneId, &P)> {
        self.panes.iter().map(|(id, pane)| (*id, pane))
    }

    /// How many panes are open, across every frame.
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Whether nothing at all is open.
    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    /// One pane, if it is open.
    pub fn pane(&self, pane: PaneId) -> Option<&P> {
        self.panes.get(&pane)
    }

    /// The same, to change.
    pub fn pane_mut(&mut self, pane: PaneId) -> Option<&mut P> {
        self.panes.get_mut(&pane)
    }

    /// Whether a pane is open.
    pub fn contains(&self, pane: PaneId) -> bool {
        self.panes.contains_key(&pane)
    }

    /// The pane in front of the active frame — the one the keyboard is talking to.
    pub fn active_pane(&self) -> Option<(PaneId, &P)> {
        let pane = self.frames.get(&self.active_frame)?.active_pane?;
        Some((pane, self.panes.get(&pane)?))
    }

    /// The first pane the predicate accepts, for "is this already open?" questions.
    pub fn find_pane(&self, mut wanted: impl FnMut(&P) -> bool) -> Option<(PaneId, &P)> {
        self.panes
            .iter()
            .find(|(_, pane)| wanted(pane))
            .map(|(id, pane)| (*id, pane))
    }

    /// The frame holding a pane.
    pub fn frame_of(&self, pane: PaneId) -> Option<FrameId> {
        self.frames
            .values()
            .find(|frame| frame.panes.contains(&pane))
            .map(Frame::id)
    }

    /// The frame that panes like this one already live in: `preferred` if it holds one, else
    /// wherever the others are. This is what keeps shells with shells and documents with
    /// documents as new panes open.
    pub fn frame_holding(
        &self,
        preferred: FrameId,
        mut wanted: impl FnMut(&P) -> bool,
    ) -> Option<FrameId> {
        let mut holds = |frame: FrameId| {
            self.frames.get(&frame).is_some_and(|frame| {
                frame
                    .panes
                    .iter()
                    .any(|pane| self.panes.get(pane).is_some_and(&mut wanted))
            })
        };

        if holds(preferred) {
            return Some(preferred);
        }
        self.frame_ids().into_iter().find(|frame| holds(*frame))
    }

    /// Whether the tree, the frames and the panes agree with each other.
    ///
    /// A restored arrangement is only worth drawing if they do; a half-written or outdated one
    /// is better thrown away than rendered.
    pub fn is_coherent(&self) -> bool {
        let frame_ids = self.frame_ids();
        !frame_ids.is_empty()
            && frame_ids.len() == self.frames.len()
            && frame_ids
                .iter()
                .all(|frame| self.frames.contains_key(frame))
            && self.frames.contains_key(&self.active_frame)
            && self
                .frames
                .values()
                .all(|frame| frame.panes.iter().all(|pane| self.panes.contains_key(pane)))
    }

    /// Keep the arrangement's shape and hand back everything that was in it.
    ///
    /// This is how a stored arrangement is reused across runs: its splits say where the user
    /// put their columns and rows, which is worth keeping, while its panes named documents and
    /// processes that this run has to open for itself. The frames come back empty, ready to be
    /// refilled — and [`drop_empty_frames`](Layout::drop_empty_frames) takes away whatever
    /// nothing landed in.
    pub fn take_panes(&mut self) -> Vec<P> {
        for frame in self.frames.values_mut() {
            frame.panes.clear();
            frame.active_pane = None;
        }
        std::mem::take(&mut self.panes).into_values().collect()
    }

    /// Put a pane in a frame, before one of its tabs or at the end of the strip. It becomes
    /// the pane in front, and its frame becomes the active one.
    pub fn add_pane(&mut self, frame: FrameId, pane: P, before: Option<PaneId>) -> PaneId {
        let id = PaneId(self.take_id());
        self.put_pane(frame, id, pane, before);
        id
    }

    /// Put a pane in a frame of its own, beside an existing frame.
    pub fn add_pane_beside(&mut self, frame: FrameId, side: DropSide, pane: P) -> PaneId {
        let id = PaneId(self.take_id());
        self.put_pane_beside(frame, side, id, pane);
        id
    }

    /// Put a pane in a frame that runs the whole width or height of the workspace, against one
    /// of its outer edges — beside everything already open, rather than beside one frame.
    ///
    /// `share` is how much of the workspace the new column or row takes; the frames already
    /// there give up an equal part of themselves to it. [`DEFAULT_EDGE_SHARE`] is a sensible
    /// one.
    pub fn add_pane_against_edge(&mut self, side: DropSide, share: f32, pane: P) -> PaneId {
        let id = PaneId(self.take_id());
        self.put_pane_against_edge(side, share, id, pane);
        id
    }

    /// Everything [`add_pane`](Layout::add_pane) does, for a pane that already has a name —
    /// which is what a move is, so a dragged tab stays the same tab.
    fn put_pane(&mut self, frame: FrameId, id: PaneId, pane: P, before: Option<PaneId>) {
        let Some(target) = self.frames.get_mut(&frame) else {
            return;
        };

        let at = before
            .and_then(|before| target.panes.iter().position(|pane| *pane == before))
            .unwrap_or(target.panes.len());
        target.panes.insert(at, id);
        target.active_pane = Some(id);

        self.panes.insert(id, pane);
        self.active_frame = frame;
    }

    fn put_pane_beside(&mut self, frame: FrameId, side: DropSide, id: PaneId, pane: P) {
        let Some(direction) = side.direction() else {
            self.put_pane(frame, id, pane, None);
            return;
        };

        let new_frame = FrameId(self.take_id());
        let root = std::mem::replace(&mut self.root, LayoutNode::Frame { frame: new_frame });
        self.root = insert_frame_beside(root, frame, new_frame, direction, side.offset());
        self.frames.insert(new_frame, Frame::new(new_frame));

        self.put_pane(new_frame, id, pane, None);
    }

    fn put_pane_against_edge(&mut self, side: DropSide, share: f32, id: PaneId, pane: P) {
        let Some(direction) = side.direction() else {
            let frame = self.active_frame;
            self.put_pane(frame, id, pane, None);
            return;
        };
        let share = share.clamp(MIN_SPLIT_FRACTION, 1.0 - MIN_SPLIT_FRACTION);
        let new_frame = FrameId(self.take_id());
        let leaf = LayoutNode::Frame { frame: new_frame };
        let at_end = side.offset() == 1;

        let root = std::mem::replace(&mut self.root, LayoutNode::Frame { frame: new_frame });
        self.root = match root {
            // A split already running the right way takes one more child, and the frames
            // already there give up an equal part of themselves to it.
            LayoutNode::Split {
                direction: existing,
                mut children,
                sizes,
            } if existing == direction => {
                let mut sizes: Vec<f32> =
                    sizes.into_iter().map(|size| size * (1.0 - share)).collect();
                if at_end {
                    children.push(leaf);
                    sizes.push(share);
                } else {
                    children.insert(0, leaf);
                    sizes.insert(0, share);
                }
                LayoutNode::Split {
                    direction,
                    children,
                    sizes,
                }
            }
            root => {
                let (children, sizes) = if at_end {
                    (vec![root, leaf], vec![1.0 - share, share])
                } else {
                    (vec![leaf, root], vec![share, 1.0 - share])
                };
                LayoutNode::Split {
                    direction,
                    children,
                    sizes,
                }
            }
        };

        self.frames.insert(new_frame, Frame::new(new_frame));
        self.put_pane(new_frame, id, pane, None);
    }

    /// Bring a pane to the front of its frame, and hand that frame the keyboard.
    pub fn focus_pane(&mut self, pane: PaneId) {
        let Some(frame) = self.frame_of(pane) else {
            return;
        };
        if let Some(frame) = self.frames.get_mut(&frame) {
            frame.active_pane = Some(pane);
        }
        self.active_frame = frame;
    }

    /// Close a pane and hand it back, taking its frame with it if that leaves it empty.
    ///
    /// The last frame stays even when empty, so the workspace always has a drop target.
    pub fn close_pane(&mut self, pane: PaneId) -> Option<P> {
        let frame_id = self.frame_of(pane)?;
        let only_frame_left = self.frames.len() == 1;

        let frame = self.frames.get_mut(&frame_id)?;
        let closed_at = frame.panes.iter().position(|open| *open == pane);
        frame.panes.retain(|open| *open != pane);
        if frame.active_pane == Some(pane) {
            // Focus lands on the tab that slid into the closed one's place, and falls back to
            // the tab before it when the last one in the strip is what closed.
            frame.active_pane = closed_at
                .and_then(|at| frame.panes.get(at).copied())
                .or_else(|| frame.panes.last().copied());
        }
        let frame_is_empty = frame.panes.is_empty();
        let closed = self.panes.remove(&pane);

        if frame_is_empty && !only_frame_left {
            self.remove_frame(frame_id);
        }
        closed
    }

    /// Drop frames that ended up with nothing in them, keeping the last one.
    ///
    /// A restored arrangement contributes its splits, and whatever this run does not put back
    /// into them would otherwise be drawn as an empty frame.
    pub fn drop_empty_frames(&mut self) {
        let empty: Vec<FrameId> = self
            .frames
            .values()
            .filter(|frame| frame.panes.is_empty())
            .map(Frame::id)
            .collect();

        for frame in empty {
            // The last frame stays even when empty, so there is always a drop target.
            if self.frames.len() == 1 {
                break;
            }
            self.remove_frame(frame);
        }
    }

    /// Move a pane onto another frame's tab strip, or against one of its edges — which puts
    /// the pane in a brand new frame beside it.
    pub fn move_pane_to_frame(
        &mut self,
        pane: PaneId,
        frame: FrameId,
        side: DropSide,
        before: Option<PaneId>,
    ) {
        let Some(source) = self.frame_of(pane) else {
            return;
        };
        let source_pane_count = self
            .frames
            .get(&source)
            .map_or(0, |frame| frame.panes.len());

        if side == DropSide::Tabs {
            if source == frame {
                self.reorder_pane(frame, pane, before);
                return;
            }
            let Some(taken) = self.close_pane(pane) else {
                return;
            };
            self.put_pane(frame, pane, taken, before);
            return;
        }

        // A lone tab dropped on its own frame's edge would just rebuild the same frame.
        if source == frame && source_pane_count == 1 {
            return;
        }
        let Some(taken) = self.close_pane(pane) else {
            return;
        };
        self.put_pane_beside(frame, side, pane, taken);
    }

    /// Move an open pane into a frame against one of the workspace's outer edges.
    pub fn move_pane_against_edge(&mut self, pane: PaneId, side: DropSide, share: f32) {
        // A lone pane moved against the edge it already fills would only rebuild what is there.
        if self.frames.len() == 1 && self.panes.len() == 1 {
            return;
        }
        let Some(taken) = self.close_pane(pane) else {
            return;
        };
        self.put_pane_against_edge(side, share, pane, taken);
    }

    /// Set the shares of one split, named by the path from the root that reaches it: the index
    /// of the child taken at each step. The empty path is the root itself.
    ///
    /// No child is left below a tenth of the split, and the shares are rescaled to add up.
    pub fn set_split_sizes(&mut self, path: &[usize], sizes: &[f32]) {
        let root = std::mem::replace(&mut self.root, LayoutNode::Frame { frame: FrameId(0) });
        self.root = set_split_sizes(root, path, sizes);
    }

    /// Bring the tab `step` places along the active frame's strip to the front, wrapping
    /// round at its ends - which is what a key for the next or the previous tab does.
    pub fn focus_tab_after(&mut self, step: isize) -> Option<PaneId> {
        let frame = self.frames.get_mut(&self.active_frame)?;
        let count = frame.panes.len() as isize;
        if count == 0 {
            return None;
        }
        let at = frame
            .active_pane
            .and_then(|pane| frame.panes.iter().position(|open| *open == pane))
            .unwrap_or(0) as isize;
        let pane = frame.panes[(at + step).rem_euclid(count) as usize];
        frame.active_pane = Some(pane);
        Some(pane)
    }

    fn reorder_pane(&mut self, frame: FrameId, pane: PaneId, before: Option<PaneId>) {
        if let Some(frame) = self.frames.get_mut(&frame) {
            frame.panes.retain(|open| *open != pane);
            let at = before
                .and_then(|before| frame.panes.iter().position(|pane| *pane == before))
                .unwrap_or(frame.panes.len());
            frame.panes.insert(at, pane);
            frame.active_pane = Some(pane);
        }
        self.active_frame = frame;
    }

    fn remove_frame(&mut self, frame: FrameId) {
        self.frames.remove(&frame);
        let root = std::mem::replace(&mut self.root, LayoutNode::Frame { frame });
        self.root = remove_frame_node(root, frame);
        let remaining = self.frame_ids();
        if !remaining.contains(&self.active_frame)
            && let Some(first) = remaining.first()
        {
            self.active_frame = *first;
        }
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

fn remove_frame_node(node: LayoutNode, frame: FrameId) -> LayoutNode {
    let LayoutNode::Split {
        direction,
        children,
        sizes,
    } = node
    else {
        return node;
    };

    let at = children
        .iter()
        .position(|child| matches!(child, LayoutNode::Frame { frame: id } if *id == frame));

    let Some(at) = at else {
        return LayoutNode::Split {
            direction,
            children: children
                .into_iter()
                .map(|child| remove_frame_node(child, frame))
                .collect(),
            sizes,
        };
    };

    let mut children = children;
    let mut sizes = sizes;
    children.remove(at);
    if at < sizes.len() {
        sizes.remove(at);
    }
    if children.len() == 1 {
        return children.remove(0);
    }

    LayoutNode::Split {
        direction,
        children,
        sizes: rescale_to_one(sizes),
    }
}

fn insert_frame_beside(
    node: LayoutNode,
    target: FrameId,
    new_frame: FrameId,
    direction: SplitDirection,
    offset: usize,
) -> LayoutNode {
    let leaf = LayoutNode::Frame { frame: new_frame };

    match node {
        LayoutNode::Frame { frame } => {
            if frame != target {
                return LayoutNode::Frame { frame };
            }
            let existing = LayoutNode::Frame { frame };
            let children = if offset == 0 {
                vec![leaf, existing]
            } else {
                vec![existing, leaf]
            };
            LayoutNode::Split {
                direction,
                children,
                sizes: vec![0.5, 0.5],
            }
        }
        LayoutNode::Split {
            direction: node_direction,
            children,
            sizes,
        } => {
            let at = children
                .iter()
                .position(|child| matches!(child, LayoutNode::Frame { frame } if *frame == target));

            let Some(at) = at.filter(|_| node_direction == direction) else {
                return LayoutNode::Split {
                    direction: node_direction,
                    children: children
                        .into_iter()
                        .map(|child| {
                            insert_frame_beside(child, target, new_frame, direction, offset)
                        })
                        .collect(),
                    sizes,
                };
            };

            // Same direction as the surrounding split: the new frame becomes another sibling
            // and takes half of the space the target had.
            let mut children = children;
            let mut sizes = sizes;
            let half = sizes.get(at).copied().unwrap_or(0.5) / 2.0;
            if let Some(size) = sizes.get_mut(at) {
                *size = half;
            }
            children.insert(at + offset, leaf);
            sizes.insert(at + offset, half);

            LayoutNode::Split {
                direction: node_direction,
                children,
                sizes,
            }
        }
    }
}

fn set_split_sizes(node: LayoutNode, path: &[usize], new_sizes: &[f32]) -> LayoutNode {
    let LayoutNode::Split {
        direction,
        children,
        sizes,
    } = node
    else {
        return node;
    };

    if path.is_empty() {
        return LayoutNode::Split {
            direction,
            children,
            sizes: rescale_to_one(
                new_sizes
                    .iter()
                    .map(|size| size.max(MIN_SPLIT_FRACTION))
                    .collect(),
            ),
        };
    }

    let (index, rest) = (path[0], &path[1..]);
    LayoutNode::Split {
        direction,
        children: children
            .into_iter()
            .enumerate()
            .map(|(at, child)| {
                if at == index {
                    set_split_sizes(child, rest, new_sizes)
                } else {
                    child
                }
            })
            .collect(),
        sizes,
    }
}

fn rescale_to_one(sizes: Vec<f32>) -> Vec<f32> {
    let total: f32 = sizes.iter().sum();
    if total <= 0.0 {
        let share = 1.0 / sizes.len().max(1) as f32;
        return sizes.iter().map(|_| share).collect();
    }
    sizes.into_iter().map(|size| size / total).collect()
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
