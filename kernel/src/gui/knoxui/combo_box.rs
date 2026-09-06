// ─── ComboBox — Drop-down selection widget ───────────────────────────
//
// Inspired by egui::ComboBox. Displays the currently selected text and
// opens a popup list when clicked, allowing the user to pick from a set
// of options.

use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::{UI_MEMORY, Ui};

use super::text_helpers as fonts;

/// Result of showing a combo box.
pub struct ComboBoxResponse {
    /// The index of the newly selected item, if the selection changed.
    pub changed: bool,
    /// Which index was clicked this frame, if any.
    pub selected_index: usize,
    /// The interaction response.
    pub response: crate::gui::response::Response,
}

/// A drop-down selection widget.
///
/// ```ignore
/// let items = &["Apple", "Banana", "Cherry"];
/// ComboBox::new("fruit_select", items, &mut selected)
///     .width(180)
///     .show(ui);
/// ```
pub struct ComboBox<'a> {
    id_str: &'a str,
    items: &'a [&'a str],
    selected: &'a mut usize,
    width: u32,
    height: u32,
    max_popup_height: u32,
    label: Option<&'a str>,
}

impl<'a> ComboBox<'a> {
    pub fn new(id_str: &'a str, items: &'a [&'a str], selected: &'a mut usize) -> Self {
        Self {
            id_str,
            items,
            selected,
            width: 160,
            height: 28,
            max_popup_height: 200,
            label: None,
        }
    }

    pub fn width(mut self, w: u32) -> Self {
        self.width = w;
        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.height = h;
        self
    }

    pub fn max_popup_height(mut self, h: u32) -> Self {
        self.max_popup_height = h;
        self
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn show(self, ui: &mut Ui) -> ComboBoxResponse {
        let id = ui.id.with(self.id_str);
        let open_key = id.with("open");

        // Optional label
        if let Some(lbl) = self.label {
            let label_rect = ui.allocate_space((lbl.len() as u32 * 8) + 8, self.height);
            fonts::draw_string_compact(
                ui.fb,
                lbl,
                label_rect.x,
                label_rect.y + (self.height as i32 - 10) / 2,
                ui.style().text_color,
            );
        }

        let rect = ui.allocate_space(self.width, self.height);
        let resp = ui.interact(rect, id, true, false);

        let mut mem = UI_MEMORY.lock();
        let mut is_open = mem.get_bool(open_key, false);

        if resp.clicked() {
            is_open = !is_open;
            mem.set_bool(open_key, is_open);
        }
        drop(mem);

        // ── Draw the combo box button ──
        let bg = if is_open {
            Pixel::rgb(50, 54, 65)
        } else if resp.hovered {
            Pixel::rgb(45, 49, 58)
        } else {
            colors::SURFACE_RAISED
        };
        ui.fb.fill_rounded_rect_aa(rect, bg, 6);
        ui.fb.draw_rounded_rect(rect, colors::SURFACE_BORDER, 6, 1);

        // Selected text
        let selected_text = if *self.selected < self.items.len() {
            self.items[*self.selected]
        } else {
            "—"
        };
        fonts::draw_string_compact(
            ui.fb,
            selected_text,
            rect.x + 8,
            rect.y + (self.height as i32 - 10) / 2,
            ui.style().text_color,
        );

        // Chevron arrow
        let arrow_x = rect.x + self.width as i32 - 18;
        let arrow_y = rect.y + self.height as i32 / 2;
        if is_open {
            // Up chevron ▲
            ui.fb.draw_line_aa(
                arrow_x,
                arrow_y + 2,
                arrow_x + 4,
                arrow_y - 2,
                ui.style().text_dimmed,
            );
            ui.fb.draw_line_aa(
                arrow_x + 4,
                arrow_y - 2,
                arrow_x + 8,
                arrow_y + 2,
                ui.style().text_dimmed,
            );
        } else {
            // Down chevron ▼
            ui.fb.draw_line_aa(
                arrow_x,
                arrow_y - 2,
                arrow_x + 4,
                arrow_y + 2,
                ui.style().text_dimmed,
            );
            ui.fb.draw_line_aa(
                arrow_x + 4,
                arrow_y + 2,
                arrow_x + 8,
                arrow_y - 2,
                ui.style().text_dimmed,
            );
        }

        // ── Popup dropdown ──
        let mut changed = false;
        if is_open {
            let item_h = 26u32;
            let popup_h = (self.items.len() as u32 * item_h + 4).min(self.max_popup_height);
            let popup_rect =
                Rect::new(rect.x, rect.y + self.height as i32 + 2, self.width, popup_h);

            // Shadow + background
            ui.fb.fill_rounded_rect_aa(
                Rect::new(
                    popup_rect.x + 2,
                    popup_rect.y + 2,
                    popup_rect.width,
                    popup_rect.height,
                ),
                Pixel::new(0, 0, 0, 80),
                6,
            );
            ui.fb
                .fill_rounded_rect_aa(popup_rect, colors::SURFACE_OVERLAY, 6);
            ui.fb
                .draw_rounded_rect(popup_rect, colors::SURFACE_BORDER, 6, 1);

            // Items
            for (i, item) in self.items.iter().enumerate() {
                let iy = popup_rect.y + 2 + (i as u32 * item_h) as i32;
                let item_rect = Rect::new(popup_rect.x + 2, iy, self.width - 4, item_h);
                let item_id = id.with_index(i);
                let item_resp = ui.interact(item_rect, item_id, true, false);

                if item_resp.hovered {
                    ui.fb
                        .fill_rounded_rect_aa(item_rect, Pixel::new(255, 255, 255, 15), 4);
                }

                let text_color = if i == *self.selected {
                    ui.style().accent
                } else {
                    ui.style().text_color
                };

                fonts::draw_string_compact(
                    ui.fb,
                    item,
                    item_rect.x + 8,
                    iy + (item_h as i32 - 10) / 2,
                    text_color,
                );

                // Check mark for selected
                if i == *self.selected {
                    fonts::draw_string_compact(
                        ui.fb,
                        "✓",
                        item_rect.x + self.width as i32 - 24,
                        iy + (item_h as i32 - 10) / 2,
                        ui.style().accent,
                    );
                }

                if item_resp.clicked() {
                    *self.selected = i;
                    changed = true;
                    let mut mem = UI_MEMORY.lock();
                    mem.set_bool(open_key, false);
                }
            }

            // Close if clicked outside
            if ui.input.pointer_primary_pressed {
                let px = ui.input.pointer_x;
                let py = ui.input.pointer_y;
                if !rect.contains(px, py) && !popup_rect.contains(px, py) {
                    let mut mem = UI_MEMORY.lock();
                    mem.set_bool(open_key, false);
                }
            }
        }

        ComboBoxResponse {
            changed,
            selected_index: *self.selected,
            response: resp,
        }
    }
}
