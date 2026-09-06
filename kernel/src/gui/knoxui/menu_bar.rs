/// MenuBar — Application menu bar with dropdown menus
///
/// ```ignore
/// MenuBar::new("app_menu").show(ui, |menu| {
///     menu.menu("File", |ui| {
///         if ui.button("New").clicked() { /* ... */ }
///         ui.separator();
///         if ui.button("Exit").clicked() { /* ... */ }
///     });
///     menu.menu("Edit", |ui| {
///         if ui.button("Undo").clicked() { /* ... */ }
///         if ui.button("Redo").clicked() { /* ... */ }
///     });
/// });
/// ```
use alloc::string::String;
use alloc::vec::Vec;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};

/// A menu item in a dropdown.
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub enabled: bool,
    pub separator: bool,
    pub checked: Option<bool>,
}

impl MenuItem {
    pub fn new(label: &str) -> Self {
        Self {
            label: String::from(label),
            shortcut: None,
            enabled: true,
            separator: false,
            checked: None,
        }
    }

    pub fn shortcut(mut self, s: &str) -> Self {
        self.shortcut = Some(String::from(s));
        self
    }
    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = v;
        self
    }
    pub fn checked(mut self, v: bool) -> Self {
        self.checked = Some(v);
        self
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            shortcut: None,
            enabled: false,
            separator: true,
            checked: None,
        }
    }
}

/// Menu bar widget.
pub struct MenuBar {
    id: Id,
    height: u32,
    bg_color: Pixel,
}

impl MenuBar {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            height: 28,
            bg_color: Pixel::rgb(18, 20, 26),
        }
    }

    pub fn show<'a>(self, ui: &mut Ui<'a>, add_menus: impl FnOnce(&mut MenuBarContext)) {
        let avail_w = ui.available_width().max(0) as u32;
        let bar_rect = ui.allocate_space(avail_w, self.height);

        // Background
        ui.fb.fill_rect(bar_rect, self.bg_color);
        ui.fb.draw_hline(
            bar_rect.x,
            bar_rect.y + self.height as i32,
            avail_w,
            Pixel::rgb(40, 44, 52),
        );

        let mut ctx = MenuBarContext {
            ui,
            bar_id: self.id,
            bar_rect,
            menu_x: bar_rect.x + 4,
            height: self.height,
        };

        add_menus(&mut ctx);
    }
}

/// Context passed to the menu bar builder closure.
pub struct MenuBarContext<'a, 'b> {
    ui: &'b mut Ui<'a>,
    bar_id: Id,
    bar_rect: Rect,
    menu_x: i32,
    height: u32,
}

impl<'a, 'b> MenuBarContext<'a, 'b> {
    /// Add a top-level menu. The closure adds items to the dropdown.
    pub fn menu(&mut self, label: &str, add_items: impl FnOnce(&mut MenuDropdown)) {
        let menu_id = self.bar_id.with(label);
        let label_w = (label.len() as u32) * 8 + 16;
        let btn_rect = Rect::new(self.menu_x, self.bar_rect.y, label_w, self.height);

        let resp = self.ui.interact(btn_rect, menu_id, true, false);

        let is_open = {
            let mem = UI_MEMORY.lock();
            mem.get_bool(menu_id, false)
        };

        // Toggle on click
        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.set_bool(menu_id, !is_open);
        }

        // Hover highlight / active state
        let bg = if is_open {
            Pixel::rgb(40, 44, 55)
        } else if resp.hovered {
            Pixel::rgb(35, 38, 48)
        } else {
            Pixel::new(0, 0, 0, 0)
        };
        if bg.a > 0 {
            self.ui.fb.fill_rounded_rect_aa(btn_rect, bg, 4);
        }

        fonts::draw_string_compact(
            self.ui.fb,
            label,
            self.menu_x + 8,
            self.bar_rect.y + (self.height as i32 - 12) / 2,
            if is_open || resp.hovered {
                colors::WHITE
            } else {
                Pixel::rgb(180, 185, 195)
            },
        );

        // Dropdown
        if is_open {
            let popup_x = self.menu_x;
            let popup_y = self.bar_rect.y + self.height as i32 + 1;

            let mut dropdown = MenuDropdown { items: Vec::new() };
            add_items(&mut dropdown);

            let popup_w = 220u32;
            let item_h = 26u32;
            let sep_h = 9u32;
            let mut popup_h = 4u32; // padding
            for item in &dropdown.items {
                popup_h += if item.separator { sep_h } else { item_h };
            }
            popup_h += 4; // bottom padding

            let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

            // Shadow
            self.ui.fb.fill_rounded_rect_aa(
                Rect::new(popup_x + 3, popup_y + 3, popup_w, popup_h),
                Pixel::new(0, 0, 0, 100),
                8,
            );
            // Background
            self.ui
                .fb
                .fill_rounded_rect_aa(popup_rect, Pixel::rgb(28, 31, 40), 8);
            self.ui
                .fb
                .draw_rounded_rect(popup_rect, Pixel::rgb(55, 60, 72), 8, 1);

            let mut iy = popup_y + 4;
            for (idx, item) in dropdown.items.iter().enumerate() {
                if item.separator {
                    self.ui.fb.draw_hline(
                        popup_x + 8,
                        iy + 4,
                        popup_w - 16,
                        Pixel::rgb(50, 55, 65),
                    );
                    iy += sep_h as i32;
                    continue;
                }

                let item_rect = Rect::new(popup_x + 4, iy, popup_w - 8, item_h);
                let item_id = menu_id.with_index(idx);
                let item_resp = self.ui.interact(item_rect, item_id, true, false);

                if item.enabled && item_resp.hovered {
                    self.ui
                        .fb
                        .fill_rounded_rect_aa(item_rect, Pixel::rgb(50, 90, 180), 4);
                }

                let text_color = if item.enabled {
                    colors::WHITE
                } else {
                    Pixel::rgb(90, 95, 105)
                };

                // Checkbox indicator
                if let Some(checked) = item.checked {
                    if checked {
                        fonts::draw_string_compact(
                            self.ui.fb,
                            "*",
                            item_rect.x + 6,
                            iy + (item_h as i32 - 12) / 2,
                            self.ui.style().accent,
                        );
                    }
                }

                let text_offset = if item.checked.is_some() { 16 } else { 6 };
                fonts::draw_string_compact(
                    self.ui.fb,
                    &item.label,
                    item_rect.x + text_offset,
                    iy + (item_h as i32 - 12) / 2,
                    text_color,
                );

                // Shortcut
                if let Some(ref shortcut) = item.shortcut {
                    let sw = shortcut.len() as i32 * 8;
                    fonts::draw_string_compact(
                        self.ui.fb,
                        shortcut,
                        item_rect.x + item_rect.width as i32 - sw - 6,
                        iy + (item_h as i32 - 12) / 2,
                        Pixel::rgb(110, 115, 125),
                    );
                }

                // Click to activate and close menu
                if item.enabled && item_resp.clicked() {
                    let mut mem = UI_MEMORY.lock();
                    mem.set_bool(menu_id, false);
                }

                iy += item_h as i32;
            }

            // Close on click outside
            if self.ui.input.pointer_primary_pressed
                && !popup_rect.contains(self.ui.input.pointer_x, self.ui.input.pointer_y)
                && !btn_rect.contains(self.ui.input.pointer_x, self.ui.input.pointer_y)
            {
                let mut mem = UI_MEMORY.lock();
                mem.set_bool(menu_id, false);
            }
        }

        self.menu_x += label_w as i32 + 2;
    }
}

/// Dropdown menu builder (collects items).
pub struct MenuDropdown {
    items: Vec<MenuItem>,
}

impl MenuDropdown {
    pub fn item(&mut self, item: MenuItem) {
        self.items.push(item);
    }

    pub fn separator(&mut self) {
        self.items.push(MenuItem::separator());
    }

    /// Quick item: just a label.
    pub fn button(&mut self, label: &str) {
        self.items.push(MenuItem::new(label));
    }

    /// Quick item with shortcut.
    pub fn button_with_shortcut(&mut self, label: &str, shortcut: &str) {
        self.items.push(MenuItem::new(label).shortcut(shortcut));
    }
}
