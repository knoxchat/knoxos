//! Aurora command-center chrome: panel, tabs, search, bottom bar, power menu.

use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::apps::draw_apps_content;
use super::chat::draw_ai_content;
use super::layout::{
    BOTTOM_BAR_H, DOCK_GAP, PAD_H, PANEL_MAX_HEIGHT, PANEL_RADIUS, PANEL_WIDTH, SEARCH_BAR_H,
    SEARCH_BAR_RADIUS, TAB_BAR_H,
};
use super::state::{START_MENU, StartMenu};
use super::tasks::draw_tasks_content;
use super::types::{CommandCenterTab, VoiceState};

// ═══════════════════════════════════════════════════════════════════════════
// RENDERING — Draw the Aurora Command Center
// ═══════════════════════════════════════════════════════════════════════════

/// Draw the start menu on the framebuffer — Aurora style floating panel
/// Appears centered above the floating dock.
pub fn draw_start_menu(fb: &mut FrameBuffer) {
    if crate::gui::gpui_platform::launcher_is_live() {
        return;
    }
    let menu = START_MENU.lock();
    if !menu.visible {
        return;
    }

    let screen_w = fb.width as i32;
    let screen_h = fb.height as i32;
    let dock_bottom_y = screen_h - crate::gui::scale::taskbar_height() as i32;
    let panel_h = PANEL_MAX_HEIGHT;
    let pw = PANEL_WIDTH as i32;

    // ── POSITION: Centered horizontally, floating above dock ──
    let panel_x = (screen_w - pw) / 2;
    let panel_y = dock_bottom_y - panel_h as i32 - DOCK_GAP;

    // ══════════════════════════════════════════════════════════════════
    // SHADOW — Multi-layer depth shadow
    // ══════════════════════════════════════════════════════════════════
    for i in (0i32..20).step_by(5) {
        let alpha = (18 - i).clamp(0, 16) as u8;
        fb.fill_rounded_rect_aa(
            Rect::new(
                panel_x + 4 - i / 3,
                panel_y + 6 + i / 2,
                PANEL_WIDTH + (i as u32) * 2 / 3,
                panel_h + i as u32 / 2,
            ),
            Pixel::new(0, 0, 0, alpha),
            PANEL_RADIUS + i as u32 / 3,
        );
    }
    // Colored glow shadow
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x - 2, panel_y + 4, PANEL_WIDTH + 4, panel_h + 2),
        Pixel::new(80, 40, 30, 12),
        PANEL_RADIUS + 2,
    );

    // ══════════════════════════════════════════════════════════════════
    // FROSTED GLASS PANEL — Gradient for 3D depth
    // ══════════════════════════════════════════════════════════════════
    let panel_rect = Rect::new(panel_x, panel_y, PANEL_WIDTH, panel_h);
    let panel_top = Pixel::new(28, 26, 34, 235);
    let panel_bottom = Pixel::new(18, 16, 22, 240);
    fb.fill_rounded_rect_gradient_aa(panel_rect, panel_top, panel_bottom, PANEL_RADIUS);

    // ── Multi-layer warm border ──
    fb.draw_rounded_rect(panel_rect, Pixel::new(180, 120, 100, 40), PANEL_RADIUS, 1);
    let inner = Rect::new(panel_x + 1, panel_y + 1, PANEL_WIDTH - 2, panel_h - 2);
    fb.draw_rounded_rect(inner, Pixel::new(200, 160, 140, 16), PANEL_RADIUS - 1, 1);
    // Top shimmer
    fb.draw_hline(
        panel_x + PANEL_RADIUS as i32 + 2,
        panel_y + 1,
        PANEL_WIDTH.saturating_sub(PANEL_RADIUS * 2 + 4),
        Pixel::new(230, 200, 180, 24),
    );

    // Clip content to panel interior
    fb.push_clip(Rect::new(
        panel_x + 1,
        panel_y + 1,
        PANEL_WIDTH - 2,
        panel_h - 2,
    ));

    // ══════════════════════════════════════════════════════════════════
    // TAB BAR — Apps | AI | Tasks
    // ══════════════════════════════════════════════════════════════════
    let tab_y = panel_y + 14;
    draw_tab_bar(fb, panel_x, tab_y, PANEL_WIDTH, &menu);

    // ══════════════════════════════════════════════════════════════════
    // SEARCH BAR (universal)
    // ══════════════════════════════════════════════════════════════════
    let search_y = tab_y + TAB_BAR_H as i32 + 10;
    draw_search_bar(fb, panel_x, search_y, PANEL_WIDTH, &menu);

    // ══════════════════════════════════════════════════════════════════
    // CONTENT AREA — Tab-specific
    // ══════════════════════════════════════════════════════════════════
    let content_y = search_y + SEARCH_BAR_H as i32 + 8;
    let content_h = panel_h as i32 - (content_y - panel_y) - BOTTOM_BAR_H as i32;

    match menu.active_tab {
        CommandCenterTab::Apps => {
            draw_apps_content(fb, panel_x, content_y, PANEL_WIDTH, content_h as u32, &menu);
        }
        CommandCenterTab::AI => {
            draw_ai_content(fb, panel_x, content_y, PANEL_WIDTH, content_h as u32, &menu);
        }
        CommandCenterTab::Tasks => {
            draw_tasks_content(fb, panel_x, content_y, PANEL_WIDTH, content_h as u32, &menu);
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // BOTTOM BAR — User info + power
    // ══════════════════════════════════════════════════════════════════
    let bottom_y = panel_y + panel_h as i32 - BOTTOM_BAR_H as i32;
    draw_bottom_bar(fb, panel_x, bottom_y, PANEL_WIDTH, &menu);

    // ══════════════════════════════════════════════════════════════════
    // POWER MENU POPUP — Shutdown / Restart / Log Out
    // ══════════════════════════════════════════════════════════════════
    if menu.power_menu_visible {
        let pm_w = 120i32;
        let pm_item_h = 30i32;
        let pm_h = pm_item_h * 3 + 8;
        let pm_x = panel_x + PANEL_WIDTH as i32 - PAD_H - pm_w - 10;
        let pm_y = bottom_y - pm_h - 4;

        // Popup background (dark glassmorphism)
        fb.fill_rounded_rect(
            Rect::new(pm_x, pm_y, pm_w as u32, pm_h as u32),
            Pixel::new(28, 26, 34, 240),
            10,
        );
        fb.draw_rounded_rect(
            Rect::new(pm_x, pm_y, pm_w as u32, pm_h as u32),
            Pixel::new(180, 130, 110, 50),
            10,
            1,
        );

        let items = [
            ("\u{23FB} Shut Down", Pixel::new(220, 85, 85, 220)), // ⏻ power
            ("\u{21BB} Restart", Pixel::new(230, 180, 90, 220)),  // ↻ restart
            ("\u{2192} Log Out", Pixel::new(140, 170, 200, 220)), // → logout
        ];

        for (i, (label, color)) in items.iter().enumerate() {
            let iy = pm_y + 4 + i as i32 * pm_item_h;
            // Hover highlight (approximate — actual hover would need mouse tracking)
            fb.fill_rounded_rect(
                Rect::new(pm_x + 4, iy, (pm_w - 8) as u32, pm_item_h as u32 - 2),
                Pixel::new(50, 45, 55, 60),
                6,
            );
            font_engine::draw_ui_text(fb, pm_x + 12, iy + 8, label, 12, *color);
        }
    }

    fb.pop_clip();
}

// ═══════════════════════════════════════════════════════════════════════════
// TAB BAR — Segmented control for Apps / AI / Tasks
// ═══════════════════════════════════════════════════════════════════════════

fn draw_tab_bar(fb: &mut FrameBuffer, panel_x: i32, tab_y: i32, panel_w: u32, menu: &StartMenu) {
    let tab_labels = ["Apps", "AI", "Tasks"];
    let tab_values = [
        CommandCenterTab::Apps,
        CommandCenterTab::AI,
        CommandCenterTab::Tasks,
    ];
    let tab_count = tab_labels.len() as i32;
    let tab_total_w = (panel_w as i32 - PAD_H * 2) as u32;
    let tab_w = tab_total_w / tab_count as u32;
    let tab_h = TAB_BAR_H - 4;

    // Tab bar background
    let bar_rect = Rect::new(panel_x + PAD_H, tab_y, tab_total_w, TAB_BAR_H);
    fb.fill_rounded_rect_aa(bar_rect, Pixel::new(22, 20, 28, 180), 10);

    for (i, &label) in tab_labels.iter().enumerate() {
        let tx = panel_x + PAD_H + (i as u32 * tab_w) as i32 + 2;
        let ty = tab_y + 2;
        let tw = tab_w - 4;
        let is_active = menu.active_tab == tab_values[i];

        if is_active {
            fb.fill_rounded_rect_aa(
                Rect::new(tx, ty, tw, tab_h),
                colors::with_alpha(colors::ACCENT_PRIMARY, 45),
                8,
            );
            fb.draw_rounded_rect(
                Rect::new(tx, ty, tw, tab_h),
                colors::with_alpha(colors::ACCENT_BRIGHT, 60),
                8,
                1,
            );
        }

        let icon_x = tx + 10;
        let text_x = icon_x + 16;
        let text_y = ty + (tab_h as i32 - 12) / 2;

        let icon_color = if is_active {
            colors::ACCENT_BRIGHT
        } else {
            Pixel::new(140, 120, 110, 160)
        };

        match tab_values[i] {
            CommandCenterTab::Apps => {
                let ic = icon_x + 2;
                let iy = ty + (tab_h as i32 - 10) / 2;
                fb.fill_rounded_rect_aa(Rect::new(ic, iy, 4, 4), icon_color, 1);
                fb.fill_rounded_rect_aa(Rect::new(ic + 6, iy, 4, 4), icon_color, 1);
                fb.fill_rounded_rect_aa(Rect::new(ic, iy + 6, 4, 4), icon_color, 1);
                fb.fill_rounded_rect_aa(Rect::new(ic + 6, iy + 6, 4, 4), icon_color, 1);
            }
            CommandCenterTab::AI => {
                let ic = icon_x + 6;
                let iy = ty + tab_h as i32 / 2;
                fb.fill_circle_aa(ic, iy, 4, icon_color);
                fb.draw_circle_aa(
                    ic,
                    iy,
                    6,
                    Pixel::new(icon_color.r, icon_color.g, icon_color.b, icon_color.a / 2),
                );
            }
            CommandCenterTab::Tasks => {
                let ic = icon_x + 2;
                let iy = ty + (tab_h as i32 - 10) / 2;
                fb.fill_rounded_rect_aa(Rect::new(ic, iy, 3, 3), icon_color, 1);
                fb.draw_hline(ic + 5, iy + 1, 6, icon_color);
                fb.fill_rounded_rect_aa(Rect::new(ic, iy + 5, 3, 3), icon_color, 1);
                fb.draw_hline(ic + 5, iy + 6, 6, icon_color);
            }
        }

        let text_color = if is_active {
            Pixel::new(245, 238, 230, 255)
        } else {
            Pixel::new(140, 125, 115, 180)
        };
        font_engine::draw_ui_bold(fb, text_x, text_y, label, 12, text_color);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SEARCH BAR — Universal search with AI mode indicator & voice input
// ═══════════════════════════════════════════════════════════════════════════

fn draw_search_bar(
    fb: &mut FrameBuffer,
    panel_x: i32,
    search_y: i32,
    panel_w: u32,
    menu: &StartMenu,
) {
    let sx = panel_x + PAD_H;
    let sw = (panel_w as i32 - PAD_H * 2) as u32;

    fb.fill_rounded_rect_aa(
        Rect::new(sx, search_y, sw, SEARCH_BAR_H),
        Pixel::new(26, 24, 32, 210),
        SEARCH_BAR_RADIUS,
    );

    let border_color = if menu.search_active && menu.active_tab == CommandCenterTab::Apps {
        colors::with_alpha(colors::ACCENT_PRIMARY, 60)
    } else if menu.active_tab == CommandCenterTab::AI {
        Pixel::new(180, 140, 200, 50)
    } else {
        Pixel::new(80, 70, 65, 35)
    };
    fb.draw_rounded_rect(
        Rect::new(sx, search_y, sw, SEARCH_BAR_H),
        border_color,
        SEARCH_BAR_RADIUS,
        1,
    );

    // Magnifying glass icon
    let mag_cx = sx + 18;
    let mag_cy = search_y + SEARCH_BAR_H as i32 / 2;
    let mag_color = if menu.active_tab == CommandCenterTab::AI {
        Pixel::new(180, 140, 200, 180)
    } else {
        Pixel::new(180, 140, 120, 180)
    };
    fb.draw_circle_aa(mag_cx, mag_cy - 1, 6, mag_color);
    fb.draw_line_aa(mag_cx + 4, mag_cy + 3, mag_cx + 8, mag_cy + 7, mag_color);

    // Text or placeholder
    let text_x = sx + 34;
    let text_y = search_y + (SEARCH_BAR_H as i32 - 12) / 2;

    let display_text = match menu.active_tab {
        CommandCenterTab::Apps => {
            if menu.search_text.is_empty() {
                None
            } else {
                Some(menu.search_text.as_str())
            }
        }
        CommandCenterTab::AI => {
            if menu.chat_input.is_empty() {
                None
            } else {
                Some(menu.chat_input.as_str())
            }
        }
        CommandCenterTab::Tasks => None,
    };

    if let Some(text) = display_text {
        font_engine::draw_ui_text(fb, text_x, text_y, text, 12, Pixel::new(220, 230, 250, 240));
        let cursor_x = text_x + font_engine::measure_ui_text(text, 12) as i32 + 1;
        fb.fill_rect(
            Rect::new(cursor_x, text_y, 2, 12),
            Pixel::new(0, 200, 255, 200),
        );
    } else {
        let placeholder = match menu.active_tab {
            CommandCenterTab::Apps => "Search apps...",
            CommandCenterTab::AI => "Ask KnoxOS AI anything...",
            CommandCenterTab::Tasks => "Filter tasks...",
        };
        font_engine::draw_ui_text(
            fb,
            text_x,
            text_y,
            placeholder,
            12,
            Pixel::new(110, 100, 90, 120),
        );
    }

    // Voice input button (microphone)
    let mic_cx = sx + sw as i32 - 28;
    let mic_cy = search_y + SEARCH_BAR_H as i32 / 2;
    let mic_color = match menu.voice_state {
        VoiceState::Idle => Pixel::new(80, 130, 200, 150),
        VoiceState::Listening => Pixel::new(255, 80, 80, 255),
        VoiceState::Processing => Pixel::new(255, 200, 0, 200),
    };
    fb.fill_rounded_rect_aa(Rect::new(mic_cx - 3, mic_cy - 6, 6, 9), mic_color, 3);
    fb.draw_circle_aa(
        mic_cx,
        mic_cy + 2,
        5,
        Pixel::new(mic_color.r, mic_color.g, mic_color.b, mic_color.a / 2),
    );
    fb.draw_vline(mic_cx, mic_cy + 6, 3, mic_color);
    fb.draw_hline(mic_cx - 2, mic_cy + 9, 5, mic_color);

    if menu.voice_state == VoiceState::Listening {
        fb.draw_circle_aa(mic_cx, mic_cy, 10, Pixel::new(255, 60, 60, 80));
        fb.draw_circle_aa(mic_cx, mic_cy, 12, Pixel::new(255, 60, 60, 40));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BOTTOM BAR — User profile, power, settings shortcuts
// ═══════════════════════════════════════════════════════════════════════════

fn draw_bottom_bar(
    fb: &mut FrameBuffer,
    panel_x: i32,
    bottom_y: i32,
    panel_w: u32,
    _menu: &StartMenu,
) {
    let pw = panel_w as i32;

    fb.draw_hline(
        panel_x + PAD_H,
        bottom_y,
        (pw - PAD_H * 2).max(0) as u32,
        Pixel::new(50, 80, 140, 20),
    );

    let bar_cy = bottom_y + BOTTOM_BAR_H as i32 / 2;

    // User avatar
    let user_cx = panel_x + PAD_H + 16;
    fb.fill_circle_aa(user_cx, bar_cy, 12, Pixel::new(25, 50, 90, 180));
    fb.draw_circle_aa(user_cx, bar_cy, 12, Pixel::new(60, 120, 200, 60));
    fb.fill_circle_aa(user_cx, bar_cy - 4, 4, Pixel::new(100, 160, 220, 160));
    fb.fill_circle_aa(user_cx, bar_cy + 5, 7, Pixel::new(80, 140, 200, 80));
    font_engine::draw_ui_text(
        fb,
        user_cx + 18,
        bar_cy - 6,
        "User",
        12,
        Pixel::new(160, 190, 230, 200),
    );

    // Settings gear
    let settings_cx = panel_x + pw - PAD_H - 50;
    fb.draw_circle_aa(settings_cx, bar_cy, 7, Pixel::new(80, 120, 180, 140));
    fb.fill_circle_aa(settings_cx, bar_cy, 3, Pixel::new(80, 120, 180, 100));

    // Power button
    let power_cx = panel_x + pw - PAD_H - 18;
    let power_col = Pixel::new(200, 70, 70, 190);
    fb.draw_circle_aa(power_cx, bar_cy, 9, power_col);
    fb.draw_vline(power_cx, bar_cy - 12, 7, power_col);
}
