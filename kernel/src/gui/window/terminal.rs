/// Terminal window content — tab bar and terminal renderer
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::types::Window;

impl Window {
    pub(super) fn draw_terminal_content(&self, fb: &mut FrameBuffer, content: Rect) {
        // ── Terminal tab bar (only if multiple tabs) ──
        let tab_bar_h: i32 = if self.terminal_tabs.len() > 1 { 26 } else { 0 };

        if tab_bar_h > 0 {
            let tab_bg = Pixel::rgb(30, 30, 30);
            let tab_active_bg = Pixel::rgb(12, 12, 12);
            let tab_text = Pixel::rgb(180, 180, 180);
            let tab_active_text = Pixel::rgb(255, 255, 255);
            let tab_border = Pixel::rgb(50, 50, 50);

            // Tab bar background
            fb.fill_rect(
                Rect::new(content.x, content.y, content.width, tab_bar_h as u32),
                tab_bg,
            );
            // Bottom border
            fb.draw_hline(
                content.x,
                content.y + tab_bar_h - 1,
                content.width,
                tab_border,
            );

            let tab_w = 140i32.min(content.width as i32 / self.terminal_tabs.len().max(1) as i32);
            for (i, _tid) in self.terminal_tabs.iter().enumerate() {
                let tx = content.x + (i as i32) * tab_w;
                let is_active = i == self.terminal_active_tab;

                if is_active {
                    fb.fill_rect(
                        Rect::new(tx, content.y, tab_w as u32, tab_bar_h as u32),
                        tab_active_bg,
                    );
                }

                let label = alloc::format!("Shell {}", i + 1);
                let text_color = if is_active { tab_active_text } else { tab_text };
                fonts::draw_string_compact(fb, tx + 8, content.y + 6, &label, text_color, 1);

                // Tab close button (x) for non-first tabs
                if self.terminal_tabs.len() > 1 {
                    let cx = tx + tab_w - 18;
                    let cy = content.y + 6;
                    fonts::draw_string_compact(fb, cx, cy, "×", Pixel::rgb(120, 120, 120), 1);
                }

                // Tab separator
                if i > 0 {
                    fb.draw_vline(tx, content.y + 4, (tab_bar_h - 8) as u32, tab_border);
                }
            }

            // "+" button after last tab
            let plus_x = content.x + (self.terminal_tabs.len() as i32) * tab_w;
            if plus_x + 24 < content.x + content.width as i32 {
                fonts::draw_string_compact(
                    fb,
                    plus_x + 8,
                    content.y + 6,
                    "+",
                    Pixel::rgb(100, 100, 100),
                    1,
                );
            }
        }

        // ── Terminal content area (below tab bar) ──
        let term_area = Rect::new(
            content.x,
            content.y + tab_bar_h,
            content.width,
            content.height.saturating_sub(tab_bar_h as u32),
        );

        // Determine which terminal ID to render
        let term_id = if !self.terminal_tabs.is_empty() {
            self.terminal_tabs
                .get(self.terminal_active_tab)
                .copied()
                .unwrap_or(self.id)
        } else {
            self.id
        };

        crate::terminal::render_for_window(term_id, fb, term_area);
    }
}
