/// Desktop icon drawing and IconType → theme mapping
use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::icon_theme;
use crate::gui::icon_theme::IconCategory;
use crate::gui::icons;

use super::types::{
    DESKTOP, ICON_HEIGHT, ICON_SIZE_ACTUAL, ICON_WIDTH, IconType, TEXT_LABEL_MARGIN_TOP,
};

/// Draw all desktop icons
pub(crate) fn draw_desktop_icons(fb: &mut FrameBuffer) {
    let desktop = DESKTOP.lock();

    for (i, icon) in desktop.icons.iter().enumerate() {
        // If this icon is being dragged, draw a dim ghost at original position
        // and draw the actual icon at the drag position
        let is_dragging = desktop.dragging_icon == Some(i);

        if is_dragging {
            // Ghost at original position (30% opacity via dimmer color)
            let ghost_alpha = Pixel::new(100, 90, 80, 40);
            fb.fill_rounded_rect_aa(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                ghost_alpha,
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                Pixel::new(200, 140, 120, 50),
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

        // Draw the 48×48 icon graphic centered in the cell
        let ix = icon.x + (ICON_WIDTH as i32 - icons::ICON_SIZE as i32) / 2;
        let iy = icon.y + 2;

        draw_icon_graphic(fb, ix, iy, icon.icon_type);

        // Draw icon label with text shadow (font_engine)
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
                Pixel::new(200, 130, 110, 45),
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(dx, dy, ICON_WIDTH, ICON_HEIGHT),
                Pixel::new(232, 160, 140, 100),
                6,
                1,
            );

            // Icon graphic
            let ix = dx + (ICON_WIDTH as i32 - icons::ICON_SIZE as i32) / 2;
            let iy = dy + 2;
            draw_icon_graphic(fb, ix, iy, icon.icon_type);

            // Label (font_engine)
            draw_icon_label_fe(fb, dx, dy, &icon.name);
        }
    }
}

/// Map desktop IconType to icon_theme (category, name) pairs
pub(crate) fn icon_type_theme(icon_type: IconType) -> (IconCategory, &'static str) {
    match icon_type {
        IconType::MyPC => (IconCategory::Devices, "computer"),
        IconType::Folder => (IconCategory::Places, "folder"),
        IconType::Document => (IconCategory::Mimetypes, "text-x-generic"),
        IconType::Globe => (IconCategory::Apps, "web-browser"),
        IconType::Terminal => (IconCategory::Apps, "utilities-x-terminal"),
        IconType::MediaPlayer => (IconCategory::Apps, "multimedia-video-player"),
        IconType::Game => (IconCategory::Categories, "applications-games"),
        IconType::AIBrain => (IconCategory::Apps, "preferences-system"),
        IconType::Settings => (IconCategory::Apps, "org.gnome.Settings"),
        IconType::Trash => (IconCategory::Places, "user-trash"),
        IconType::Image => (IconCategory::Mimetypes, "image-x-generic"),
        IconType::Archive => (IconCategory::Mimetypes, "application-zip"),
        IconType::Script => (IconCategory::Mimetypes, "text-x-script"),
    }
}

/// Helper: draw the icon graphic by type using icon_theme (48×48)
fn draw_icon_graphic(fb: &mut FrameBuffer, ix: i32, iy: i32, icon_type: IconType) {
    let (category, name) = icon_type_theme(icon_type);
    icon_theme::draw_desktop_icon(fb, ix, iy, category, name);
}

/// Draw a centered icon label below the icon using font_engine with shadow
fn draw_icon_label_fe(fb: &mut FrameBuffer, icon_x: i32, icon_y: i32, text: &str) {
    const LABEL_SIZE: u16 = 11;
    let tw = font_engine::measure_ui_text(text, LABEL_SIZE) as i32;
    let cx = icon_x + (ICON_WIDTH as i32 - tw) / 2;
    let cy = icon_y + ICON_SIZE_ACTUAL + TEXT_LABEL_MARGIN_TOP;
    // Shadow
    font_engine::draw_ui_text(
        fb,
        cx + 1,
        cy + 1,
        text,
        LABEL_SIZE,
        colors::ICON_TEXT_SHADOW,
    );
    // Text
    font_engine::draw_ui_text(fb, cx, cy, text, LABEL_SIZE, colors::ICON_TEXT);
}

/// Draw the rubber band selection rectangle overlay
pub(crate) fn draw_rubber_band(fb: &mut FrameBuffer) {
    let desktop = DESKTOP.lock();
    if let Some(rb) = desktop.rubber_band {
        let rect = rb.to_rect();
        // Semi-transparent fill
        fb.fill_rounded_rect_aa(rect, Pixel::new(232, 121, 100, 25), 2);
        // Warm border
        fb.draw_rounded_rect(rect, Pixel::new(232, 140, 120, 120), 2, 1);
    }
}
