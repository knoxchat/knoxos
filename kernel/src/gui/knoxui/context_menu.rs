use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};
/// ContextMenu — Right-click context menu overlay.
///
/// ```ignore
/// // Attach to a region:
/// ContextMenu::for_response(&widget_resp, "ctx_menu")
///     .item("Cut", "Ctrl+X", || { /* cut */ })
///     .item("Copy", "Ctrl+C", || { /* copy */ })
///     .separator()
///     .item("Paste", "Ctrl+V", || { /* paste */ })
///     .show(ui);
/// ```
use alloc::string::String;
use alloc::vec::Vec;

enum ContextMenuEntry {
    Item {
        label: String,
        shortcut: Option<String>,
        enabled: bool,
    },
    Separator,
}

pub struct ContextMenu {
    id: Id,
    entries: Vec<ContextMenuEntry>,
    trigger_rect: Rect,
    min_width: u32,
}

impl ContextMenu {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            entries: Vec::new(),
            trigger_rect: Rect::new(0, 0, 0, 0),
            min_width: 160,
        }
    }

    /// Create a context menu triggered by right-clicking on a response.
    pub fn for_response(resp: &Response, id_salt: &str) -> Self {
        let mut m = Self::new(id_salt);
        m.trigger_rect = resp.rect;
        m
    }

    pub fn item(mut self, label: &str, shortcut: &str) -> Self {
        self.entries.push(ContextMenuEntry::Item {
            label: String::from(label),
            shortcut: if shortcut.is_empty() {
                None
            } else {
                Some(String::from(shortcut))
            },
            enabled: true,
        });
        self
    }

    pub fn item_disabled(mut self, label: &str) -> Self {
        self.entries.push(ContextMenuEntry::Item {
            label: String::from(label),
            shortcut: None,
            enabled: false,
        });
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(ContextMenuEntry::Separator);
        self
    }

    pub fn min_width(mut self, w: u32) -> Self {
        self.min_width = w;
        self
    }

    /// Show the context menu. Returns `Some(index)` of the item clicked (separator-excluded counting).
    pub fn show(self, ui: &mut Ui) -> Option<usize> {
        let id = self.id;
        let open_id = id.with("ctx_open");
        let pos_x_id = id.with("ctx_x");
        let pos_y_id = id.with("ctx_y");

        // Check for right-click to open
        let trigger_resp = ui.interact(self.trigger_rect, id.with("trigger"), true, false);
        if trigger_resp.secondary_clicked {
            let mut mem = UI_MEMORY.lock();
            mem.set_bool(open_id, true);
            mem.set_i32(pos_x_id, ui.input.pointer_x);
            mem.set_i32(pos_y_id, ui.input.pointer_y);
        }

        let mem = UI_MEMORY.lock();
        let is_open = mem.get_bool(open_id, false);
        let menu_x = mem.get_i32(pos_x_id, 0);
        let menu_y = mem.get_i32(pos_y_id, 0);
        drop(mem);

        if !is_open {
            return None;
        }

        // Calculate size
        let item_h = 24u32;
        let sep_h = 8u32;
        let padding = 4u32;
        let mut total_h = padding * 2;
        let mut max_label_w = self.min_width;

        for entry in &self.entries {
            match entry {
                ContextMenuEntry::Item {
                    label, shortcut, ..
                } => {
                    let lw = label.len() as u32 * 8;
                    let sw = shortcut
                        .as_ref()
                        .map(|s| s.len() as u32 * 8 + 24)
                        .unwrap_or(0);
                    max_label_w = max_label_w.max(lw + sw + 24);
                    total_h += item_h;
                }
                ContextMenuEntry::Separator => {
                    total_h += sep_h;
                }
            }
        }

        let menu_rect = Rect::new(menu_x, menu_y, max_label_w, total_h);

        // Shadow
        let shadow_rect = Rect::new(menu_x + 2, menu_y + 2, max_label_w, total_h);
        ui.fb
            .fill_rounded_rect_aa(shadow_rect, Pixel::new(0, 0, 0, 80), 6);

        // Background
        ui.fb
            .fill_rounded_rect_aa(menu_rect, colors::SURFACE_OVERLAY, 6);
        ui.fb
            .draw_rounded_rect(menu_rect, colors::SURFACE_BORDER, 6, 1);

        let mut y = menu_y + padding as i32;
        let mut clicked_item: Option<usize> = None;
        let mut item_index = 0usize;

        for entry in &self.entries {
            match entry {
                ContextMenuEntry::Item {
                    label,
                    shortcut,
                    enabled,
                } => {
                    let item_rect = Rect::new(menu_x + 2, y, max_label_w - 4, item_h);
                    let item_id = id.with_index(item_index);
                    let resp = ui.interact(item_rect, item_id, true, false);

                    if *enabled && resp.hovered {
                        ui.fb
                            .fill_rounded_rect_aa(item_rect, Pixel::new(255, 255, 255, 15), 4);
                    }

                    let text_col = if *enabled {
                        if resp.hovered {
                            colors::TEXT_PRIMARY
                        } else {
                            colors::TEXT_SECONDARY
                        }
                    } else {
                        colors::TEXT_DISABLED
                    };

                    fonts::draw_string_compact(ui.fb, label, menu_x + 12, y + 7, text_col);

                    if let Some(sc) = shortcut {
                        fonts::draw_string_compact(
                            ui.fb,
                            sc,
                            menu_x + max_label_w as i32 - sc.len() as i32 * 8 - 12,
                            y + 7,
                            colors::TEXT_MUTED,
                        );
                    }

                    if *enabled && resp.clicked {
                        clicked_item = Some(item_index);
                        // Close menu
                        let mut mem = UI_MEMORY.lock();
                        mem.set_bool(open_id, false);
                    }

                    y += item_h as i32;
                    item_index += 1;
                }
                ContextMenuEntry::Separator => {
                    let sy = y + sep_h as i32 / 2;
                    ui.fb.draw_line_aa(
                        menu_x + 8,
                        sy,
                        menu_x + max_label_w as i32 - 8,
                        sy,
                        Pixel::new(255, 255, 255, 20),
                    );
                    y += sep_h as i32;
                }
            }
        }

        // Close on click outside
        if ui.input.pointer_primary_pressed {
            let mx = ui.input.pointer_x;
            let my = ui.input.pointer_y;
            if !menu_rect.contains(mx, my) {
                let mut mem = UI_MEMORY.lock();
                mem.set_bool(open_id, false);
            }
        }

        clicked_item
    }
}
