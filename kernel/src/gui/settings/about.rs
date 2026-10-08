/// About tab — OS logo, version, and system facts.
use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::ARCH_NAME;
use super::widgets::{draw_info_row, ttf_centered, ttf_centered_b};

// ═══════════════════════════════════════════════════════════════════════════
// ABOUT TAB
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_about_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32) -> i32 {
    // OS logo area
    let logo_y = y + 24;
    let logo_size: i32 = 56;
    let center_x = x + w / 2;

    // K logo circle
    fb.fill_circle_aa(
        center_x,
        logo_y + logo_size / 2,
        (logo_size / 2) as u32,
        Pixel::rgb(82, 139, 255),
    );
    font_engine::draw_ui_bold(
        fb,
        center_x - 10,
        logo_y + logo_size / 2 - 12,
        "K",
        28,
        colors::WHITE,
    );

    // OS name
    ttf_centered_b(
        fb,
        x,
        logo_y + logo_size + 8,
        w as u32,
        "KnoxOS",
        22,
        colors::WHITE,
    );

    ttf_centered(
        fb,
        x,
        logo_y + logo_size + 34,
        w as u32,
        "Version 0.2.3",
        13,
        Pixel::rgb(140, 140, 140),
    );

    fb.draw_hline(
        x + 16,
        logo_y + logo_size + 52,
        (w - 32) as u32,
        Pixel::rgb(50, 50, 55),
    );

    let info_y = logo_y + logo_size + 64;
    draw_info_row(fb, x, info_y, "Architecture", ARCH_NAME);
    draw_info_row(fb, x, info_y + 20, "Language", "Rust (no_std)");
    draw_info_row(fb, x, info_y + 40, "Bootloader", "bootloader_api 0.11");
    draw_info_row(fb, x, info_y + 60, "Heap Size", "32 MiB");
    draw_info_row(fb, x, info_y + 80, "Font", "Hack 10x20 AA");
    draw_info_row(fb, x, info_y + 100, "Theme", "Tokyo Night");

    fb.draw_hline(
        x + 16,
        info_y + 122,
        (w - 32) as u32,
        Pixel::rgb(50, 50, 55),
    );

    ttf_centered(
        fb,
        x,
        info_y + 136,
        w as u32,
        "KnoxOS: An AI-native operating system written entirely in Rust.",
        13,
        Pixel::rgb(120, 120, 120),
    );
    ttf_centered(
        fb,
        x,
        info_y + 154,
        w as u32,
        "github.com/knoxchat/knoxos",
        13,
        Pixel::rgb(82, 139, 255),
    );

    // Total content height
    info_y - y + 190
}
