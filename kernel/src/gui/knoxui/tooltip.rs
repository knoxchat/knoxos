use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::Ui;
/// Tooltip — Enhanced tooltip overlay that appears on hover.
///
/// ```ignore
/// let resp = ui.button("Hover me");
/// Tooltip::for_response(&resp, "My helpful tooltip").show(ui);
/// ```
use alloc::string::String;

pub struct Tooltip {
    text: String,
    anchor: Rect,
    max_width: u32,
    delay_frames: u32,
}

impl Tooltip {
    pub fn new(text: &str) -> Self {
        Self {
            text: String::from(text),
            anchor: Rect::new(0, 0, 0, 0),
            max_width: 250,
            delay_frames: 30, // ~0.5 second at 60fps
        }
    }

    pub fn for_response(resp: &crate::gui::response::Response, text: &str) -> Self {
        let mut t = Self::new(text);
        t.anchor = resp.rect;
        t
    }

    pub fn max_width(mut self, w: u32) -> Self {
        self.max_width = w;
        self
    }
    pub fn delay(mut self, frames: u32) -> Self {
        self.delay_frames = frames;
        self
    }

    /// Show the tooltip if the anchor is hovered.
    pub fn show(self, ui: &mut Ui) {
        let id = ui.id.with("tooltip");
        let resp = ui.interact(self.anchor, id, true, false);

        if !resp.hovered {
            return;
        }

        // Simple: show immediately (delay would need frame counter tracking)
        let text_w = (self.text.len() as u32 * 8).min(self.max_width);
        let char_per_line = (text_w / 8).max(1) as usize;
        let lines = self.text.len().div_ceil(char_per_line);
        let line_h = 12u32;
        let pad = 6u32;
        let tip_w = text_w + pad * 2;
        let tip_h = lines as u32 * line_h + pad * 2;

        // Position below anchor
        let tx = self.anchor.x;
        let ty = self.anchor.y + self.anchor.height as i32 + 4;
        let tip_rect = Rect::new(tx, ty, tip_w, tip_h);

        // Shadow
        let shadow = Rect::new(tx + 1, ty + 1, tip_w, tip_h);
        ui.fb
            .fill_rounded_rect_aa(shadow, Pixel::new(0, 0, 0, 80), 4);

        // Background
        ui.fb
            .fill_rounded_rect_aa(tip_rect, colors::SURFACE_OVERLAY, 4);
        ui.fb
            .draw_rounded_rect(tip_rect, colors::SURFACE_BORDER, 4, 1);

        // Text (simple line wrapping)
        let mut y = ty + pad as i32;
        let bytes = self.text.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + char_per_line).min(bytes.len());
            // Find safe break point at word boundary
            let slice = if end < bytes.len() {
                let mut brk = end;
                while brk > offset && bytes[brk] != b' ' {
                    brk -= 1;
                }
                if brk == offset {
                    &self.text[offset..end]
                } else {
                    let s = &self.text[offset..brk];
                    offset = brk + 1; // skip space
                    // Set offset before continue
                    fonts::draw_string_compact(
                        ui.fb,
                        s,
                        tx + pad as i32,
                        y,
                        colors::TEXT_SECONDARY,
                    );
                    y += line_h as i32;
                    continue;
                }
            } else {
                &self.text[offset..end]
            };
            fonts::draw_string_compact(ui.fb, slice, tx + pad as i32, y, colors::TEXT_SECONDARY);
            y += line_h as i32;
            offset = end;
        }
    }
}
