use crate::gui::fonts;
use crate::gui::framebuffer::Rect;
use crate::gui::response::InnerResponse;

use super::{UI_MEMORY, Ui};

impl<'a> Ui<'a> {
    // ── Collapsing header ────────────────────────────────────────

    /// Draw a collapsing/expanding section header.
    pub fn collapsing<R>(
        &mut self,
        title: &str,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let id = self.id_from(title);
        let is_open = {
            let mem = UI_MEMORY.lock();
            mem.get_bool(id, false)
        };

        // Header
        let header_w = self.available_width().max(0) as u32;
        let header_h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(header_w, header_h);
        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.set_bool(id, !is_open);
        }

        // Draw header bg on hover
        if resp.hovered {
            self.fb
                .fill_rounded_rect_aa(rect, self.style.widget_bg_hovered, 4);
        }

        // Triangle indicator
        let tri_x = rect.x + 4;
        let tri_y = rect.y + header_h as i32 / 2;
        if is_open {
            // Down triangle ▼
            for dy in 0..6i32 {
                let half = dy;
                self.fb.draw_hline(
                    tri_x + 3 - half,
                    tri_y - 3 + dy,
                    (half * 2 + 1) as u32,
                    self.style.text_color,
                );
            }
        } else {
            // Right triangle ►
            for dx in 0..6i32 {
                let half = dx;
                self.fb.draw_vline(
                    tri_x + dx,
                    tri_y - half,
                    (half * 2 + 1) as u32,
                    self.style.text_color,
                );
            }
        }

        // Title text
        if is_open {
            fonts::draw_string_bold_compact(
                self.fb,
                rect.x + 16,
                rect.y + (header_h as i32 - 12) / 2,
                title,
                self.style.text_color,
                1,
            );
        } else {
            fonts::draw_string_compact(
                self.fb,
                rect.x + 16,
                rect.y + (header_h as i32 - 12) / 2,
                title,
                self.style.text_color,
                1,
            );
        }

        // Content
        let inner = if is_open {
            let indent = self.style.spacing.indent;
            self.region.cursor_x += indent;
            self.region.max_rect.x += indent;
            self.region.max_rect.width = (self.region.max_rect.width as i32 - indent).max(0) as u32;

            let r = add_contents(self);

            self.region.cursor_x -= indent;
            self.region.max_rect.x -= indent;
            self.region.max_rect.width = (self.region.max_rect.width as i32 + indent) as u32;

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
