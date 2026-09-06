/// Tabs — Tabbed container with closable, reorderable tabs
///
/// ```ignore
/// TabBar::new("editor_tabs")
///     .show(ui, &tab_names, &mut active_tab, |ui, tab_index| {
///         ui.label(&format!("Content of tab {}", tab_index));
///     });
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

/// Tab bar with closable tabs.
pub struct TabBar {
    id: Id,
    closable: bool,
    tab_height: u32,
    tab_min_width: u32,
    bg_color: Pixel,
    active_bg: Pixel,
    inactive_bg: Pixel,
    hover_bg: Pixel,
    indicator_color: Pixel,
    add_tab_button: bool,
}

impl TabBar {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            closable: false,
            tab_height: 32,
            tab_min_width: 80,
            bg_color: Pixel::rgb(18, 20, 26),
            active_bg: Pixel::rgb(30, 33, 40),
            inactive_bg: Pixel::rgb(22, 24, 30),
            hover_bg: Pixel::rgb(35, 38, 46),
            indicator_color: Pixel::rgb(82, 139, 255),
            add_tab_button: false,
        }
    }

    pub fn closable(mut self, v: bool) -> Self {
        self.closable = v;
        self
    }
    pub fn add_tab_button(mut self, v: bool) -> Self {
        self.add_tab_button = v;
        self
    }
    pub fn tab_height(mut self, h: u32) -> Self {
        self.tab_height = h;
        self
    }

    /// Show the tab bar and its content.
    /// Returns which tab was closed (if any) and whether "add" was clicked.
    pub fn show<'a>(
        &self,
        ui: &mut Ui<'a>,
        tab_names: &[&str],
        active: &mut usize,
        content: impl Fn(&mut Ui, usize),
    ) -> TabBarResponse {
        let avail_w = ui.available_width().max(0) as u32;
        let bar_rect = ui.allocate_space(avail_w, self.tab_height);

        // Bar background
        ui.fb.fill_rect(bar_rect, self.bg_color);

        let num_tabs = tab_names.len().max(1);
        let extra_space = if self.add_tab_button { 32 } else { 0 };
        let tab_w = ((avail_w - extra_space) / num_tabs as u32).max(self.tab_min_width);

        let mut closed_tab: Option<usize> = None;
        let mut add_clicked = false;

        let mut tx = bar_rect.x;
        for (i, name) in tab_names.iter().enumerate() {
            let is_active = i == *active;
            let w = tab_w.min(avail_w - (tx - bar_rect.x) as u32);
            let tab_rect = Rect::new(tx, bar_rect.y, w, self.tab_height);

            let tab_id = self.id.with_index(i);
            let resp = ui.interact(tab_rect, tab_id, true, false);

            // Background
            let bg = if is_active {
                self.active_bg
            } else if resp.hovered {
                self.hover_bg
            } else {
                self.inactive_bg
            };
            ui.fb.fill_rect(tab_rect, bg);

            // Active indicator bar
            if is_active {
                let ind_w = w.clamp(16, 48);
                let ind_x = tx + (w as i32 - ind_w as i32) / 2;
                ui.fb.fill_rounded_rect_aa(
                    Rect::new(ind_x, bar_rect.y + self.tab_height as i32 - 3, ind_w, 3),
                    self.indicator_color,
                    2,
                );
            }

            // Tab text
            let text_color = if is_active {
                colors::WHITE
            } else {
                Pixel::rgb(140, 145, 155)
            };
            let text_x = tx + 8;
            let max_text_w = w as i32 - 16 - if self.closable { 16 } else { 0 };
            let display_name = if name.len() as i32 * 8 > max_text_w {
                let chars = (max_text_w / 8).max(2) as usize;
                let truncated: String = name.chars().take(chars.saturating_sub(1)).collect();
                alloc::format!("{}..", truncated)
            } else {
                String::from(*name)
            };

            if is_active {
                fonts::draw_string_bold_compact(
                    ui.fb,
                    &display_name,
                    text_x,
                    bar_rect.y + (self.tab_height as i32 - 12) / 2,
                    text_color,
                );
            } else {
                fonts::draw_string_compact(
                    ui.fb,
                    &display_name,
                    text_x,
                    bar_rect.y + (self.tab_height as i32 - 12) / 2,
                    text_color,
                );
            }

            // Close button
            if self.closable {
                let cx = tx + w as i32 - 16;
                let cy = bar_rect.y + self.tab_height as i32 / 2;
                let close_rect = Rect::new(cx - 5, cy - 5, 10, 10);
                let close_id = tab_id.with("close");
                let close_resp = ui.interact(close_rect, close_id, true, false);

                if close_resp.hovered {
                    ui.fb.fill_circle_aa(cx, cy, 6, Pixel::rgb(200, 60, 70));
                    ui.fb
                        .draw_line_aa(cx - 3, cy - 3, cx + 3, cy + 3, colors::WHITE);
                    ui.fb
                        .draw_line_aa(cx - 3, cy + 3, cx + 3, cy - 3, colors::WHITE);
                }
                if close_resp.clicked() {
                    closed_tab = Some(i);
                }
            }

            // Select on click
            if resp.clicked() {
                *active = i;
            }

            tx += w as i32;
        }

        // Add tab button
        if self.add_tab_button {
            let add_rect = Rect::new(tx, bar_rect.y, 32, self.tab_height);
            let add_id = self.id.with("add_tab");
            let add_resp = ui.interact(add_rect, add_id, true, false);

            if add_resp.hovered {
                ui.fb.fill_rect(add_rect, self.hover_bg);
            }
            fonts::draw_string_centered_compact(
                ui.fb,
                "+",
                tx + 16,
                bar_rect.y + (self.tab_height as i32 - 10) / 2,
                colors::WHITE,
            );
            if add_resp.clicked() {
                add_clicked = true;
            }
        }

        // Bottom border
        ui.fb.draw_hline(
            bar_rect.x,
            bar_rect.y + self.tab_height as i32,
            avail_w,
            Pixel::rgb(45, 48, 56),
        );

        // Tab content
        if *active < tab_names.len() {
            content(ui, *active);
        }

        TabBarResponse {
            closed_tab,
            add_clicked,
        }
    }
}

/// Result of showing a tab bar.
pub struct TabBarResponse {
    pub closed_tab: Option<usize>,
    pub add_clicked: bool,
}

/// A trait for types that can be viewed as tabs (for more complex tab systems).
pub trait TabViewer {
    type Tab;
    fn title(&self, tab: &Self::Tab) -> &str;
    fn ui(&self, ui: &mut Ui, tab: &Self::Tab);
    fn closable(&self, _tab: &Self::Tab) -> bool {
        true
    }
}
