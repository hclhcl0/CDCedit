// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer Abdullah and Concat contributors

//! The workspace's arrangement: a tree of splits and views, walked flat
//! for Slint, which has no recursion.

use crate::ui::{DockDivider, DockSide, PaneKind, SeatBox};

pub const SEAT_MIN_W: f32 = 240.0;
pub const SEAT_MIN_H: f32 = 140.0;
/// The floor that holds where the one above cannot: enough of a seat left to
/// find its corner button with, which is the only way back from a layout that
/// has been squeezed too far.
pub const SEAT_MIN_GRAB: f32 = 64.0;
/// The gutter between the halves of a split, and the margin round the lot.
pub const SEAT_GAP: f32 = 8.0;

pub enum Dock {
    /// One view, filling its box.
    Leaf(PaneKind),
    /// Two nodes and the gutter between them. `columns` means side by side,
    /// which is a vertical gutter dragged left and right; `ratio` is the share
    /// of the usable extent — the box less the gutter — that the first takes.
    Split {
        columns: bool,
        ratio: f32,
        first: Box<Dock>,
        second: Box<Dock>,
    },
}

impl Dock {
    pub fn leaf(kind: PaneKind) -> Box<Dock> {
        Box::new(Dock::Leaf(kind))
    }

    /// The node a path names, following each step into the first branch or the
    /// second. A path that runs past a leaf stops there, which cannot happen
    /// for a path this module produced.
    pub fn at(&self, path: &[bool]) -> &Dock {
        let Some((step, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Dock::Split { first, second, .. } => {
                if *step {
                    second.at(rest)
                } else {
                    first.at(rest)
                }
            }
            leaf => leaf,
        }
    }

    pub fn at_mut(&mut self, path: &[bool]) -> &mut Dock {
        let Some((step, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Dock::Split { first, second, .. } => {
                if *step {
                    second.at_mut(rest)
                } else {
                    first.at_mut(rest)
                }
            }
            leaf => leaf,
        }
    }

    /// The path to the nth leaf, counted in the same walk `lay_out` uses — so
    /// the number a seat carries and the number a drop reports are the same
    /// number. Paths rather than indices for the editing below, because an
    /// index is invalidated by a change elsewhere in the tree and a path is
    /// only invalidated by a change on its own way down.
    pub fn leaf_path(&self, index: usize) -> Option<Vec<bool>> {
        fn walk(node: &Dock, want: usize, seen: &mut usize, path: &mut Vec<bool>) -> bool {
            match node {
                Dock::Leaf(_) => {
                    if *seen == want {
                        return true;
                    }
                    *seen += 1;
                    false
                }
                Dock::Split { first, second, .. } => {
                    path.push(false);
                    if walk(first, want, seen, path) {
                        return true;
                    }
                    path.pop();
                    path.push(true);
                    if walk(second, want, seen, path) {
                        return true;
                    }
                    path.pop();
                    false
                }
            }
        }
        let (mut seen, mut path) = (0, Vec::new());
        walk(self, index, &mut seen, &mut path).then_some(path)
    }

    /// The same for the nth split — parent before children, which is the order
    /// `lay_out` numbers the gutters in.
    pub fn split_path(&self, index: usize) -> Option<Vec<bool>> {
        fn walk(node: &Dock, want: usize, seen: &mut usize, path: &mut Vec<bool>) -> bool {
            let Dock::Split { first, second, .. } = node else {
                return false;
            };
            if *seen == want {
                return true;
            }
            *seen += 1;
            path.push(false);
            if walk(first, want, seen, path) {
                return true;
            }
            path.pop();
            path.push(true);
            if walk(second, want, seen, path) {
                return true;
            }
            path.pop();
            false
        }
        let (mut seen, mut path) = (0, Vec::new());
        walk(self, index, &mut seen, &mut path).then_some(path)
    }

    /// Whether a view of this kind is anywhere in the tree.
    pub fn contains(&self, kind: PaneKind) -> bool {
        match self {
            Dock::Leaf(held) => *held == kind,
            Dock::Split { first, second, .. } => first.contains(kind) || second.contains(kind),
        }
    }

    pub fn kind_at(&self, index: usize) -> Option<PaneKind> {
        match self.at(&self.leaf_path(index)?) {
            Dock::Leaf(kind) => Some(*kind),
            _ => None,
        }
    }

    /// Turn one leaf into a split, with a new view on the named side of it.
    /// Half and half: the seat being split is the only thing that knows how
    /// much room there is, and half of it is the answer that needs no rule.
    pub fn split_leaf(&mut self, path: &[bool], kind: PaneKind, side: DockSide) {
        let node = self.at_mut(path);
        let existing = Box::new(std::mem::replace(node, Dock::Leaf(kind)));
        let (columns, first, second) = match side {
            DockSide::Left => (true, Dock::leaf(kind), existing),
            DockSide::Right => (true, existing, Dock::leaf(kind)),
            DockSide::Top => (false, Dock::leaf(kind), existing),
            _ => (false, existing, Dock::leaf(kind)),
        };
        *node = Dock::Split {
            columns,
            ratio: 0.5,
            first,
            second,
        };
    }

    /// Take a leaf out, and let its sibling take the space — the split it was
    /// half of stops existing. A path with nothing above it is the whole tree,
    /// which cannot be removed: a window with no seats has nowhere to put
    /// anything back.
    pub fn remove_leaf(&mut self, path: &[bool]) {
        let Some((last, above)) = path.split_last() else {
            return;
        };
        let parent = self.at_mut(above);
        let survivor = match parent {
            Dock::Split { first, second, .. } => {
                let keep = if *last { first } else { second };
                std::mem::replace(keep.as_mut(), Dock::Leaf(PaneKind::Media))
            }
            _ => return,
        };
        *parent = survivor;
    }
}

/// The tree walked flat: a box per view, a box per gutter.
#[derive(Default)]
pub struct DockLayout {
    pub seats: Vec<SeatBox>,
    pub dividers: Vec<DockDivider>,
    /// The usable extent of each split — its box less the gutter — in the same
    /// order as `dividers`. A divider drag is a delta in pixels and a ratio is
    /// a fraction, and this is what converts between them.
    pub extents: Vec<f32>,
}

/// A temporary view over the dock. The original tree and seat identities stay
/// intact, so expanding Flow never discards input focus, drafts or split ratios.
pub struct FlowWorkspace {
    pub mode: i32,
    pub seat: usize,
    pub preview_ratio: f32,
}
impl Default for FlowWorkspace {
    fn default() -> Self {
        Self {
            mode: 0,
            seat: 0,
            preview_ratio: 0.3,
        }
    }
}
impl FlowWorkspace {
    pub fn apply(&self, out: &mut DockLayout, width: f32, height: f32) {
        if self.mode == 0
            || out
                .seats
                .get(self.seat)
                .is_none_or(|s| s.kind != PaneKind::Inspector)
        {
            return;
        }
        let w = (width - SEAT_GAP * 2.0).max(0.0);
        let h = (height - SEAT_GAP * 2.0).max(0.0);
        let preview = (self.mode == 1 && w >= 700.0)
            .then(|| out.seats.iter().position(|s| s.kind == PaneKind::Preview))
            .flatten();
        for seat in &mut out.seats {
            seat.width = 0.0;
            seat.height = 0.0;
        }
        out.dividers.clear();
        out.extents.clear();
        let mut left = SEAT_GAP;
        let mut flow_width = w;
        if let Some(index) = preview {
            let usable = (w - SEAT_GAP).max(0.0);
            let head = (usable * self.preview_ratio).clamp(240.0, usable - 440.0);
            let seat = &mut out.seats[index];
            seat.x = SEAT_GAP;
            seat.y = SEAT_GAP;
            seat.width = head;
            seat.height = h;
            out.dividers.push(DockDivider {
                index: 0,
                columns: true,
                x: SEAT_GAP + head,
                y: SEAT_GAP,
                width: SEAT_GAP,
                height: h,
            });
            out.extents.push(usable);
            left += head + SEAT_GAP;
            flow_width = usable - head;
        }
        let seat = &mut out.seats[self.seat];
        seat.x = left;
        seat.y = SEAT_GAP;
        seat.width = flow_width;
        seat.height = h;
    }
}

pub fn lay_out(node: &Dock, (x, y, w, h): (f32, f32, f32, f32), out: &mut DockLayout) {
    match node {
        Dock::Leaf(kind) => out.seats.push(SeatBox {
            index: out.seats.len() as i32,
            kind: *kind,
            x,
            y,
            width: w.max(0.0),
            height: h.max(0.0),
        }),
        Dock::Split {
            columns,
            ratio,
            first,
            second,
        } => {
            // The slot is taken before the children are walked, so a gutter's
            // number is its split's position in a parent-first walk — which is
            // what `split_path` counts and what a drag reports back.
            let index = out.dividers.len();
            out.dividers.push(DockDivider::default());
            out.extents.push(0.0);

            let along = if *columns { w } else { h };
            let usable = (along - SEAT_GAP).max(0.0);
            let head = (usable * ratio).clamp(0.0, usable);
            out.extents[index] = usable;
            out.dividers[index] = if *columns {
                DockDivider {
                    index: index as i32,
                    columns: true,
                    x: x + head,
                    y,
                    width: SEAT_GAP,
                    height: h.max(0.0),
                }
            } else {
                DockDivider {
                    index: index as i32,
                    columns: false,
                    x,
                    y: y + head,
                    width: w.max(0.0),
                    height: SEAT_GAP,
                }
            };

            if *columns {
                lay_out(first, (x, y, head, h), out);
                lay_out(second, (x + head + SEAT_GAP, y, usable - head, h), out);
            } else {
                lay_out(first, (x, y, w, head), out);
                lay_out(second, (x, y + head + SEAT_GAP, w, usable - head), out);
            }
        }
    }
}

/// The arrangement the editor opens with: the inspector a column down the
/// whole right side, and in the rest the library and the monitor across the
/// top and the timeline under both of them. The inspector is the tallest
/// thing in the window - a stack of sections that would otherwise scroll -
/// and the timeline gives up its right end to it, which is the end with
/// nothing on it yet. About 0.22 of the width to the inspector and 0.25 to
/// the library, which is 0.32 of what is left once the inspector has had
/// its share; the timeline takes the lower 0.4 of the height.
pub fn default_dock() -> Dock {
    Dock::Split {
        columns: true,
        ratio: 0.81,
        first: Box::new(Dock::Split {
            columns: false,
            ratio: 0.58,
            first: Box::new(Dock::Split {
                columns: true,
                ratio: 0.38,
                first: Dock::leaf(PaneKind::Media),
                second: Dock::leaf(PaneKind::Preview),
            }),
            second: Dock::leaf(PaneKind::Timeline),
        }),
        second: Dock::leaf(PaneKind::Inspector),
    }
}

/// The top edge of a row, measured down the stack from the first lane.
/// The dock a narrow window shows: the monitor over the timeline, and
/// nothing beside either. The library and the inspector are a switch away
/// on each seat - see `PaneSwitcher` - which on a phone is how they are
/// reached in every editor: one at a time, in the space the picture had.
pub fn compact_dock() -> Dock {
    Dock::Split {
        columns: false,
        ratio: 0.5,
        first: Dock::leaf(PaneKind::Preview),
        second: Dock::leaf(PaneKind::Timeline),
    }
}

pub fn row_top(heights: &[f32], row: i32) -> f32 {
    heights.iter().take(row.max(0) as usize).sum()
}

/// The row a point down the stack falls in, clamped to the stack.
pub fn row_at(heights: &[f32], y: f32) -> i32 {
    let mut top = 0.0;
    for (row, height) in heights.iter().enumerate() {
        top += height;
        if y < top {
            return row as i32;
        }
    }
    (heights.len() as i32 - 1).max(0)
}

/// The row whose top edge is nearest a point down the stack.
///
/// What a *move* wants, and not the same question as `row_at`: dragging a clip
/// halfway into the lane below should put it there, which is the rounding the
/// old `delta / lane-height` did for free back when every lane was one height.
pub fn nearest_row(heights: &[f32], y: f32) -> i32 {
    let (mut top, mut best, mut best_distance) = (0.0_f32, 0_i32, f32::MAX);
    for (row, height) in heights.iter().enumerate() {
        let distance = (top - y).abs();
        if distance < best_distance {
            best_distance = distance;
            best = row as i32;
        }
        top += height;
    }
    best
}

#[cfg(test)]
mod flow_workspace_tests {
    use super::*;

    fn layout(tree: &Dock, focus: &FlowWorkspace, width: f32) -> DockLayout {
        let mut out = DockLayout::default();
        lay_out(tree, (8.0, 8.0, width - 16.0, 704.0), &mut out);
        focus.apply(&mut out, width, 720.0);
        out
    }

    #[test]
    fn flow_expansion_preserves_seat_identity_and_restores_the_original_dock() {
        let tree = default_dock();
        let original = layout(&tree, &FlowWorkspace::default(), 1440.0);
        let mut focus = FlowWorkspace {
            mode: 1,
            seat: 3,
            preview_ratio: 0.3,
        };
        let wide = layout(&tree, &focus, 1440.0);
        assert_eq!(wide.seats.len(), original.seats.len());
        for (before, after) in original.seats.iter().zip(&wide.seats) {
            assert_eq!(before.index, after.index);
            assert_eq!(before.kind, after.kind);
        }
        assert!(wide.seats[3].width > 900.0);
        assert!(wide.seats[1].width >= 240.0);
        assert_eq!(wide.seats[0].width, 0.0);
        assert_eq!(wide.dividers.len(), 1);
        focus.mode = 2;
        let full = layout(&tree, &focus, 1440.0);
        assert_eq!(full.seats[3].width, 1424.0);
        assert!(full.dividers.is_empty());
        focus.mode = 0;
        let restored = layout(&tree, &focus, 1440.0);
        for (before, after) in original.seats.iter().zip(restored.seats) {
            assert_eq!(
                (before.x, before.y, before.width, before.height),
                (after.x, after.y, after.width, after.height)
            );
        }
    }

    #[test]
    fn wide_flow_adapts_to_narrow_windows_and_missing_preview_without_invalid_bounds() {
        let tree = default_dock();
        let mut focus = FlowWorkspace {
            mode: 1,
            seat: 3,
            preview_ratio: 0.99,
        };
        let narrow = layout(&tree, &focus, 640.0);
        assert_eq!(narrow.seats[3].width, 624.0);
        assert!(narrow.dividers.is_empty());
        let clamped = layout(&tree, &focus, 1000.0);
        assert!(clamped.seats[3].width >= 440.0);
        focus.seat = 0;
        let only = layout(&Dock::Leaf(PaneKind::Inspector), &focus, 1000.0);
        assert_eq!(only.seats[0].width, 984.0);
        assert!(only.dividers.is_empty());
    }
}
