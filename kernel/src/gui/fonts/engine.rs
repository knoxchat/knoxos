use crate::gui::framebuffer::{FrameBuffer, Pixel};

// ═══════════════════════════════════════════════════════════════════════
// FONT ENGINE BRIDGE
// ═══════════════════════════════════════════════════════════════════════
// These functions delegate to the new scalable font_engine when TTF fonts
// are loaded, falling back to the bitmap path above otherwise.

/// Draw text using the new scalable font engine (UI proportional font).
/// Falls back to bitmap `draw_string` if TTF fonts aren't loaded.
pub fn draw_ui(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, size_px: u16) {
    crate::gui::font_engine::draw_ui_text(fb, x, y, text, size_px, color);
}

/// Draw text using the new scalable font engine (monospace font).
/// Falls back to bitmap `draw_string` if TTF fonts aren't loaded.
pub fn draw_mono(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, size_px: u16) {
    crate::gui::font_engine::draw_mono_text(fb, x, y, text, size_px, color);
}

/// Measure UI text width using new scalable font engine.
pub fn measure_ui(text: &str, size_px: u16) -> u32 {
    crate::gui::font_engine::measure_ui_text(text, size_px)
}

/// Measure monospace text width using new scalable font engine.
pub fn measure_mono(text: &str, size_px: u16) -> u32 {
    crate::gui::font_engine::measure_mono_text(text, size_px)
}
