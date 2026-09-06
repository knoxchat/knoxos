use super::text_helpers as fonts;
/// ColorPicker — Interactive color selection with hue/saturation/value
///
/// ```ignore
/// ColorPicker::new("theme_color").show(ui, &mut color);
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};

pub struct ColorPicker {
    id: Id,
    size: u32,
    show_alpha: bool,
    show_hex: bool,
}

impl ColorPicker {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            size: 180,
            show_alpha: false,
            show_hex: true,
        }
    }

    pub fn size(mut self, s: u32) -> Self {
        self.size = s;
        self
    }
    pub fn show_alpha(mut self, v: bool) -> Self {
        self.show_alpha = v;
        self
    }

    /// Show the color picker. Modifies `color` in place.
    pub fn show<'a>(&self, ui: &mut Ui<'a>, color: &mut Pixel) {
        let avail_w = ui.available_width().max(0) as u32;

        // HSV from current color
        let (mut h, mut s, mut v) = rgb_to_hsv(color.r, color.g, color.b);

        // Hue bar (vertical, on the right)
        let hue_bar_w = 20u32;
        let gap = 8;
        let sv_size = self.size;
        let total_w = sv_size + gap as u32 + hue_bar_w;

        let area_rect = ui.allocate_space(total_w, sv_size);

        // === SV square ===
        let sv_rect = Rect::new(area_rect.x, area_rect.y, sv_size, sv_size);
        // Draw SV gradient
        for py in 0..sv_size {
            for px in 0..sv_size {
                let local_s = px as f32 / sv_size as f32;
                let local_v = 1.0 - (py as f32 / sv_size as f32);
                let (r, g, b) = hsv_to_rgb(h, local_s, local_v);
                ui.fb.set_pixel(
                    (area_rect.x + px as i32) as usize,
                    (area_rect.y + py as i32) as usize,
                    Pixel::rgb(r, g, b),
                );
            }
        }

        // SV interaction
        let sv_id = self.id.with("sv");
        let sv_resp = ui.interact(sv_rect, sv_id, true, true);
        if sv_resp.is_pointer_button_down_on || sv_resp.dragged {
            s = ((ui.input.pointer_x - sv_rect.x) as f32 / sv_size as f32).clamp(0.0, 1.0);
            v = 1.0 - ((ui.input.pointer_y - sv_rect.y) as f32 / sv_size as f32).clamp(0.0, 1.0);
        }

        // SV cursor
        let cx = sv_rect.x + (s * sv_size as f32) as i32;
        let cy = sv_rect.y + ((1.0 - v) * sv_size as f32) as i32;
        ui.fb.fill_circle_aa(cx, cy, 6, colors::WHITE);
        ui.fb.fill_circle_aa(cx, cy, 4, *color);

        // Border
        ui.fb
            .draw_rounded_rect(sv_rect, Pixel::rgb(60, 65, 75), 0, 1);

        // === Hue bar ===
        let hue_x = area_rect.x + sv_size as i32 + gap;
        let hue_rect = Rect::new(hue_x, area_rect.y, hue_bar_w, sv_size);

        for py in 0..sv_size {
            let local_h = py as f32 / sv_size as f32 * 360.0;
            let (r, g, b) = hsv_to_rgb(local_h, 1.0, 1.0);
            ui.fb.draw_hline(
                hue_x,
                area_rect.y + py as i32,
                hue_bar_w,
                Pixel::rgb(r, g, b),
            );
        }

        let hue_id = self.id.with("hue");
        let hue_resp = ui.interact(hue_rect, hue_id, true, true);
        if hue_resp.is_pointer_button_down_on || hue_resp.dragged {
            h = ((ui.input.pointer_y - hue_rect.y) as f32 / sv_size as f32 * 360.0)
                .clamp(0.0, 360.0);
        }

        // Hue cursor
        let hy = hue_rect.y + (h / 360.0 * sv_size as f32) as i32;
        ui.fb.draw_hline(hue_x, hy, hue_bar_w, colors::WHITE);
        ui.fb
            .draw_rounded_rect(hue_rect, Pixel::rgb(60, 65, 75), 0, 1);

        // Update color
        let (r, g, b) = hsv_to_rgb(h, s, v);
        color.r = r;
        color.g = g;
        color.b = b;

        // Color preview + hex
        if self.show_hex {
            ui.add_space(4);
            let preview_rect = ui.allocate_space(total_w, 24);
            // Preview swatch
            ui.fb.fill_rounded_rect_aa(
                Rect::new(preview_rect.x, preview_rect.y, 24, 24),
                *color,
                4,
            );
            // Hex text
            let hex = alloc::format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b);
            fonts::draw_string_compact(
                ui.fb,
                &hex,
                preview_rect.x + 32,
                preview_rect.y + 6,
                ui.style().text_color,
            );
        }

        // Alpha slider
        if self.show_alpha {
            ui.add_space(4);
            ui.slider_u8("Alpha", &mut color.a, 0, 255);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HSV ↔ RGB conversion (integer-friendly, no_std)
// ═══════════════════════════════════════════════════════════════════════

/// Convert RGB (0-255 each) to HSV (h: 0-360, s: 0-1, v: 0-1).
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let h = if delta < 0.001 {
        0.0
    } else if (max - rf).abs() < 0.001 {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if (max - gf).abs() < 0.001 {
        60.0 * ((bf - rf) / delta + 2.0)
    } else {
        60.0 * ((rf - gf) / delta + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };

    let s = if max < 0.001 { 0.0 } else { delta / max };
    (h, s, max)
}

/// Convert HSV (h: 0-360, s: 0-1, v: 0-1) to RGB (0-255 each).
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}
