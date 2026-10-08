/// Settings chrome — sidebar, content dispatch, scrollbar.
use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::draw_focus_indicator;
use super::{
    FocusRegion, SCROLLBAR_WIDTH, SIDEBAR_WIDTH, SettingsState, SettingsTab, TAB_HEIGHT, TABS,
};

/// Draw settings content inside a window's content area.
/// `scroll_y` is the window's scroll offset for the content pane.
/// Returns the total virtual content height (used by the caller to set max_scroll_y).
pub fn draw_settings(
    fb: &mut FrameBuffer,
    content_rect: Rect,
    state: &SettingsState,
    scroll_y: i32,
) -> i32 {
    let cx = content_rect.x;
    let cy = content_rect.y;
    let cw = content_rect.width;
    let ch = content_rect.height;

    // Background
    fb.fill_rect(content_rect, Pixel::rgb(24, 24, 28));

    // ── Sidebar ──────────────────────────
    fb.fill_rect(
        Rect::new(cx, cy, SIDEBAR_WIDTH as u32, ch),
        Pixel::rgb(30, 30, 34),
    );

    // Title
    font_engine::draw_ui_bold(fb, cx + 16, cy + 12, "Settings", 16, colors::WHITE);
    fb.draw_hline(
        cx + 12,
        cy + 34,
        (SIDEBAR_WIDTH - 24) as u32,
        Pixel::rgb(50, 50, 55),
    );

    let (mx, my) = {
        let mouse = crate::gui::input::MOUSE.lock();
        (mouse.x, mouse.y)
    };

    // Tab items
    for (i, (tab, label, icon)) in TABS.iter().enumerate() {
        let ty = cy + 44 + i as i32 * TAB_HEIGHT;
        let is_active = state.active_tab == *tab;
        let tab_rect = Rect::new(
            cx + 6,
            ty,
            (SIDEBAR_WIDTH - 12) as u32,
            TAB_HEIGHT as u32 - 4,
        );
        let hovered = !is_active && tab_rect.contains(mx, my);

        if is_active {
            fb.fill_rounded_rect_aa(tab_rect, Pixel::rgb(55, 55, 60), 4);
            // Active indicator bar
            fb.fill_rounded_rect_aa(
                Rect::new(cx + 4, ty + 6, 3, (TAB_HEIGHT - 16) as u32),
                Pixel::rgb(82, 139, 255),
                1,
            );
        } else if hovered {
            fb.fill_rounded_rect_aa(tab_rect, Pixel::rgb(42, 42, 48), 4);
        }

        // Icon circle
        let icon_bg = if is_active {
            Pixel::rgb(82, 139, 255)
        } else if hovered {
            Pixel::rgb(70, 70, 80)
        } else {
            Pixel::rgb(50, 50, 55)
        };
        fb.fill_circle_aa(cx + 24, ty + TAB_HEIGHT / 2 - 2, 10, icon_bg);
        fonts::draw_char_bold_compact(
            fb,
            cx + 20,
            ty + TAB_HEIGHT / 2 - 8,
            icon.chars().next().unwrap_or('?'),
            colors::WHITE,
            1,
        );

        let text_color = if is_active {
            colors::WHITE
        } else if hovered {
            Pixel::rgb(220, 220, 230)
        } else {
            Pixel::rgb(160, 160, 160)
        };
        if is_active {
            font_engine::draw_ui_bold(
                fb,
                cx + 42,
                ty + (TAB_HEIGHT - 13) / 2,
                label,
                13,
                text_color,
            );
        } else {
            font_engine::draw_ui_text(
                fb,
                cx + 42,
                ty + (TAB_HEIGHT - 13) / 2,
                label,
                13,
                text_color,
            );
        }

        // Draw focus ring on sidebar tab if keyboard nav is active
        if state.keyboard_nav {
            if let FocusRegion::Sidebar(fi) = state.focus_region() {
                if fi == i as i32 {
                    let tab_rect = Rect::new(
                        cx + 6,
                        ty,
                        (SIDEBAR_WIDTH - 12) as u32,
                        TAB_HEIGHT as u32 - 4,
                    );
                    draw_focus_indicator(fb, tab_rect, true);
                }
            }
        }
    }

    // ── Content area (scrollable) ─────────────────────
    let content_x = cx + SIDEBAR_WIDTH;
    let content_y = cy;
    let content_w = cw as i32 - SIDEBAR_WIDTH;
    let content_h = ch;

    // Vertical separator
    fb.draw_vline(content_x, cy, ch, Pixel::rgb(45, 45, 50));

    // Clip content to the pane right of the sidebar
    let content_clip = Rect::new(content_x, content_y, content_w as u32, content_h);
    fb.push_clip(content_clip);

    // Determine which content item is focused (-1 if sidebar or nav inactive)
    let focused_content = if state.keyboard_nav {
        match state.focus_region() {
            FocusRegion::Content(idx) => idx,
            _ => -1,
        }
    } else {
        -1
    };

    // Draw current tab with scroll offset applied
    let total_h = match state.active_tab {
        SettingsTab::Display => super::display::draw_display_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Sound => super::sound::draw_sound_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Network => super::network::draw_network_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Personalization => super::personalization::draw_personalization_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Users => super::users::draw_users_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::System => super::system::draw_system_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::DateTime => super::datetime::draw_datetime_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Privacy => super::privacy::draw_privacy_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Startup => super::startup::draw_startup_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::About => {
            super::about::draw_about_tab(fb, content_x, content_y - scroll_y, content_w)
        }
    };

    fb.pop_clip();

    // ── Scrollbar (drawn on top, inside content pane) ──────────────
    let visible_h = content_h as i32;
    if total_h > visible_h {
        let sb_x = cx + cw as i32 - SCROLLBAR_WIDTH - 2;
        let sb_track = Rect::new(
            sb_x,
            content_y + 2,
            SCROLLBAR_WIDTH as u32,
            content_h.saturating_sub(4),
        );
        fb.fill_rounded_rect_aa(
            sb_track,
            colors::SCROLLBAR_TRACK,
            (SCROLLBAR_WIDTH / 2) as u32,
        );

        let visible_ratio = visible_h as f32 / total_h as f32;
        let thumb_h = ((visible_ratio * visible_h as f32) as u32)
            .max(20)
            .min(content_h.saturating_sub(4));
        let max_scroll = (total_h - visible_h).max(1);
        let scroll_ratio = scroll_y as f32 / max_scroll as f32;
        let track_space = content_h.saturating_sub(4 + thumb_h) as f32;
        let thumb_y = content_y + 2 + (scroll_ratio.clamp(0.0, 1.0) * track_space) as i32;

        fb.fill_rounded_rect_aa(
            Rect::new(sb_x + 1, thumb_y, (SCROLLBAR_WIDTH - 2) as u32, thumb_h),
            colors::SCROLLBAR_THUMB,
            ((SCROLLBAR_WIDTH - 2) / 2) as u32,
        );
    }

    total_h
}
