/// Aurora floating dock paint — pill, launcher, workspaces, and app entries.
use alloc::string::String;
use core::fmt::Write;

use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::icon_theme;
use crate::gui::window;

use super::layout::compute_dock_layout;
use super::state::{
    DOCK_BAR_HEIGHT, DOCK_ITEM_SIZE, DOCK_RADIUS, ENTRY_ICON_SIZE, MAX_DOCK_ENTRIES, TASKBAR,
};

/// Draw the Aurora floating dock — centered pill-shaped bar with warm app dots
pub fn draw_taskbar(fb: &mut FrameBuffer) {
    let taskbar = TASKBAR.lock();
    let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);
    let layout = compute_dock_layout(entry_count, fb.width as u32, fb.height as u32);
    let dock_x = layout.dock_x;
    let dock_y = layout.dock_y;
    let dock_w = layout.dock_w;
    let dock_cy = layout.dock_cy;
    let entry_spacing = layout.entry_spacing;
    let separator_w = layout.separator_w;
    let workspace_dot_size = layout.workspace_dot_size;
    let workspace_dot_gap = layout.workspace_dot_gap;
    let workspace_count = layout.workspace_count;

    // ══════════════════════════════════════════════════════════
    // LIGHTWEIGHT DOCK SHADOW — Fast floating effect
    // Two compact passes instead of 3 + per-pixel glow
    // ══════════════════════════════════════════════════════════
    {
        // Layer 1: Soft ambient shadow (4 iterations instead of 20)
        for offset in (0i32..16).step_by(4) {
            let alpha = (14 - offset).clamp(0, 12) as u8;
            let shadow_rect = Rect::new(
                dock_x - offset / 2,
                dock_y + 6 + offset / 2,
                dock_w + offset as u32,
                DOCK_BAR_HEIGHT + offset as u32 / 2,
            );
            fb.draw_rounded_rect(
                shadow_rect,
                Pixel::new(0, 0, 0, alpha),
                DOCK_RADIUS + offset as u32 / 3,
                1,
            );
        }
        // Layer 2: Sharp contact shadow (2 iterations instead of 5)
        for offset in 0i32..3 {
            let alpha = ((3 - offset) * 10).min(30) as u8;
            let shadow_rect = Rect::new(
                dock_x - offset,
                dock_y + 3 + offset,
                dock_w + offset as u32 * 2,
                DOCK_BAR_HEIGHT + offset as u32,
            );
            fb.draw_rounded_rect(shadow_rect, Pixel::new(40, 20, 15, alpha), DOCK_RADIUS, 1);
        }
    }

    // ══════════════════════════════════════════════════════════
    // DOCK PILL — Enhanced glassmorphism with gradient depth
    // Modern UI trend: slightly lighter on top for 3D effect
    // ══════════════════════════════════════════════════════════
    let dock_rect = Rect::new(dock_x, dock_y, dock_w, DOCK_BAR_HEIGHT);

    // Glassmorphism with vertical gradient (lighter at top for depth)
    let dock_top = Pixel::new(30, 28, 36, 180);
    let dock_bottom = Pixel::new(22, 20, 28, 170);
    fb.fill_rounded_rect_gradient_aa(dock_rect, dock_top, dock_bottom, DOCK_RADIUS);

    // Multi-layer warm border (creates depth)
    // Outer border with warm accent
    fb.draw_rounded_rect(
        dock_rect,
        colors::with_alpha(colors::ACCENT_PRIMARY, 50),
        DOCK_RADIUS,
        1,
    );

    // Inner highlight for 3D bevel effect
    let inner_rect = Rect::new(dock_x + 1, dock_y + 1, dock_w - 2, DOCK_BAR_HEIGHT - 2);
    fb.draw_rounded_rect(
        inner_rect,
        Pixel::new(220, 180, 160, 20),
        DOCK_RADIUS - 1,
        1,
    );

    // Top edge warm highlight (hline instead of per-pixel loop)
    let hl_x0 = dock_x + DOCK_RADIUS as i32 + 2;
    let hl_w = dock_w.saturating_sub(DOCK_RADIUS * 2 + 4);
    fb.draw_hline(hl_x0, dock_y + 1, hl_w, Pixel::new(240, 210, 190, 28));
    fb.draw_hline(hl_x0, dock_y + 2, hl_w, Pixel::new(210, 180, 160, 16));

    // ══════════════════════════════════════════════════════════
    // DOCK CONTENT LAYOUT
    // ══════════════════════════════════════════════════════════
    let mut cursor_x = layout.content_x;

    let (hover_mx, hover_my) = {
        let m = crate::gui::input::MOUSE.lock();
        (m.x, m.y)
    };

    // ── START LAUNCHER (app grid icon) ──────────────────────
    {
        let btn_rect = Rect::new(
            cursor_x,
            dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );
        let hovered = btn_rect.contains(hover_mx, hover_my);
        let menu_open = crate::gui::startmenu::is_visible();

        if menu_open || hovered {
            fb.fill_rounded_rect_aa(btn_rect, colors::with_alpha(colors::ACCENT_PRIMARY, 50), 8);
        }

        // App grid icon (9 dots in 3×3 grid)
        let icon_cx = cursor_x + DOCK_ITEM_SIZE as i32 / 2;
        let dot_r = 2u32;
        let dot_spacing = 7i32;
        let dot_color = if hovered {
            colors::ACCENT_BRIGHT
        } else {
            Pixel::new(140, 168, 165, 200)
        };
        for row in -1..=1i32 {
            for col in -1..=1i32 {
                fb.fill_circle_aa(
                    icon_cx + col * dot_spacing,
                    dock_cy + row * dot_spacing,
                    dot_r,
                    dot_color,
                );
            }
        }

        cursor_x += DOCK_ITEM_SIZE as i32 + entry_spacing;
    }

    // ── WORKSPACE INDICATOR (4 small dots/pills) ──────────────
    {
        use core::sync::atomic::Ordering;
        let current_ws = crate::gui::window::CURRENT_WORKSPACE.load(Ordering::Relaxed);
        for ws_i in 0..workspace_count {
            let dot_x = cursor_x;
            let dot_y = dock_cy - workspace_dot_size / 2;
            let dot_rect = Rect::new(
                dot_x,
                dot_y,
                workspace_dot_size as u32,
                workspace_dot_size as u32,
            );
            let is_active = ws_i as u8 == current_ws;
            let is_hovered = dot_rect.contains(hover_mx, hover_my);

            if is_active {
                // Active workspace: teal pill
                fb.fill_rounded_rect_aa(
                    dot_rect,
                    colors::with_alpha(colors::ACCENT_PRIMARY, 180),
                    4,
                );
                // Glow effect
                let glow_rect = Rect::new(
                    dot_x - 1,
                    dot_y - 1,
                    workspace_dot_size as u32 + 2,
                    workspace_dot_size as u32 + 2,
                );
                fb.draw_rounded_rect(
                    glow_rect,
                    colors::with_alpha(colors::ACCENT_PRIMARY, 50),
                    5,
                    1,
                );
            } else if is_hovered {
                fb.fill_rounded_rect_aa(
                    dot_rect,
                    colors::with_alpha(colors::ACCENT_PRIMARY, 80),
                    4,
                );
            } else {
                // Inactive: dim pill
                fb.fill_rounded_rect_aa(dot_rect, Pixel::new(60, 55, 50, 100), 4);
            }

            // Draw workspace number (1-indexed)
            let num_char = match ws_i {
                0 => "1",
                1 => "2",
                2 => "3",
                _ => "4",
            };
            let text_w = font_engine::measure_ui_text(num_char, 11) as i32;
            let text_x = dot_x + (workspace_dot_size - text_w) / 2;
            let text_y = dot_y + (workspace_dot_size - 11) / 2;
            let text_color = if is_active {
                Pixel::new(255, 255, 255, 255)
            } else {
                Pixel::new(180, 170, 160, 180)
            };
            font_engine::draw_ui_text(fb, text_x, text_y, num_char, 11, text_color);

            cursor_x += workspace_dot_size + workspace_dot_gap;
        }
        cursor_x += entry_spacing - workspace_dot_gap; // adjust spacing before apps
    }

    // ── APP ENTRIES — Icon-only squares with active indicators ──
    for (i, entry) in taskbar.entries.iter().enumerate() {
        if i >= MAX_DOCK_ENTRIES {
            break;
        }

        let item_rect = Rect::new(
            cursor_x,
            dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );
        let hovered = taskbar.hovered_entry == Some(i);

        // Background glow on hover or active
        if entry.active {
            fb.fill_rounded_rect_aa(item_rect, colors::with_alpha(colors::ACCENT_PRIMARY, 65), 8);
        } else if hovered {
            fb.fill_rounded_rect_aa(item_rect, colors::with_alpha(colors::ACCENT_PRIMARY, 50), 8);
        }

        // Draw app icon (20×20 centered in 36×36 cell)
        let icon_x = cursor_x + (DOCK_ITEM_SIZE as i32 - ENTRY_ICON_SIZE as i32) / 2;
        let icon_y = dock_cy - ENTRY_ICON_SIZE as i32 / 2;
        draw_taskbar_icon(fb, icon_x, icon_y, entry.content_type);

        // Active indicator dot — glowing cyan dot below icon
        if entry.active {
            fb.fill_circle_aa(
                cursor_x + DOCK_ITEM_SIZE as i32 / 2,
                dock_y + DOCK_BAR_HEIGHT as i32 - 6,
                3,
                colors::DOCK_INDICATOR_ACTIVE,
            );
            // Subtle glow halo
            fb.fill_circle_aa(
                cursor_x + DOCK_ITEM_SIZE as i32 / 2,
                dock_y + DOCK_BAR_HEIGHT as i32 - 6,
                5,
                colors::DOCK_INDICATOR_GLOW,
            );
        } else {
            // Inactive: dim dot
            fb.fill_circle_aa(
                cursor_x + DOCK_ITEM_SIZE as i32 / 2,
                dock_y + DOCK_BAR_HEIGHT as i32 - 6,
                2,
                Pixel::new(140, 120, 110, 100),
            );
        }

        // Notification badge — red circle with count in top-right corner
        if entry.badge_count > 0 {
            let badge_cx = cursor_x + DOCK_ITEM_SIZE as i32 - 6;
            let badge_cy = dock_cy - DOCK_ITEM_SIZE as i32 / 2 + 5;
            // Red glow halo
            fb.fill_circle_aa(badge_cx, badge_cy, 7, Pixel::new(255, 40, 40, 60));
            // Badge circle
            fb.fill_circle_aa(badge_cx, badge_cy, 5, Pixel::rgb(230, 50, 50));
            // Count digit (or 9+ if >= 10)
            let badge_str = if entry.badge_count >= 10 {
                alloc::string::String::from("9")
            } else {
                let mut s = alloc::string::String::new();
                let _ = core::fmt::Write::write_fmt(&mut s, format_args!("{}", entry.badge_count));
                s
            };
            fonts::draw_char_bold_compact(
                fb,
                badge_cx - 3,
                badge_cy - 5,
                badge_str.chars().next().unwrap_or('0'),
                colors::WHITE,
                1,
            );
            if entry.badge_count >= 10 {
                fonts::draw_char_bold_compact(
                    fb,
                    badge_cx + 1,
                    badge_cy - 5,
                    '+',
                    colors::WHITE,
                    1,
                );
            }
        }

        cursor_x += DOCK_ITEM_SIZE as i32;
    }

    // ── SEPARATOR LINE (vertical divider before system tray) ────
    cursor_x += separator_w / 2;
    fb.fill_rounded_rect_aa(
        Rect::new(cursor_x - 1, dock_cy - 14, 2, 28),
        Pixel::new(160, 130, 120, 40),
        1,
    );
    cursor_x += separator_w / 2;

    // D9.5: system tray icons live in the GPUI panel; the legacy dock
    // no longer paints fill_rect tray glyphs.
    let _ = cursor_x;
}

/// Draw a tiny icon (16×16) in the taskbar for a given window content type
fn draw_taskbar_icon(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    content_type: window::WindowContentType,
) {
    icon_theme::draw_tiny_icon(fb, x, y, icon_theme::for_content(content_type));
}
