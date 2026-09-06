/// Terminal Split Panes — Split the terminal into multiple panes
///
/// Provides:
///   - Horizontal and vertical splits
///   - Pane tree (binary tree of splits)
///   - Focus navigation between panes
///   - Pane resize
///   - Pane close/collapse
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// PANE TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal, // side by side
    Vertical,   // top and bottom
}

/// A unique pane ID
static NEXT_PANE_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

fn alloc_pane_id() -> u32 {
    NEXT_PANE_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

/// Pane area (in terminal character coordinates)
#[derive(Debug, Clone, Copy)]
pub struct PaneRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PaneRect {
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self {
            x,
            y,
            width: w,
            height: h,
        }
    }

    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// A leaf pane (actual terminal)
#[derive(Debug, Clone)]
pub struct Pane {
    pub id: u32,
    pub rect: PaneRect,
    pub title: String,
    /// Terminal buffer index this pane is connected to
    pub terminal_id: u32,
    pub focused: bool,
}

impl Pane {
    pub fn new(rect: PaneRect) -> Self {
        Self {
            id: alloc_pane_id(),
            rect,
            title: String::from("Terminal"),
            terminal_id: 0,
            focused: false,
        }
    }
}

/// Pane tree node
pub enum PaneNode {
    Leaf(Pane),
    Split {
        direction: SplitDirection,
        /// Split ratio (0.0 to 1.0, where the ratio is for the first child)
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    /// Get all leaf panes
    pub fn leaves(&self) -> Vec<&Pane> {
        match self {
            PaneNode::Leaf(pane) => alloc::vec![pane],
            PaneNode::Split { first, second, .. } => {
                let mut v = first.leaves();
                v.extend(second.leaves());
                v
            }
        }
    }

    /// Get all leaf panes mutably
    pub fn leaves_mut(&mut self) -> Vec<&mut Pane> {
        match self {
            PaneNode::Leaf(pane) => alloc::vec![pane],
            PaneNode::Split { first, second, .. } => {
                let mut v = first.leaves_mut();
                v.extend(second.leaves_mut());
                v
            }
        }
    }

    /// Find a pane by ID
    pub fn find(&self, id: u32) -> Option<&Pane> {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id == id {
                    Some(pane)
                } else {
                    None
                }
            }
            PaneNode::Split { first, second, .. } => first.find(id).or_else(|| second.find(id)),
        }
    }

    /// Recalculate pane rectangles after split/resize
    pub fn layout(&mut self, rect: PaneRect) {
        match self {
            PaneNode::Leaf(pane) => {
                pane.rect = rect;
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                match direction {
                    SplitDirection::Horizontal => {
                        let first_w = ((rect.width as f32) * *ratio) as u32;
                        let second_w = rect.width - first_w - 1; // 1 for divider
                        first.layout(PaneRect::new(rect.x, rect.y, first_w, rect.height));
                        second.layout(PaneRect::new(
                            rect.x + first_w + 1,
                            rect.y,
                            second_w,
                            rect.height,
                        ));
                    }
                    SplitDirection::Vertical => {
                        let first_h = ((rect.height as f32) * *ratio) as u32;
                        let second_h = rect.height - first_h - 1; // 1 for divider
                        first.layout(PaneRect::new(rect.x, rect.y, rect.width, first_h));
                        second.layout(PaneRect::new(
                            rect.x,
                            rect.y + first_h + 1,
                            rect.width,
                            second_h,
                        ));
                    }
                }
            }
        }
    }

    /// Count leaf panes
    pub fn count(&self) -> usize {
        match self {
            PaneNode::Leaf(_) => 1,
            PaneNode::Split { first, second, .. } => first.count() + second.count(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PANE MANAGER
// ═══════════════════════════════════════════════════════════════════════

pub struct PaneManager {
    pub root: Option<PaneNode>,
    pub total_rect: PaneRect,
    pub focused_pane_id: Option<u32>,
}

impl PaneManager {
    pub fn new(width: u32, height: u32) -> Self {
        let rect = PaneRect::new(0, 0, width, height);
        let pane = Pane::new(rect);
        let id = pane.id;

        Self {
            root: Some(PaneNode::Leaf(pane)),
            total_rect: rect,
            focused_pane_id: Some(id),
        }
    }

    /// Split the focused pane
    pub fn split(&mut self, direction: SplitDirection) -> Option<u32> {
        let focused_id = self.focused_pane_id?;
        let root = self.root.take()?;
        let (new_root, new_id) = Self::split_node(root, focused_id, direction);
        self.root = Some(new_root);
        self.relayout();
        new_id
    }

    fn split_node(
        node: PaneNode,
        target_id: u32,
        direction: SplitDirection,
    ) -> (PaneNode, Option<u32>) {
        match node {
            PaneNode::Leaf(pane) if pane.id == target_id => {
                let new_pane = Pane::new(pane.rect);
                let new_id = new_pane.id;
                let split = PaneNode::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(PaneNode::Leaf(pane)),
                    second: Box::new(PaneNode::Leaf(new_pane)),
                };
                (split, Some(new_id))
            }
            PaneNode::Split {
                direction: d,
                ratio,
                first,
                second,
            } => {
                let (new_first, id1) = Self::split_node(*first, target_id, direction);
                if id1.is_some() {
                    return (
                        PaneNode::Split {
                            direction: d,
                            ratio,
                            first: Box::new(new_first),
                            second,
                        },
                        id1,
                    );
                }
                let (new_second, id2) = Self::split_node(*second, target_id, direction);
                (
                    PaneNode::Split {
                        direction: d,
                        ratio,
                        first: Box::new(new_first),
                        second: Box::new(new_second),
                    },
                    id2,
                )
            }
            other => (other, None),
        }
    }

    /// Close a pane by ID
    pub fn close_pane(&mut self, id: u32) -> bool {
        if let Some(root) = self.root.take() {
            if let Some(new_root) = Self::remove_pane(root, id) {
                self.root = Some(new_root);
                self.relayout();

                // Update focus if we closed the focused pane
                if self.focused_pane_id == Some(id) {
                    if let Some(ref root) = self.root {
                        self.focused_pane_id = root.leaves().first().map(|p| p.id);
                    }
                }
                return true;
            }
        }
        false
    }

    fn remove_pane(node: PaneNode, target_id: u32) -> Option<PaneNode> {
        match node {
            PaneNode::Leaf(pane) if pane.id == target_id => None,
            PaneNode::Split { first, second, .. } => match (*first, *second) {
                (PaneNode::Leaf(p1), PaneNode::Leaf(p2)) => {
                    if p1.id == target_id {
                        Some(PaneNode::Leaf(p2))
                    } else if p2.id == target_id {
                        Some(PaneNode::Leaf(p1))
                    } else {
                        Some(PaneNode::Split {
                            direction: SplitDirection::Horizontal,
                            ratio: 0.5,
                            first: Box::new(PaneNode::Leaf(p1)),
                            second: Box::new(PaneNode::Leaf(p2)),
                        })
                    }
                }
                (f, s) => {
                    let direction = SplitDirection::Horizontal;
                    let ratio = 0.5;
                    if let Some(new_f) = Self::remove_pane(f, target_id) {
                        Some(PaneNode::Split {
                            direction,
                            ratio,
                            first: Box::new(new_f),
                            second: Box::new(s),
                        })
                    } else {
                        Some(s)
                    }
                }
            },
            other => Some(other),
        }
    }

    /// Focus the next pane
    pub fn focus_next(&mut self) {
        if let Some(ref root) = self.root {
            let leaves = root.leaves();
            if let Some(focused) = self.focused_pane_id {
                let idx = leaves.iter().position(|p| p.id == focused).unwrap_or(0);
                let next = (idx + 1) % leaves.len();
                self.focused_pane_id = Some(leaves[next].id);
            }
        }
    }

    /// Focus the previous pane
    pub fn focus_prev(&mut self) {
        if let Some(ref root) = self.root {
            let leaves = root.leaves();
            if let Some(focused) = self.focused_pane_id {
                let idx = leaves.iter().position(|p| p.id == focused).unwrap_or(0);
                let prev = if idx == 0 { leaves.len() - 1 } else { idx - 1 };
                self.focused_pane_id = Some(leaves[prev].id);
            }
        }
    }

    /// Recalculate all pane layouts
    pub fn relayout(&mut self) {
        if let Some(ref mut root) = self.root {
            root.layout(self.total_rect);
        }
    }

    /// Resize total area
    pub fn resize(&mut self, width: u32, height: u32) {
        self.total_rect = PaneRect::new(0, 0, width, height);
        self.relayout();
    }

    /// Get focused pane
    pub fn focused_pane(&self) -> Option<&Pane> {
        let id = self.focused_pane_id?;
        self.root.as_ref()?.find(id)
    }

    /// Get pane count
    pub fn pane_count(&self) -> usize {
        self.root.as_ref().map(|r| r.count()).unwrap_or(0)
    }
}

lazy_static::lazy_static! {
    pub static ref PANE_MANAGER: Mutex<PaneManager> = Mutex::new(PaneManager::new(80, 24));
}

/// Initialize pane system
pub fn init() {
    serial_println!("[KnoxOS] Terminal split panes initialized");
}
