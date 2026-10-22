/// Desktop Lucide helpers for compositor paint (drag preview, leftover chrome).
/// HiDPI scale is kept as a public knob even though GPUI surfaces size glyphs
/// in layout instead of stretching bitmaps.
use knox_ui::IconName;

use super::desktop::IconType;
use super::framebuffer::{FrameBuffer, Pixel};
use super::icon_theme;
use core::sync::atomic::{AtomicU32, Ordering};

/// Icon size in pixels (48×48 desktop, 24×24 start menu, 16×16 taskbar)
pub const ICON_SIZE: u32 = 48;

/// Global DPI scale factor stored as fixed-point 8.8 (256 = 1.0x, 512 = 2.0x).
static DPI_SCALE: AtomicU32 = AtomicU32::new(256);

pub fn set_dpi_scale(scale_100: u32) {
    let fp = scale_100 * 256 / 100;
    DPI_SCALE.store(fp, Ordering::Relaxed);
}

pub fn dpi_scale_fp() -> u32 {
    DPI_SCALE.load(Ordering::Relaxed)
}

pub fn dpi_scale_percent() -> u32 {
    DPI_SCALE.load(Ordering::Relaxed) * 100 / 256
}

pub fn dpi_scale(px: u32) -> u32 {
    (px as u64 * dpi_scale_fp() as u64 / 256) as u32
}

fn draw_scaled(fb: &mut FrameBuffer, x: i32, y: i32, name: IconName, native: u32) {
    let target = dpi_scale(native).max(8) as i32;
    icon_theme::blit(fb, x, y, name, target);
}

pub fn draw_my_pc(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::Monitor, 48);
}

pub fn draw_folder(fb: &mut FrameBuffer, x: i32, y: i32, _color: Pixel) {
    draw_scaled(fb, x, y, IconName::Folder, 48);
}

pub fn draw_document(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::FileText, 48);
}

pub fn draw_globe(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::Globe, 48);
}

pub fn draw_terminal(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::SquareTerminal, 48);
}

pub fn draw_media_player(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::Play, 48);
}

pub fn draw_game(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::Gamepad, 48);
}

pub fn draw_ai_brain(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::Sparkles, 48);
}

pub fn draw_shortcut_arrow(fb: &mut FrameBuffer, x: i32, y: i32) {
    icon_theme::blit(fb, x + 2, y + 32, IconName::ArrowRight, 14);
}

pub fn draw_app_launcher_icon(fb: &mut FrameBuffer, x: i32, y: i32) {
    icon_theme::draw_tiny_icon(fb, x, y, IconName::LayoutGrid);
}

pub fn draw_settings(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_scaled(fb, x, y, IconName::Settings, 48);
}

pub fn draw_small_icon(fb: &mut FrameBuffer, x: i32, y: i32, icon_type: &IconType) {
    let target = dpi_scale(24).max(8) as i32;
    icon_theme::blit(fb, x, y, icon_theme::for_desktop(*icon_type), target);
}

pub fn draw_tiny_icon(fb: &mut FrameBuffer, x: i32, y: i32, icon_type: &IconType) {
    let target = dpi_scale(16).max(8) as i32;
    icon_theme::blit(fb, x, y, icon_theme::for_desktop(*icon_type), target);
}
