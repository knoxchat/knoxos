/// Desktop pointer interaction: clicks, icon drag/drop, rubber-band, snap preview
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::notifications;
use crate::gui::startmenu;
use crate::gui::window;

use super::apps::open_application;
use super::types::{
    DESKTOP, ICON_GRID_SPACING_X, ICON_GRID_SPACING_Y, ICON_GRID_X, ICON_GRID_Y, ICON_HEIGHT,
    ICON_WIDTH, IconType, RubberBand,
};

// ═══════════════════════════════════════════════════════════════════════
// SNAP PREVIEW — visual indicator when dragging window near screen edges
// ═══════════════════════════════════════════════════════════════════════

/// Draw snap preview overlay when a window is being dragged near edges
pub(crate) fn draw_snap_preview(fb: &mut FrameBuffer, mx: i32, my: i32) {
    let wm = window::WINDOW_MANAGER.lock();

    // Only show when actively dragging a window
    if !wm.any_dragging() {
        return;
    }

    let w = fb.width as u32;
    let h = fb.height as u32 - crate::gui::scale::taskbar_height();
    let snap_zone = 12; // pixels from edge to trigger preview
    let corner_zone = 48; // corner detection extends further

    // Semi-transparent blue overlay with rounded corner effect
    let preview_fill = Pixel::new(0, 106, 230, 45);
    let preview_border = Pixel::new(0, 106, 230, 140);
    let margin = 6i32;

    let hw = w / 2;
    let hh = h / 2;

    // Corner zones (priority over edge zones)
    if mx <= snap_zone && my <= corner_zone {
        // Top-left quarter snap preview
        let r = crate::gui::framebuffer::Rect::new(
            margin,
            margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx >= fb.width as i32 - snap_zone && my <= corner_zone {
        // Top-right quarter snap preview
        let r = crate::gui::framebuffer::Rect::new(
            hw as i32 + margin,
            margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx <= snap_zone && my >= h as i32 - corner_zone {
        // Bottom-left quarter snap preview
        let r = crate::gui::framebuffer::Rect::new(
            margin,
            hh as i32 + margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx >= fb.width as i32 - snap_zone && my >= h as i32 - corner_zone {
        // Bottom-right quarter snap preview
        let r = crate::gui::framebuffer::Rect::new(
            hw as i32 + margin,
            hh as i32 + margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx <= snap_zone {
        // Left half snap preview
        let r = crate::gui::framebuffer::Rect::new(
            margin,
            margin,
            hw - margin as u32 * 2,
            h - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx >= fb.width as i32 - snap_zone {
        // Right half snap preview
        let r = crate::gui::framebuffer::Rect::new(
            hw as i32 + margin,
            margin,
            hw - margin as u32 * 2,
            h - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if my <= snap_zone {
        // Maximize preview
        let r = crate::gui::framebuffer::Rect::new(
            margin,
            margin,
            w - margin as u32 * 2,
            h - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    }
}

/// Handle desktop click
pub fn handle_click(x: i32, y: i32) {
    // Close start menu if open and clicking outside
    if startmenu::is_visible() {
        if let Some(item) = startmenu::handle_click(x, y) {
            crate::serial_println!("[KnoxOS] Start menu: Opening {}", item.name);
            open_application(&item.name, item.icon_type);
            crate::gui::request_redraw();
            return;
        }
        startmenu::close();
        crate::gui::request_redraw();
        return;
    }

    let mut desktop = DESKTOP.lock();

    // Check if clicking on an icon
    let mut clicked_icon: Option<(usize, bool)> = None; // (index, was_already_selected)
    for (i, icon) in desktop.icons.iter().enumerate() {
        let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
        if icon_rect.contains(x, y) {
            clicked_icon = Some((i, icon.selected));
            break;
        }
    }

    match clicked_icon {
        Some((idx, _was_selected)) => {
            // Select this icon
            for (i, icon) in desktop.icons.iter_mut().enumerate() {
                icon.selected = i == idx;
            }
            desktop.selected_icon = Some(idx);

            // Start drag tracking — the actual drag movement starts when
            // handle_icon_drag() detects the mouse has moved enough
            desktop.dragging_icon = Some(idx);
            desktop.drag_offset_x = x - desktop.icons[idx].x;
            desktop.drag_offset_y = y - desktop.icons[idx].y;
            desktop.drag_x = desktop.icons[idx].x;
            desktop.drag_y = desktop.icons[idx].y;
        }
        None => {
            // Clicked on empty desktop — deselect all and start rubber band
            for icon in desktop.icons.iter_mut() {
                icon.selected = false;
            }
            desktop.selected_icon = None;
            desktop.rubber_band = Some(RubberBand {
                start_x: x,
                start_y: y,
                end_x: x,
                end_y: y,
            });
        }
    }

    drop(desktop);
    crate::gui::request_redraw();
}

/// Handle desktop double-click - open the clicked icon
pub fn handle_double_click(x: i32, y: i32) {
    // Cancel any drag in progress
    {
        let mut desktop = DESKTOP.lock();
        desktop.dragging_icon = None;
    }

    let desktop = DESKTOP.lock();

    for icon in &desktop.icons {
        let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
        if icon_rect.contains(x, y) {
            crate::serial_println!("[KnoxOS] Opening: {}", icon.name);
            let name = icon.name.clone();
            let icon_type = icon.icon_type;
            drop(desktop);
            open_application(&name, icon_type);
            crate::gui::request_redraw();
            return;
        }
    }
}

/// Check if a desktop icon is currently being dragged
pub fn is_dragging_icon() -> bool {
    DESKTOP.lock().dragging_icon.is_some()
}

/// Handle icon drag movement. Called from input.rs handle_drag() when
/// a desktop icon drag is active. Updates the drag position.
pub fn handle_icon_drag(mx: i32, my: i32) {
    let mut desktop = DESKTOP.lock();
    if let Some(_idx) = desktop.dragging_icon {
        let old_x = desktop.drag_x;
        let old_y = desktop.drag_y;
        desktop.drag_x = mx - desktop.drag_offset_x;
        desktop.drag_y = my - desktop.drag_offset_y;

        // Push damage for old and new positions
        let old_rect = Rect::new(old_x - 2, old_y - 2, ICON_WIDTH + 4, ICON_HEIGHT + 4);
        let new_rect = Rect::new(
            desktop.drag_x - 2,
            desktop.drag_y - 2,
            ICON_WIDTH + 4,
            ICON_HEIGHT + 4,
        );
        drop(desktop);
        crate::gui::push_damage(old_rect);
        crate::gui::push_damage(new_rect);
    }
}

/// Handle icon drop — snap to nearest grid cell, or perform file operation
/// if a Document/file icon is dropped onto a Folder icon.
/// Called from input.rs handle_mouse_up().
pub fn handle_icon_drop(mx: i32, my: i32) {
    let mut desktop = DESKTOP.lock();
    if let Some(idx) = desktop.dragging_icon {
        // Final drop position
        let drop_x = mx - desktop.drag_offset_x;
        let drop_y = my - desktop.drag_offset_y;

        // Snap to nearest grid cell
        let col = ((drop_x - ICON_GRID_X + ICON_GRID_SPACING_X / 2) / ICON_GRID_SPACING_X).max(0);
        let row = ((drop_y - ICON_GRID_Y + ICON_GRID_SPACING_Y / 2) / ICON_GRID_SPACING_Y).max(0);

        let snapped_x = ICON_GRID_X + col * ICON_GRID_SPACING_X;
        let snapped_y = ICON_GRID_Y + row * ICON_GRID_SPACING_Y;

        // ── File drop onto folder icon ───────────────────────
        // Check if we dropped a file/document icon onto a folder icon
        let dragged_type = desktop.icons[idx].icon_type;
        let dragged_name = desktop.icons[idx].name.clone();
        let target_icon = desktop
            .icons
            .iter()
            .enumerate()
            .find(|(i, ic)| *i != idx && ic.x == snapped_x && ic.y == snapped_y);

        if let Some((target_idx, _)) = target_icon {
            let target_type = desktop.icons[target_idx].icon_type;
            let target_name = desktop.icons[target_idx].name.clone();

            // If dragging a Document onto a Folder, move the file into the folder
            if dragged_type == IconType::Document && target_type == IconType::Folder {
                let desktop_dir = "/home/user/Desktop";
                let src_path = alloc::format!("{}/{}", desktop_dir, dragged_name);
                // Map folder name to its path
                let dst_dir = match target_name.as_str() {
                    "Documents" => "/home/user/Documents",
                    "Downloads" => "/home/user/Downloads",
                    "Music" => "/home/user/Music",
                    "Pictures" => "/home/user/Pictures",
                    "Videos" => "/home/user/Videos",
                    _ => {
                        // Try as a subfolder on Desktop
                        let fallback = alloc::format!("{}/{}", desktop_dir, target_name);
                        // leak to get 'static str — acceptable in kernel
                        // Use the stack buffer approach instead
                        desktop.dragging_icon = None;
                        drop(desktop);
                        match crate::vfs::move_file_dispatch(&src_path, &fallback) {
                            Ok(()) => {
                                crate::gui::notifications::info(
                                    "Desktop",
                                    &alloc::format!("Moved {} → {}", dragged_name, target_name),
                                );
                                // Remove the dragged icon from the desktop
                                let mut d2 = DESKTOP.lock();
                                d2.icons.remove(idx);
                                drop(d2);
                            }
                            Err(_) => {
                                crate::gui::notifications::error(
                                    "Desktop",
                                    &alloc::format!("Failed to move {}", dragged_name),
                                );
                            }
                        }
                        crate::gui::request_redraw();
                        return;
                    }
                };

                desktop.dragging_icon = None;
                drop(desktop);

                match crate::vfs::move_file_dispatch(&src_path, dst_dir) {
                    Ok(()) => {
                        crate::gui::notifications::info(
                            "Desktop",
                            &alloc::format!("Moved {} → {}", dragged_name, target_name),
                        );
                        // Remove the dragged icon from the desktop
                        let mut d2 = DESKTOP.lock();
                        d2.icons.remove(idx);
                        drop(d2);
                    }
                    Err(_) => {
                        crate::gui::notifications::error(
                            "Desktop",
                            &alloc::format!("Failed to move {}", dragged_name),
                        );
                    }
                }
                crate::gui::request_redraw();
                return;
            }
        }

        // ── Normal icon repositioning (no file operation) ────
        // Check if another icon already occupies this grid cell
        let occupied = desktop
            .icons
            .iter()
            .enumerate()
            .any(|(i, ic)| i != idx && ic.x == snapped_x && ic.y == snapped_y);

        if occupied {
            // If target cell is occupied, swap positions with that icon
            if let Some(other_idx) = desktop
                .icons
                .iter()
                .position(|ic| ic.x == snapped_x && ic.y == snapped_y)
            {
                let orig_x = desktop.icons[idx].x;
                let orig_y = desktop.icons[idx].y;
                desktop.icons[other_idx].x = orig_x;
                desktop.icons[other_idx].y = orig_y;
            }
        }

        desktop.icons[idx].x = snapped_x;
        desktop.icons[idx].y = snapped_y;
        desktop.dragging_icon = None;

        crate::serial_println!("[KnoxOS] Icon dropped at grid ({}, {})", col, row,);
    } else {
        desktop.dragging_icon = None;
    }
    drop(desktop);
    crate::gui::request_redraw();
}

/// Cancel icon drag without moving
pub fn cancel_icon_drag() {
    let mut desktop = DESKTOP.lock();
    desktop.dragging_icon = None;
}

/// Check if rubber band selection is active
pub fn is_rubber_band_active() -> bool {
    DESKTOP.lock().rubber_band.is_some()
}

/// Update rubber band selection rectangle as mouse moves.
/// Selects all icons that intersect the rubber band.
pub fn handle_rubber_band_drag(mx: i32, my: i32) {
    let mut desktop = DESKTOP.lock();
    if let Some(ref mut rb) = desktop.rubber_band {
        let old_rect = rb.to_rect();

        rb.end_x = mx;
        rb.end_y = my;

        let new_rect = rb.to_rect();

        // Select icons that intersect the rubber band rectangle
        let band_rect = new_rect;
        for icon in desktop.icons.iter_mut() {
            let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
            icon.selected = icon_rect.intersects(&band_rect);
        }

        // Push damage for old and new rubber band areas
        drop(desktop);
        crate::gui::push_damage(Rect::new(
            old_rect.x - 2,
            old_rect.y - 2,
            old_rect.width + 4,
            old_rect.height + 4,
        ));
        crate::gui::push_damage(Rect::new(
            new_rect.x - 2,
            new_rect.y - 2,
            new_rect.width + 4,
            new_rect.height + 4,
        ));
    }
}

/// Finish rubber band selection (mouse released).
pub fn finish_rubber_band() {
    let mut desktop = DESKTOP.lock();
    if let Some(rb) = desktop.rubber_band.take() {
        // Final selection — icons that intersect
        let band_rect = rb.to_rect();
        let mut first_selected = None;
        for (i, icon) in desktop.icons.iter_mut().enumerate() {
            let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
            icon.selected = icon_rect.intersects(&band_rect);
            if icon.selected && first_selected.is_none() {
                first_selected = Some(i);
            }
        }
        desktop.selected_icon = first_selected;

        let count = desktop.icons.iter().filter(|i| i.selected).count();
        crate::serial_println!("[KnoxOS] Rubber band selection: {} icons selected", count);
    }
    drop(desktop);
    crate::gui::request_redraw();
}
