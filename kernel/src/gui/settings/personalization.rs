/// Personalization tab — themes, accent color, cursor style.
use crate::gui::colors;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{draw_section_header, draw_toggle_row, ttf, ttf_b};

// ═══════════════════════════════════════════════════════════════════════════
// PERSONALIZATION TAB
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_personalization_tab(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    focus_idx: i32,
) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Personalization");

    ttf_b(fb, x + 24, y + 42, "Theme", Pixel::rgb(140, 140, 140));

    // Theme swatches — use actual theme system
    let active_theme = crate::gui::theme::active_theme();
    let themes: [(crate::gui::theme::ThemeId, &str, Pixel); 3] = [
        (
            crate::gui::theme::ThemeId::NebulaDark,
            "Nebula Dark",
            Pixel::rgb(8, 12, 24),
        ),
        (
            crate::gui::theme::ThemeId::ArcticLight,
            "Arctic Light",
            Pixel::rgb(245, 248, 252),
        ),
        (
            crate::gui::theme::ThemeId::SunsetWarm,
            "Sunset Warm",
            Pixel::rgb(28, 20, 16),
        ),
    ];

    for (i, (theme_id, name, color)) in themes.iter().enumerate() {
        let sx = x + 24 + i as i32 * 82;
        let sy = y + 60;
        fb.fill_rounded_rect_aa(Rect::new(sx, sy, 72, 36), *color, 4);
        if active_theme == *theme_id {
            let accent = crate::gui::theme::accent_color();
            fb.draw_rounded_rect(Rect::new(sx - 1, sy - 1, 74, 38), accent, 4, 2);
        }
        let label_color = if active_theme == *theme_id {
            crate::gui::theme::accent_color()
        } else {
            Pixel::rgb(120, 120, 120)
        };
        ttf(fb, sx + 2, sy + 40, name, label_color);
    }

    // Accent color
    fb.draw_hline(x + 16, y + 115, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    ttf_b(
        fb,
        x + 24,
        y + 125,
        "Accent Color",
        Pixel::rgb(140, 140, 140),
    );

    let current_accent = crate::gui::theme::accent_index();
    for (i, color) in crate::gui::theme::ACCENT_COLORS.iter().enumerate() {
        let ax = x + 24 + i as i32 * 32;
        let ay = y + 145;
        if i as u8 == current_accent {
            fb.fill_circle_aa(ax + 10, ay + 10, 12, colors::WHITE);
        }
        fb.fill_circle_aa(ax + 10, ay + 10, 10, *color);
    }

    fb.draw_hline(x + 16, y + 178, (w - 32) as u32, Pixel::rgb(45, 45, 50));

    // Cursor Theme
    ttf_b(
        fb,
        x + 24,
        y + 188,
        "Cursor Style",
        Pixel::rgb(140, 140, 140),
    );

    let current_cursor = crate::gui::desktop::cursor_theme();
    let cursor_themes: [(&str, crate::gui::desktop::CursorTheme); 4] = [
        ("White", crate::gui::desktop::CursorTheme::Default),
        ("Dark", crate::gui::desktop::CursorTheme::Dark),
        ("Accent", crate::gui::desktop::CursorTheme::Accent),
        ("Large", crate::gui::desktop::CursorTheme::Large),
    ];

    for (i, (name, theme)) in cursor_themes.iter().enumerate() {
        let bx = x + 24 + i as i32 * 55;
        let by = y + 206;
        let is_active = current_cursor == *theme;
        let bg = if is_active {
            Pixel::new(60, 80, 120, 180)
        } else {
            Pixel::rgb(40, 42, 48)
        };
        fb.fill_rounded_rect_aa(Rect::new(bx, by, 48, 22), bg, 4);
        if is_active {
            fb.draw_rounded_rect(
                Rect::new(bx - 1, by - 1, 50, 24),
                crate::gui::theme::accent_color(),
                4,
                1,
            );
        }
        let tc = if is_active {
            Pixel::rgb(220, 235, 255)
        } else {
            Pixel::rgb(120, 120, 120)
        };
        ttf(fb, bx + 4, by + 5, name, tc);
    }

    fb.draw_hline(x + 16, y + 240, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_toggle_row(
        fb,
        x,
        y + 252,
        w,
        "Transparent Taskbar",
        "Enable taskbar transparency",
        true,
    );
    draw_toggle_row(
        fb,
        x,
        y + 292,
        w,
        "Window Animations",
        "Enable window effects",
        true,
    );

    340
}
