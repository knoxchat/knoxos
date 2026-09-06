/// Desktop Icons — KnoxOS SVG icons pre-rendered at build time
/// Uses blit_bgra to draw BGRA pixel bitmaps with full alpha blending
/// Supports HiDPI scaling via a global DPI scale factor.
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::icon_data;
use core::sync::atomic::{AtomicU32, Ordering};

/// Icon size in pixels (48×48 desktop, 24×24 start menu, 16×16 taskbar)
pub const ICON_SIZE: u32 = 48;

// ═══════════════════════════════════════════════════════════════════════
// HiDPI SCALE FACTOR
// ═══════════════════════════════════════════════════════════════════════

/// Global DPI scale factor stored as fixed-point 8.8 (256 = 1.0x, 512 = 2.0x).
/// Default 256 (1.0x). Set via `set_dpi_scale()`.
static DPI_SCALE: AtomicU32 = AtomicU32::new(256);

/// Set the global DPI scale factor. `scale_100` is the percentage (100 = 1.0x, 200 = 2.0x).
pub fn set_dpi_scale(scale_100: u32) {
    let fp = scale_100 * 256 / 100;
    DPI_SCALE.store(fp, Ordering::Relaxed);
}

/// Get the current DPI scale factor as fixed-point 8.8.
pub fn dpi_scale_fp() -> u32 {
    DPI_SCALE.load(Ordering::Relaxed)
}

/// Get the DPI scale percentage (100 = 1.0x, 150 = 1.5x, 200 = 2.0x).
pub fn dpi_scale_percent() -> u32 {
    DPI_SCALE.load(Ordering::Relaxed) * 100 / 256
}

/// Scale a pixel dimension by the current DPI factor.
pub fn dpi_scale(px: u32) -> u32 {
    (px as u64 * dpi_scale_fp() as u64 / 256) as u32
}

/// Draw a BGRA icon with HiDPI-aware scaling.
/// If DPI scale is 1.0x, uses native blit for maximum quality.
/// At higher scales, uses bilinear `blit_bgra_scaled` from the highest-quality
/// source available, with optional sharpening.
fn draw_icon_scaled(fb: &mut FrameBuffer, x: i32, y: i32, native_size: u32, data: &[u8]) {
    let scale = dpi_scale_fp();
    if scale == 256 {
        // 1.0x — native blit (fastest, sharpest)
        fb.blit_bgra(x, y, native_size, native_size, data);
    } else {
        // HiDPI: scale to target size using bilinear interpolation
        let target = dpi_scale(native_size);
        fb.blit_bgra_scaled(x, y, target, target, data, native_size, native_size);
        // Apply subtle sharpening to counteract bilinear softening
        if target > native_size {
            fb.sharpen_region(x, y, target, target, 80);
        }
    }
}

/// Draw a 48×48 desktop icon, HiDPI-aware.
/// When scaled up, uses 48×48 source with bilinear upscaling + sharpening.
fn draw_desktop_icon_scaled(fb: &mut FrameBuffer, x: i32, y: i32, data_48: &[u8]) {
    draw_icon_scaled(fb, x, y, 48, data_48);
}

/// Draw a 24×24 start menu icon, HiDPI-aware.
/// At 2.0x, uses the 48×48 source for native quality instead of upscaling 24×24.
fn draw_small_icon_scaled(fb: &mut FrameBuffer, x: i32, y: i32, data_24: &[u8], data_48: &[u8]) {
    let scale = dpi_scale_fp();
    if scale == 256 {
        fb.blit_bgra(x, y, 24, 24, data_24);
    } else {
        let target = dpi_scale(24);
        if target >= 40 {
            // Use 48×48 source for better quality when upscaling beyond ~1.7x
            fb.blit_bgra_scaled(x, y, target, target, data_48, 48, 48);
        } else {
            fb.blit_bgra_scaled(x, y, target, target, data_24, 24, 24);
        }
        if target > 24 {
            fb.sharpen_region(x, y, target, target, 60);
        }
    }
}

/// Draw a 16×16 taskbar icon, HiDPI-aware.
/// At 2.0x, uses the 24×24 source for better quality.
fn draw_tiny_icon_scaled(fb: &mut FrameBuffer, x: i32, y: i32, data_16: &[u8], data_24: &[u8]) {
    let scale = dpi_scale_fp();
    if scale == 256 {
        fb.blit_bgra(x, y, 16, 16, data_16);
    } else {
        let target = dpi_scale(16);
        if target >= 20 {
            // Use 24×24 source for better quality
            fb.blit_bgra_scaled(x, y, target, target, data_24, 24, 24);
        } else {
            fb.blit_bgra_scaled(x, y, target, target, data_16, 16, 16);
        }
        if target > 16 {
            fb.sharpen_region(x, y, target, target, 60);
        }
    }
}

/// Draw the "Files" icon — computer SVG (48×48)
pub fn draw_my_pc(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::COMPUTER);
}

/// Draw a folder icon — folder SVG (48×48)
pub fn draw_folder(fb: &mut FrameBuffer, x: i32, y: i32, _color: Pixel) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::FOLDER);
}

/// Draw a document/file icon — text-x-generic SVG (48×48)
pub fn draw_document(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::DOCUMENT);
}

/// Draw a globe/browser icon — internet-web-browser SVG (48×48)
pub fn draw_globe(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::BROWSER);
}

/// Draw a terminal icon — terminal-1 SVG (48×48)
pub fn draw_terminal(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::TERMINAL);
}

/// Draw a media player icon — multimedia-video-player SVG (48×48)
pub fn draw_media_player(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::MEDIA_PLAYER);
}

/// Draw the game controller icon — applications-games SVG (48×48)
pub fn draw_game(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::GAME);
}

/// Draw an AI brain icon — utilities-x-terminal SVG (48×48)
pub fn draw_ai_brain(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::AI_BRAIN);
}

/// Draw a shortcut arrow overlay (small arrow in bottom-left)
pub fn draw_shortcut_arrow(fb: &mut FrameBuffer, x: i32, y: i32) {
    let arrow_bg = Pixel::rgb(255, 255, 255);
    let arrow_fg = Pixel::rgb(0, 100, 200);

    // White rounded box — AA
    fb.fill_rounded_rect_aa(Rect::new(x, y + 35, 14, 12), arrow_bg, 3);
    fb.draw_rounded_rect(Rect::new(x, y + 35, 14, 12), Pixel::rgb(0, 100, 180), 3, 1);

    // Arrow shape — use AA lines for smooth diagonals
    fb.draw_line_aa(x + 4, y + 41, x + 10, y + 41, arrow_fg);
    fb.draw_line_aa(x + 8, y + 38, x + 8, y + 44, arrow_fg);
    fb.draw_line_aa(x + 6, y + 39, x + 8, y + 38, arrow_fg);
    fb.draw_line_aa(x + 10, y + 39, x + 8, y + 38, arrow_fg);
}

/// Draw the app launcher icon for the start button — appgrid SVG (16×16)
pub fn draw_app_launcher_icon(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 16, 16, &icon_data::APP_LAUNCHER_16);
}

/// Draw a settings gear icon — org.gnome.Settings SVG (48×48)
pub fn draw_settings(fb: &mut FrameBuffer, x: i32, y: i32) {
    fb.blit_bgra(x, y, 48, 48, &icon_data::SETTINGS);
}

// ═══════════════════════════════════════════════════════════════════════
// Bitmap blit helpers for 24×24 (start menu) and 16×16 (taskbar) sizes
// ═══════════════════════════════════════════════════════════════════════

use super::desktop::IconType;

/// Draw a 24×24 small icon for the start menu, using pre-rendered SVGs.
/// HiDPI-aware: at high DPI, uses the 48×48 source for better quality.
pub fn draw_small_icon(fb: &mut FrameBuffer, x: i32, y: i32, icon_type: &IconType) {
    let (data_24, data_48): (&[u8], &[u8]) = match icon_type {
        IconType::MyPC => (&icon_data::COMPUTER_24, &icon_data::COMPUTER),
        IconType::Folder => (&icon_data::FOLDER_24, &icon_data::FOLDER),
        IconType::Document => (&icon_data::DOCUMENT_24, &icon_data::DOCUMENT),
        IconType::Globe => (&icon_data::BROWSER_24, &icon_data::BROWSER),
        IconType::Terminal => (&icon_data::TERMINAL_24, &icon_data::TERMINAL),
        IconType::MediaPlayer => (&icon_data::MEDIA_PLAYER_24, &icon_data::MEDIA_PLAYER),
        IconType::Game => (&icon_data::GAME_24, &icon_data::GAME),
        IconType::AIBrain => (&icon_data::AI_BRAIN_24, &icon_data::AI_BRAIN),
        IconType::Settings => (&icon_data::SETTINGS_24, &icon_data::SETTINGS),
        // New types fall back to generic icons
        IconType::Trash => (&icon_data::FOLDER_24, &icon_data::FOLDER),
        IconType::Image => (&icon_data::DOCUMENT_24, &icon_data::DOCUMENT),
        IconType::Archive => (&icon_data::DOCUMENT_24, &icon_data::DOCUMENT),
        IconType::Script => (&icon_data::TERMINAL_24, &icon_data::TERMINAL),
    };
    draw_small_icon_scaled(fb, x, y, data_24, data_48);
}

/// Draw a 16×16 tiny icon for the taskbar, using pre-rendered SVGs.
/// HiDPI-aware: at high DPI, uses the 24×24 source for better quality.
pub fn draw_tiny_icon(fb: &mut FrameBuffer, x: i32, y: i32, icon_type: &IconType) {
    let (data_16, data_24): (&[u8], &[u8]) = match icon_type {
        IconType::MyPC => (&icon_data::COMPUTER_16, &icon_data::COMPUTER_24),
        IconType::Folder => (&icon_data::FOLDER_16, &icon_data::FOLDER_24),
        IconType::Document => (&icon_data::DOCUMENT_16, &icon_data::DOCUMENT_24),
        IconType::Globe => (&icon_data::BROWSER_16, &icon_data::BROWSER_24),
        IconType::Terminal => (&icon_data::TERMINAL_16, &icon_data::TERMINAL_24),
        IconType::MediaPlayer => (&icon_data::MEDIA_PLAYER_16, &icon_data::MEDIA_PLAYER_24),
        IconType::Game => (&icon_data::GAME_16, &icon_data::GAME_24),
        IconType::AIBrain => (&icon_data::AI_BRAIN_16, &icon_data::AI_BRAIN_24),
        IconType::Settings => (&icon_data::SETTINGS_16, &icon_data::SETTINGS_24),
        IconType::Trash => (&icon_data::FOLDER_16, &icon_data::FOLDER_24),
        IconType::Image => (&icon_data::DOCUMENT_16, &icon_data::DOCUMENT_24),
        IconType::Archive => (&icon_data::DOCUMENT_16, &icon_data::DOCUMENT_24),
        IconType::Script => (&icon_data::TERMINAL_16, &icon_data::TERMINAL_24),
    };
    draw_tiny_icon_scaled(fb, x, y, data_16, data_24);
}
