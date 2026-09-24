use crate::gui::framebuffer::{FrameBuffer, Pixel};

// ═══════════════════════════════════════════════════════════════════════════
// SEPARATOR WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Separator;

impl Separator {
    pub fn draw_horizontal(fb: &mut FrameBuffer, x: i32, y: i32, width: u32) {
        fb.draw_hline(x, y, width, Pixel::rgb(50, 50, 55));
    }

    pub fn draw_vertical(fb: &mut FrameBuffer, x: i32, y: i32, height: u32) {
        fb.draw_vline(x, y, height, Pixel::rgb(50, 50, 55));
    }
}
