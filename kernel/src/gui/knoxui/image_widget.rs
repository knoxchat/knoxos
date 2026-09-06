/// ImageWidget — Display an RGBA image in the UI.
///
/// ```ignore
/// ImageWidget::new(&rgba_data, 64, 64).show(ui);
/// ```
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;

pub struct ImageWidget<'d> {
    data: &'d [u8],
    src_width: u32,
    src_height: u32,
    display_width: Option<u32>,
    display_height: Option<u32>,
    corner_radius: u32,
    tint: Option<Pixel>,
}

impl<'d> ImageWidget<'d> {
    /// Create an image widget from RGBA pixel data.
    /// `data` should be `src_width * src_height * 4` bytes (RGBA).
    pub fn new(data: &'d [u8], src_width: u32, src_height: u32) -> Self {
        Self {
            data,
            src_width,
            src_height,
            display_width: None,
            display_height: None,
            corner_radius: 0,
            tint: None,
        }
    }

    pub fn display_size(mut self, w: u32, h: u32) -> Self {
        self.display_width = Some(w);
        self.display_height = Some(h);
        self
    }

    pub fn corner_radius(mut self, r: u32) -> Self {
        self.corner_radius = r;
        self
    }

    pub fn tint(mut self, t: Pixel) -> Self {
        self.tint = Some(t);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let dw = self.display_width.unwrap_or(self.src_width);
        let dh = self.display_height.unwrap_or(self.src_height);
        let rect = ui.allocate_space(dw, dh);
        let id = ui.id.with("image");

        // Nearest-neighbor blit
        for dy in 0..dh {
            for dx in 0..dw {
                let sx = (dx as u64 * self.src_width as u64 / dw as u64) as u32;
                let sy = (dy as u64 * self.src_height as u64 / dh as u64) as u32;

                // Corner radius check: skip pixels outside rounded corners
                if self.corner_radius > 0 {
                    let cr = self.corner_radius;
                    let in_tl = dx < cr && dy < cr;
                    let in_tr = dx >= dw - cr && dy < cr;
                    let in_bl = dx < cr && dy >= dh - cr;
                    let in_br = dx >= dw - cr && dy >= dh - cr;

                    if in_tl || in_tr || in_bl || in_br {
                        let (ccx, ccy) = if in_tl {
                            (cr, cr)
                        } else if in_tr {
                            (dw - cr, cr)
                        } else if in_bl {
                            (cr, dh - cr)
                        } else {
                            (dw - cr, dh - cr)
                        };

                        let ddx = dx as i32 - ccx as i32;
                        let ddy = dy as i32 - ccy as i32;
                        if (ddx * ddx + ddy * ddy) > (cr * cr) as i32 {
                            continue;
                        }
                    }
                }

                let idx = ((sy * self.src_width + sx) * 4) as usize;
                if idx + 3 < self.data.len() {
                    let mut r = self.data[idx];
                    let mut g = self.data[idx + 1];
                    let mut b = self.data[idx + 2];
                    let a = self.data[idx + 3];

                    // Apply tint
                    if let Some(tint) = self.tint {
                        r = ((r as u16 * tint.r as u16) / 255) as u8;
                        g = ((g as u16 * tint.g as u16) / 255) as u8;
                        b = ((b as u16 * tint.b as u16) / 255) as u8;
                    }

                    if a > 0 {
                        let px = (rect.x + dx as i32) as usize;
                        let py = (rect.y + dy as i32) as usize;
                        ui.fb.set_pixel(px, py, Pixel::new(r, g, b, a));
                    }
                }
            }
        }

        ui.interact(rect, id, true, false)
    }
}
