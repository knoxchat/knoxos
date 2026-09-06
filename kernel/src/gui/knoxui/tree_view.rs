/// TreeView — Hierarchical tree widget with expand/collapse, icons, and selection
///
/// ```ignore
/// let tree = TreeNode::new("root")
///     .child(TreeNode::leaf("file1.txt"))
///     .child(TreeNode::new("src")
///         .child(TreeNode::leaf("main.rs"))
///         .child(TreeNode::leaf("lib.rs")));
///
/// TreeView::new("file_tree").show(ui, &tree, &mut selected);
/// ```
use alloc::string::String;
use alloc::vec::Vec;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};

/// A node in a tree structure.
#[derive(Clone, Debug)]
pub struct TreeNode {
    pub label: String,
    pub children: Vec<TreeNode>,
    pub icon: Option<String>,
    pub id_salt: String,
}

impl TreeNode {
    /// Create a branch node (can have children).
    pub fn new(label: &str) -> Self {
        Self {
            label: String::from(label),
            children: Vec::new(),
            icon: None,
            id_salt: String::from(label),
        }
    }

    /// Create a leaf node (no children).
    pub fn leaf(label: &str) -> Self {
        Self {
            label: String::from(label),
            children: Vec::new(),
            icon: None,
            id_salt: String::from(label),
        }
    }

    pub fn child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn icon(mut self, i: &str) -> Self {
        self.icon = Some(String::from(i));
        self
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

/// Tree view widget.
pub struct TreeView {
    id: Id,
    indent: i32,
    row_height: u32,
    show_lines: bool,
    line_color: Pixel,
}

impl TreeView {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            indent: 16,
            row_height: 22,
            show_lines: true,
            line_color: Pixel::rgb(50, 55, 65),
        }
    }

    pub fn indent(mut self, i: i32) -> Self {
        self.indent = i;
        self
    }
    pub fn row_height(mut self, h: u32) -> Self {
        self.row_height = h;
        self
    }
    pub fn show_lines(mut self, v: bool) -> Self {
        self.show_lines = v;
        self
    }

    /// Show the tree. `selected` is updated when a node is clicked.
    pub fn show<'a>(&self, ui: &mut Ui<'a>, root: &TreeNode, selected: &mut Option<String>) {
        self.show_node(ui, root, 0, selected);
    }

    fn show_node<'a>(
        &self,
        ui: &mut Ui<'a>,
        node: &TreeNode,
        depth: usize,
        selected: &mut Option<String>,
    ) {
        let node_id = self.id.with(&node.id_salt).with_index(depth);
        let avail_w = ui.available_width().max(0) as u32;
        let indent_px = depth as i32 * self.indent;

        let row_rect = ui.allocate_space(avail_w, self.row_height);
        let resp = ui.interact(row_rect, node_id, true, false);

        let is_selected = selected.as_deref() == Some(&node.label);

        // Selection / hover highlight
        if is_selected {
            ui.fb
                .fill_rounded_rect_aa(row_rect, Pixel::rgb(40, 80, 160), 3);
        } else if resp.hovered {
            ui.fb
                .fill_rounded_rect_aa(row_rect, ui.style().widget_bg_hovered, 3);
        }

        // Click to select
        if resp.clicked() {
            *selected = Some(node.label.clone());
        }

        let mut tx = row_rect.x + indent_px + 4;

        // Tree lines
        if self.show_lines && depth > 0 {
            let line_x = row_rect.x + (depth as i32 - 1) * self.indent + self.indent / 2 + 4;
            ui.fb
                .draw_vline(line_x, row_rect.y, self.row_height / 2, self.line_color);
            ui.fb.draw_hline(
                line_x,
                row_rect.y + self.row_height as i32 / 2,
                (self.indent / 2) as u32,
                self.line_color,
            );
        }

        // Expand/collapse indicator for branches
        if !node.is_leaf() {
            let is_open = {
                let mem = UI_MEMORY.lock();
                mem.get_bool(node_id.with("open"), true)
            };

            // Toggle on click
            if resp.clicked() {
                let mut mem = UI_MEMORY.lock();
                mem.set_bool(node_id.with("open"), !is_open);
            }

            // Draw indicator
            let ix = tx + 2;
            let iy = row_rect.y + self.row_height as i32 / 2;
            let ind_color = if is_open {
                ui.style().accent
            } else {
                ui.style().text_dimmed
            };

            if is_open {
                for dy in 0..4i32 {
                    ui.fb
                        .draw_hline(ix + 2 - dy, iy - 1 + dy, (dy * 2 + 1) as u32, ind_color);
                }
            } else {
                for dx in 0..4i32 {
                    ui.fb
                        .draw_vline(ix + dx, iy - dx, (dx * 2 + 1) as u32, ind_color);
                }
            }
            tx += 12;

            // Icon
            if let Some(ref icon) = node.icon {
                fonts::draw_string_compact(
                    ui.fb,
                    icon,
                    tx,
                    row_rect.y + (self.row_height as i32 - 12) / 2,
                    ui.style().text_dimmed,
                );
                tx += (icon.len() as i32 + 1) * 8;
            }

            // Label
            let text_color = if is_selected {
                colors::WHITE
            } else {
                ui.style().text_color
            };
            fonts::draw_string_compact(
                ui.fb,
                &node.label,
                tx,
                row_rect.y + (self.row_height as i32 - 12) / 2,
                text_color,
            );

            // Children (if open)
            if is_open {
                // Draw vertical continuation line
                if self.show_lines {
                    let line_x = row_rect.x + depth as i32 * self.indent + self.indent / 2 + 4;
                    // Line extends through all children — drawn per child
                }
                for child in &node.children {
                    self.show_node(ui, child, depth + 1, selected);
                }
            }
        } else {
            // Leaf node — icon + label
            if let Some(ref icon) = node.icon {
                fonts::draw_string_compact(
                    ui.fb,
                    icon,
                    tx,
                    row_rect.y + (self.row_height as i32 - 12) / 2,
                    ui.style().text_dimmed,
                );
                tx += (icon.len() as i32 + 1) * 8;
            }

            let text_color = if is_selected {
                colors::WHITE
            } else {
                ui.style().text_color
            };
            fonts::draw_string_compact(
                ui.fb,
                &node.label,
                tx,
                row_rect.y + (self.row_height as i32 - 12) / 2,
                text_color,
            );
        }
    }
}
