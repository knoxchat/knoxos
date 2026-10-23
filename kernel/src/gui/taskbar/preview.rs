/// Window preview tooltip shown when hovering a running app entry.
use alloc::string::String;
use spin::Mutex;

use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::window;
use crate::gui::window::ViewKind;

use super::layout::compute_dock_layout;
use super::state::{DOCK_ITEM_SIZE, MAX_DOCK_ENTRIES, TASKBAR};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Window preview tooltip — shown when hovering a running app entry
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// State for the window preview tooltip above taskbar
struct WindowPreview {
    visible: bool,
    window_id: window::WindowId,
    title: String,
    content_type: window::WindowContentType,
    /// X center of the hovered dock item (for tooltip placement)
    item_center_x: i32,
    /// Y of the dock top edge
    dock_top_y: i32,
}

lazy_static::lazy_static! {
    static ref WINDOW_PREVIEW: Mutex<WindowPreview> = Mutex::new(WindowPreview {
        visible: false,
        window_id: 0,
        title: String::new(),
        content_type: window::WindowContentType::ShmEmpty,
        item_center_x: 0,
        dock_top_y: 0,
    });
}

/// Update the window preview based on current hover state.
/// Called after update_hover().
pub fn update_window_preview(screen_width: u32, screen_height: u32) {
    let taskbar = TASKBAR.lock();
    let mut preview = WINDOW_PREVIEW.lock();

    if let Some(idx) = taskbar.hovered_entry {
        if let Some(entry) = taskbar.entries.get(idx) {
            if !entry.active {
                if preview.visible {
                    preview.visible = false;
                    crate::gui::request_redraw();
                }
                return;
            }
            let changed = !preview.visible || preview.window_id != entry.window_id;
            preview.visible = true;
            preview.window_id = entry.window_id;
            preview.title.clear();
            preview.title.push_str(&entry.title);
            preview.content_type = entry.content_type;

            // Compute dock item center x (same layout math as update_hover)
            let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);
            let layout = compute_dock_layout(entry_count, screen_width, screen_height);
            let items_start_x = layout.content_x
                + layout.start_btn_w
                + layout.workspace_area_w
                + layout.entry_spacing;
            preview.item_center_x =
                items_start_x + idx as i32 * DOCK_ITEM_SIZE as i32 + DOCK_ITEM_SIZE as i32 / 2;
            preview.dock_top_y = layout.dock_y;

            if changed {
                crate::gui::request_redraw();
            }
            return;
        }
    }
    if preview.visible {
        preview.visible = false;
        crate::gui::request_redraw();
    }
}

/// Check if the window preview tooltip is visible
pub fn is_preview_visible() -> bool {
    WINDOW_PREVIEW.lock().visible
}

/// Draw the window preview tooltip above the hovered dock item
pub fn draw_window_preview(fb: &mut FrameBuffer) {
    let preview = WINDOW_PREVIEW.lock();
    if !preview.visible {
        return;
    }

    let preview_w = 180i32;
    let preview_h = 120i32;
    let arrow_h = 6i32;
    let gap = 6i32;

    // Position preview centered above the dock item
    let px = (preview.item_center_x - preview_w / 2)
        .max(8)
        .min(fb.width as i32 - preview_w - 8);
    let py = preview.dock_top_y - preview_h - arrow_h - gap;

    let rect = Rect::new(px, py, preview_w as u32, preview_h as u32);

    // Background: frosted glass panel
    fb.fill_rounded_rect_aa(rect, Pixel::new(20, 25, 38, 230), 10);
    fb.draw_rounded_rect(rect, Pixel::new(80, 140, 240, 60), 10, 1);

    // Arrow pointing down toward dock item
    let arrow_cx = preview.item_center_x.max(px + 10).min(px + preview_w - 10);
    for i in 0..arrow_h {
        let half_w = (arrow_h - i).max(1);
        fb.fill_rect(
            Rect::new(
                arrow_cx - half_w,
                py + preview_h + i,
                (half_w * 2) as u32,
                1,
            ),
            Pixel::new(20, 25, 38, 230),
        );
    }

    // Window content preview area
    let content_margin = 8i32;
    let content_w = preview_w - content_margin * 2;
    let content_h = preview_h - 30 - content_margin;
    let content_rect = Rect::new(
        px + content_margin,
        py + content_margin,
        content_w as u32,
        content_h as u32,
    );

    let bg = match preview.content_type {
        window::WindowContentType::TerminalSurface => Pixel::new(25, 28, 38, 255),
        window::WindowContentType::Gpui(ViewKind::Browser) => Pixel::new(40, 45, 58, 255),
        window::WindowContentType::Gpui(ViewKind::Files) => Pixel::new(30, 38, 48, 255),
        window::WindowContentType::Gpui(ViewKind::Settings) => Pixel::new(22, 28, 38, 255),
        window::WindowContentType::Gpui(ViewKind::Editor) => Pixel::new(28, 28, 30, 255),
        _ => Pixel::new(35, 40, 52, 255),
    };
    fb.fill_rounded_rect_aa(content_rect, bg, 6);

    // Mini title bar in preview
    fb.fill_rect(
        Rect::new(
            px + content_margin,
            py + content_margin,
            content_w as u32,
            12,
        ),
        Pixel::new(40, 45, 58, 200),
    );

    // Window type icon centered in content
    let icon_cx = px + content_margin + content_w / 2;
    let icon_cy = py + content_margin + 12 + (content_h - 12) / 2;
    crate::gui::alt_tab::draw_content_type_icon(fb, icon_cx, icon_cy, preview.content_type);

    // Title text at the bottom of the preview
    let title_y = py + preview_h - 20;
    let max_chars = (content_w / 7) as usize;
    let display_title = if preview.title.len() > max_chars {
        let mut t = String::from(&preview.title[..max_chars.saturating_sub(2)]);
        t.push_str("..");
        t
    } else {
        preview.title.clone()
    };
    let text_w = font_engine::measure_ui_text(&display_title, 11) as i32;
    let text_x = px + (preview_w - text_w) / 2;
    font_engine::draw_ui_text(
        fb,
        text_x,
        title_y,
        &display_title,
        11,
        Pixel::new(200, 210, 230, 230),
    );
}
