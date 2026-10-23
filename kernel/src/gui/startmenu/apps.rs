//! Apps-tab list, category badges, and empty-state paint.

use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::icon_theme;

use super::layout::{ICON_TEXT_GAP, ITEM_ICON_SIZE, ITEM_INSET, MENU_ITEM_HEIGHT, PAD_H};
use super::state::StartMenu;
use super::types::MenuCategory;

// ═══════════════════════════════════════════════════════════════════════════
// APPS CONTENT — Pinned apps, all apps list with fuzzy search & categories
// ═══════════════════════════════════════════════════════════════════════════

pub(super) fn draw_apps_content(
    fb: &mut FrameBuffer,
    panel_x: i32,
    content_y: i32,
    panel_w: u32,
    content_h: u32,
    menu: &StartMenu,
) {
    let mut cur_y = content_y;

    // Section header
    if !menu.search_text.is_empty() && !menu.filtered_indices.is_empty() {
        let result_text = alloc::format!(
            "{} result{}",
            menu.filtered_indices.len(),
            if menu.filtered_indices.len() == 1 {
                ""
            } else {
                "s"
            }
        );
        font_engine::draw_ui_text(
            fb,
            panel_x + PAD_H + 4,
            cur_y + 2,
            &result_text,
            12,
            Pixel::new(80, 140, 220, 140),
        );
        cur_y += 18;
    } else {
        font_engine::draw_ui_text(
            fb,
            panel_x + PAD_H + 4,
            cur_y + 2,
            "All Apps",
            12,
            Pixel::new(80, 140, 220, 120),
        );
        cur_y += 18;
    }

    // Scrollable app list
    let list_start_y = cur_y;
    let list_h = content_h as i32 - (cur_y - content_y);
    let max_visible = (list_h / MENU_ITEM_HEIGHT as i32).max(0) as usize;

    for (vi, &real_idx) in menu
        .filtered_indices
        .iter()
        .enumerate()
        .skip(menu.scroll_offset)
        .take(max_visible)
    {
        let item = &menu.items[real_idx];
        let row = (vi - menu.scroll_offset) as i32;
        let item_y = list_start_y + row * MENU_ITEM_HEIGHT as i32;

        let item_rect = Rect::new(
            panel_x + ITEM_INSET,
            item_y,
            panel_w - ITEM_INSET as u32 * 2,
            MENU_ITEM_HEIGHT,
        );

        let is_hovered = menu.hovered_index == Some(vi);
        if is_hovered {
            fb.fill_rounded_rect_aa(item_rect, colors::START_MENU_HOVER, 10);
            fb.fill_rounded_rect_aa(
                Rect::new(panel_x + ITEM_INSET, item_y + 8, 3, MENU_ITEM_HEIGHT - 16),
                colors::with_alpha(colors::ACCENT_BRIGHT, 200),
                1,
            );
        }

        // Icon (24×24)
        let icon_x = panel_x + ITEM_INSET + 14;
        let icon_y = item_y + (MENU_ITEM_HEIGHT as i32 - ITEM_ICON_SIZE) / 2;
        icon_theme::draw_small_icon(fb, icon_x, icon_y, icon_theme::for_desktop(item.icon_type));

        // App name + description
        let text_x = icon_x + ITEM_ICON_SIZE + ICON_TEXT_GAP;
        let name_y = item_y + 7;
        let desc_y = item_y + 22;

        let name_color = if is_hovered {
            Pixel::new(252, 244, 236, 255)
        } else {
            Pixel::new(230, 220, 210, 230)
        };
        font_engine::draw_ui_text(fb, text_x, name_y, &item.name, 12, name_color);

        if !item.description.is_empty() {
            font_engine::draw_ui_text(
                fb,
                text_x,
                desc_y,
                &item.description,
                12,
                Pixel::new(120, 110, 100, 130),
            );
        }

        // Category badge
        let badge_text = match item.category {
            MenuCategory::Pinned => "PIN",
            MenuCategory::Game => "GAME",
            MenuCategory::AI => "AI",
            MenuCategory::Media => "MEDIA",
            MenuCategory::Development => "DEV",
            MenuCategory::System => "SYS",
            _ => "",
        };
        if !badge_text.is_empty() {
            let badge_w = font_engine::measure_ui_text(badge_text, 12) + 8;
            let badge_x = panel_x + panel_w as i32 - ITEM_INSET - badge_w as i32 - 8;
            let badge_y = item_y + (MENU_ITEM_HEIGHT as i32 - 16) / 2;
            let badge_color = match item.category {
                MenuCategory::Pinned => colors::with_alpha(colors::ACCENT_PRIMARY, 45),
                MenuCategory::Game => Pixel::new(230, 150, 60, 45),
                MenuCategory::AI => Pixel::new(180, 140, 200, 45),
                MenuCategory::Media => Pixel::new(130, 180, 150, 42),
                MenuCategory::Development => Pixel::new(200, 190, 100, 40),
                MenuCategory::System => Pixel::new(140, 160, 180, 40),
                _ => Pixel::new(90, 82, 76, 30),
            };
            fb.fill_rounded_rect_aa(Rect::new(badge_x, badge_y, badge_w, 16), badge_color, 6);
            let badge_text_color = match item.category {
                MenuCategory::Pinned => Pixel::new(240, 160, 140, 200),
                MenuCategory::Game => Pixel::new(240, 190, 100, 190),
                MenuCategory::AI => Pixel::new(200, 170, 220, 200),
                MenuCategory::Media => Pixel::new(150, 200, 170, 190),
                MenuCategory::Development => Pixel::new(220, 210, 140, 180),
                MenuCategory::System => Pixel::new(170, 185, 205, 180),
                _ => Pixel::new(140, 130, 120, 150),
            };
            font_engine::draw_ui_text(
                fb,
                badge_x + 4,
                badge_y + 2,
                badge_text,
                12,
                badge_text_color,
            );
        }
    }

    // Scroll indicator
    if menu.filtered_indices.len() > max_visible {
        let track_x = panel_x + panel_w as i32 - 6;
        let track_h = list_h.max(1);
        let thumb_ratio = max_visible as f32 / menu.filtered_indices.len() as f32;
        let thumb_h = ((track_h as f32 * thumb_ratio) as i32).max(20);
        let scroll_ratio = if menu.filtered_indices.len() > max_visible {
            menu.scroll_offset as f32 / (menu.filtered_indices.len() - max_visible) as f32
        } else {
            0.0
        };
        let thumb_y = list_start_y + ((track_h - thumb_h) as f32 * scroll_ratio) as i32;

        fb.fill_rounded_rect_aa(
            Rect::new(track_x, list_start_y, 4, track_h as u32),
            Pixel::new(30, 40, 60, 60),
            2,
        );
        fb.fill_rounded_rect_aa(
            Rect::new(track_x, thumb_y, 4, thumb_h as u32),
            Pixel::new(60, 140, 220, 120),
            2,
        );
    }

    // Empty state
    if menu.filtered_indices.is_empty() {
        let empty_y = content_y + content_h as i32 / 2 - 20;
        let cx = panel_x + panel_w as i32 / 2;
        {
            let _tw = font_engine::measure_ui_text("No apps found", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y,
                "No apps found",
                12,
                Pixel::new(100, 120, 160, 160),
            );
        }
        {
            let _tw = font_engine::measure_ui_text("Try a different search query", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y + 20,
                "Try a different search query",
                12,
                Pixel::new(80, 100, 140, 120),
            );
        }
    }
}
