/// Desktop icon drawing and IconType → theme mapping
use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::icon_theme;

use super::types::{
    DESKTOP, ICON_HEIGHT, ICON_SIZE_ACTUAL, ICON_WIDTH, IconType, TEXT_LABEL_PADDING_LEFT,
};

/// Draw all desktop icons
pub(crate) fn draw_desktop_icons(fb: &mut FrameBuffer) {
    let desktop = DESKTOP.lock();
    // D9.7: static icons are a GPUI layer. D9.9: the floating drag preview
    // stays compositor-owned (no extra full-screen GPUI pass).
    let gpui = crate::gui::gpui_platform::desktop_is_live();

    for (i, icon) in desktop.icons.iter().enumerate() {
        // If this icon is being dragged, draw a dim ghost at original position
        // and draw the actual icon at the drag position
        let is_dragging = desktop.dragging_icon == Some(i);

        if gpui && !is_dragging {
            continue;
        }

        if is_dragging {
            if gpui {
                // Ghost is painted by the GPUI icon layer at the original cell.
                continue;
            }
            // Ghost at original position (30% opacity via dimmer color)
            let ghost_alpha = Pixel::new(100, 90, 80, 40);
            fb.fill_rounded_rect_aa(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                ghost_alpha,
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                colors::with_alpha(colors::ACCENT_BRIGHT, 50),
                6,
                1,
            );
            continue; // Draw the dragging icon below
        }

        // Selection/focus background with rounded corners
        if icon.selected {
            fb.fill_rounded_rect_aa(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                colors::ICON_BG_FOCUSED,
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                colors::ICON_BORDER_FOCUSED,
                6,
                1,
            );
        }

        let iy = icon.y + (ICON_HEIGHT as i32 - ICON_SIZE_ACTUAL) / 2;
        let ix = icon.x + TEXT_LABEL_PADDING_LEFT;
        draw_icon_graphic(fb, ix, iy, icon.icon_type);
        draw_icon_label_fe(fb, icon.x, icon.y, &icon.name);
    }

    // Draw the dragging icon on top of everything (at drag position)
    if let Some(drag_idx) = desktop.dragging_icon {
        if let Some(icon) = desktop.icons.get(drag_idx) {
            let dx = desktop.drag_x;
            let dy = desktop.drag_y;

            // Floating card with glow
            fb.fill_rounded_rect_aa(
                Rect::new(dx, dy, ICON_WIDTH, ICON_HEIGHT),
                colors::with_alpha(colors::ACCENT_PRIMARY, 45),
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(dx, dy, ICON_WIDTH, ICON_HEIGHT),
                colors::with_alpha(colors::ACCENT_BRIGHT, 100),
                6,
                1,
            );

            let iy = dy + (ICON_HEIGHT as i32 - ICON_SIZE_ACTUAL) / 2;
            let ix = dx + TEXT_LABEL_PADDING_LEFT;
            draw_icon_graphic(fb, ix, iy, icon.icon_type);
            draw_icon_label_fe(fb, dx, dy, &icon.name);
        }
    }
}

/// Helper: draw the icon graphic by type using Lucide (desktop lockup size).
fn draw_icon_graphic(fb: &mut FrameBuffer, ix: i32, iy: i32, icon_type: IconType) {
    icon_theme::blit(
        fb,
        ix,
        iy,
        icon_theme::for_desktop(icon_type),
        ICON_SIZE_ACTUAL,
    );
}

/// Label to the right of the Lucide glyph (same lockup as the GPUI layer).
fn draw_icon_label_fe(fb: &mut FrameBuffer, icon_x: i32, icon_y: i32, text: &str) {
    const LABEL_SIZE: u16 = knox_ui::TypeTokens::DESKTOP;
    let cx = icon_x + TEXT_LABEL_PADDING_LEFT + ICON_SIZE_ACTUAL + 8;
    let cy = icon_y + (ICON_HEIGHT as i32 - LABEL_SIZE as i32) / 2;
    font_engine::draw_ui_text(
        fb,
        cx + 1,
        cy + 1,
        text,
        LABEL_SIZE,
        colors::ICON_TEXT_SHADOW,
    );
    font_engine::draw_ui_text(fb, cx, cy, text, LABEL_SIZE, colors::ICON_TEXT);
}

/// Draw the rubber band selection rectangle overlay
pub(crate) fn draw_rubber_band(fb: &mut FrameBuffer) {
    let desktop = DESKTOP.lock();
    if let Some(rb) = desktop.rubber_band {
        let rect = rb.to_rect();
        // Semi-transparent fill
        fb.fill_rounded_rect_aa(rect, colors::ICON_BG_SELECTED, 2);
        fb.draw_rounded_rect(rect, colors::with_alpha(colors::ACCENT_BRIGHT, 120), 2, 1);
    }
}
