/// CollapsingSection — Expandable/collapsible section with header and body
///
/// Enhanced version of the core `Ui::collapsing` with more options:
/// - Custom header rendering
/// - Animated expand/collapse
/// - Optional icon
/// - Default open/closed state
///
/// ```ignore
/// CollapsingSection::new("Advanced Settings")
///     .default_open(false)
///     .icon("⚙")
///     .show(ui, |ui| {
///         ui.label("Hidden content");
///     });
/// ```
use alloc::string::String;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::{UI_MEMORY, Ui};

pub struct CollapsingSection {
    id: Id,
    title: String,
    default_open: bool,
    icon: Option<String>,
    header_bg: Option<Pixel>,
    indent: bool,
}

impl CollapsingSection {
    pub fn new(title: &str) -> Self {
        Self {
            id: Id::from_str(title),
            title: String::from(title),
            default_open: true,
            icon: None,
            header_bg: None,
            indent: true,
        }
    }

    pub fn default_open(mut self, v: bool) -> Self {
        self.default_open = v;
        self
    }
    pub fn icon(mut self, i: &str) -> Self {
        self.icon = Some(String::from(i));
        self
    }
    pub fn header_bg(mut self, c: Pixel) -> Self {
        self.header_bg = Some(c);
        self
    }
    pub fn indent(mut self, v: bool) -> Self {
        self.indent = v;
        self
    }

    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let is_open = {
            let mem = UI_MEMORY.lock();
            mem.get_bool(self.id, self.default_open)
        };

        let header_h = ui.style().spacing.interact_height as u32;
        let avail_w = ui.available_width().max(0) as u32;
        let rect = ui.allocate_space(avail_w, header_h);
        let resp = ui.interact(rect, self.id, true, false);

        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.set_bool(self.id, !is_open);
        }

        // Header background
        if let Some(bg) = self.header_bg {
            ui.fb.fill_rounded_rect_aa(rect, bg, 4);
        } else if resp.hovered {
            ui.fb
                .fill_rounded_rect_aa(rect, ui.style().widget_bg_hovered, 4);
        }

        // Triangle indicator with smooth rotation feel
        let tri_x = rect.x + 8;
        let tri_y = rect.y + header_h as i32 / 2;
        let tri_color = if is_open {
            ui.style().accent
        } else {
            ui.style().text_dimmed
        };

        if is_open {
            // ▼ down
            for dy in 0..5i32 {
                let half = dy;
                ui.fb.draw_hline(
                    tri_x + 2 - half,
                    tri_y - 2 + dy,
                    (half * 2 + 1) as u32,
                    tri_color,
                );
            }
        } else {
            // ► right
            for dx in 0..5i32 {
                let half = dx;
                ui.fb
                    .draw_vline(tri_x + dx, tri_y - half, (half * 2 + 1) as u32, tri_color);
            }
        }

        // Icon
        let mut text_x = rect.x + 22;
        if let Some(ref icon) = self.icon {
            fonts::draw_string_compact(
                ui.fb,
                icon,
                text_x,
                rect.y + (header_h as i32 - 12) / 2,
                ui.style().text_dimmed,
            );
            text_x += (icon.len() as i32 + 1) * 8;
        }

        // Title
        let title_color = if is_open {
            ui.style().text_color
        } else {
            ui.style().text_dimmed
        };
        if is_open {
            fonts::draw_string_bold_compact(
                ui.fb,
                &self.title,
                text_x,
                rect.y + (header_h as i32 - 12) / 2,
                title_color,
            );
        } else {
            fonts::draw_string_compact(
                ui.fb,
                &self.title,
                text_x,
                rect.y + (header_h as i32 - 12) / 2,
                title_color,
            );
        }

        // Content
        let inner = if is_open {
            let indent = if self.indent {
                ui.style().spacing.indent
            } else {
                0
            };
            if indent > 0 {
                ui.region.cursor_x += indent;
                ui.region.max_rect.x += indent;
                ui.region.max_rect.width = (ui.region.max_rect.width as i32 - indent).max(0) as u32;
            }

            let r = add_contents(ui);

            if indent > 0 {
                ui.region.cursor_x -= indent;
                ui.region.max_rect.x -= indent;
                ui.region.max_rect.width = (ui.region.max_rect.width as i32 + indent) as u32;
            }
            Some(r)
        } else {
            None
        };

        InnerResponse {
            inner,
            response: resp,
        }
    }
}
