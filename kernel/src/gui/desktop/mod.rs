/// Desktop - Main desktop environment rendering
/// Draws the Aurora wallpaper, desktop icons, and coordinates all GUI elements
mod apps;
mod context_menu;
mod cursor;
mod files;
mod folder;
mod icons;
mod interaction;
mod types;
mod wallpaper;
mod zoom;

pub use apps::open_application;
pub use context_menu::{close_context_menu, handle_context_menu_click, show_context_menu};
pub use cursor::{
    CURSOR_H, CURSOR_SAVE_H, CURSOR_SAVE_PAD, CURSOR_SAVE_W, CURSOR_W, CursorTheme, CursorType,
    cursor_colors, cursor_theme, draw_cursor, get_cursor_type, invalidate_cursor_bg,
    is_over_desktop_icon, last_cursor_pos, restore_cursor_background, save_cursor_background,
    set_cursor_theme, set_cursor_type, set_last_cursor_pos, update_cursor_for_position,
};
pub use files::accept_file_drop;
pub use folder::{
    empty_trash, move_to_trash, restore_from_trash, sync_desktop_folder, trash_count,
};
pub use interaction::{
    cancel_icon_drag, finish_rubber_band, handle_click, handle_double_click, handle_icon_drag,
    handle_icon_drop, handle_rubber_band_drag, is_dragging_icon, is_rubber_band_active,
};
pub use types::{
    CONTEXT_MENU, ContextAction, ContextMenuItem, DESKTOP, Desktop, DesktopContextMenu,
    DesktopIcon, IconType, RubberBand,
};
pub use wallpaper::invalidate_wallpaper_cache;

pub(crate) use icons::icon_type_theme;

use super::FRAMEBUFFER;
use super::framebuffer::Rect;
use super::startmenu;
use super::taskbar;
use super::window;

use context_menu::{CONTEXT_MENU_WIDTH, draw_context_menu};
use icons::{draw_desktop_icons, draw_rubber_band};
use interaction::draw_snap_preview;
use wallpaper::{WALLPAPER_CACHE, draw_wallpaper};
use zoom::apply_zoom_lens;

/// Draw the complete desktop (called at startup and on redraws).
///
/// Accepts a list of damage rects describing which regions changed.
/// Only pixels inside the union of those rects are recomposed and presented
/// to the HW framebuffer. This is the key optimization that keeps the cursor
/// responsive even while windows are being dragged or resized:
///
///  - Mouse-only movement uses the fast `update_cursor_only()` path (no compositing)
///  - Window drag pushes two small damage rects (old position + new position)
///  - Click/scroll pushes damage for just the affected widget/area
///  - Full redraws (resolution change, first paint) push a screen-sized rect
pub fn draw_desktop_damaged(damage: &[Rect]) {
    // Read mouse position BEFORE locking FRAMEBUFFER to avoid deadlock.
    let (mx, my) = {
        let mouse = super::input::MOUSE.lock();
        (mouse.x, mouse.y)
    };

    if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
        let screen_w = fb.width as u32;
        let screen_h = fb.height as u32;
        let screen_rect = Rect::new(0, 0, screen_w, screen_h);

        // ── 0. Restore old cursor pixels FIRST ──────────────────────
        // The cursor was drawn into the back buffer on the previous frame.
        // We MUST erase it before compositing, otherwise the old cursor
        // pixels get baked into the wallpaper cache reads and leave ghosts.
        let (old_cx, old_cy) = last_cursor_pos();
        let old_cursor_rect = Rect::new(
            old_cx - CURSOR_SAVE_PAD,
            old_cy - CURSOR_SAVE_PAD,
            CURSOR_SAVE_W as u32,
            CURSOR_SAVE_H as u32,
        );
        restore_cursor_background(fb);

        // New cursor rect for the position we'll draw at this frame
        let new_cursor_rect = Rect::new(
            mx - CURSOR_SAVE_PAD,
            my - CURSOR_SAVE_PAD,
            CURSOR_SAVE_W as u32,
            CURSOR_SAVE_H as u32,
        );

        // Compute the union bounding box of all damage rects, clamped to screen.
        // Include both old and new cursor positions so the HW framebuffer
        // is updated everywhere the cursor was or will be.
        let base_damage = if damage.is_empty() {
            screen_rect
        } else {
            let mut u = damage[0];
            for r in &damage[1..] {
                u = super::rect_union(u, *r);
            }
            u
        };
        let damage_union = clamp_rect_to_screen(
            super::rect_union(
                super::rect_union(base_damage, old_cursor_rect),
                new_cursor_rect,
            ),
            screen_w,
            screen_h,
        );

        // Check if this is a full-screen redraw (damage covers ≥90% of screen)
        let damage_area = damage_union.width as u64 * damage_union.height as u64;
        let screen_area = screen_w as u64 * screen_h as u64;
        let is_full = damage_area * 100 / screen_area.max(1) >= 90;

        // Update taskbar/dock hover state and auto-hide animation
        taskbar::update_hover(mx, my, screen_w, screen_h);
        taskbar::update_auto_hide(my, screen_h);
        if startmenu::is_visible() {
            startmenu::update_hover(mx, my);
        }

        // 1. Restore wallpaper in damaged region only
        {
            let mut cache = WALLPAPER_CACHE.lock();
            if cache.is_none() {
                // First time: render full wallpaper and cache it.
                // The cursor has already been erased above, so this is clean.
                draw_wallpaper(fb);
                *cache = Some(fb.buffer.clone());
            } else if let Some(ref cached) = *cache {
                if is_full {
                    // Full screen — single memcpy (fastest)
                    let len = fb.buffer.len().min(cached.len());
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            cached.as_ptr(),
                            fb.buffer.as_mut_ptr(),
                            len,
                        );
                    }
                } else {
                    // Partial — only copy rows within the damage union
                    let bpp = fb.bytes_per_pixel;
                    let pitch = fb.pitch;
                    let x0 = damage_union.x.max(0) as usize;
                    let y0 = damage_union.y.max(0) as usize;
                    let x1 = ((damage_union.x + damage_union.width as i32) as usize).min(fb.width);
                    let y1 =
                        ((damage_union.y + damage_union.height as i32) as usize).min(fb.height);
                    let row_bytes = (x1 - x0) * bpp;
                    for row in y0..y1 {
                        let off = row * pitch + x0 * bpp;
                        if off + row_bytes <= cached.len() && off + row_bytes <= fb.buffer.len() {
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    cached.as_ptr().add(off),
                                    fb.buffer.as_mut_ptr().add(off),
                                    row_bytes,
                                );
                            }
                        }
                    }
                }
            }
        }

        // 2-8: Compose all layers (icons, taskbar, windows, overlays)
        //
        // For partial redraws, push a clip so drawing outside the damage
        // region is skipped. Additionally, skip entire layers that don't
        // overlap the damage region at all — avoids function call overhead,
        // lock acquisitions, and loop iterations for unaffected layers.
        if !is_full {
            fb.push_clip(damage_union);
        }

        // Desktop widgets — rendered on wallpaper surface before icons/windows
        {
            let widget_area = super::desktop_widgets::bounding_rect(screen_w);
            if is_full || damage_union.intersects(&widget_area) {
                super::desktop_widgets::draw(fb);
            }
        }

        // Desktop icons — only if damage overlaps the icon area (top-left region)
        {
            let icon_area = Rect::new(0, 0, 200, screen_h);
            if is_full || damage_union.intersects(&icon_area) {
                draw_desktop_icons(fb);
            }
        }

        // Rubber band selection overlay
        draw_rubber_band(fb);

        // Taskbar — only if damage overlaps the taskbar strip at screen bottom
        {
            let taskbar_y = screen_h as i32 - super::scale::taskbar_height() as i32;
            let taskbar_area = Rect::new(0, taskbar_y, screen_w, super::scale::taskbar_height());
            if is_full || damage_union.intersects(&taskbar_area) {
                taskbar::draw_taskbar(fb);
            }
        }

        // Windows — draw_all now does per-window clip intersection checks internally
        window::WINDOW_MANAGER.lock().draw_all(fb);

        // Snap preview — only while dragging
        draw_snap_preview(fb, mx, my);

        // Alt+Tab overlay — drawn above all windows
        if super::alt_tab::is_visible() {
            super::alt_tab::draw(fb);
        }

        // Exposé / Mission Control overlay — drawn above all windows
        if super::expose::is_visible() {
            super::expose::draw(fb);
        }

        // Overlays — only if damage overlaps their approximate regions or if visible
        if is_full || startmenu::is_visible() {
            startmenu::draw_start_menu(fb);
        }
        {
            let cm = CONTEXT_MENU.lock();
            if cm.visible {
                let cm_rect = Rect::new(cm.x, cm.y, CONTEXT_MENU_WIDTH, 200);
                if is_full || damage_union.intersects(&cm_rect) {
                    drop(cm);
                    draw_context_menu(fb);
                } else {
                    drop(cm);
                }
            }
        }
        if is_full || super::popups::any_popup_open() {
            super::popups::draw_calendar(fb);
            super::popups::draw_volume_popup(fb);
            super::popups::draw_quick_settings(fb);
        }
        if is_full || super::system_tray::is_context_menu_open() {
            super::system_tray::draw_context_menu(fb);
        }
        if is_full || super::taskbar::is_context_menu_open() {
            super::taskbar::draw_context_menu(fb);
        }
        if is_full || super::taskbar::is_preview_visible() {
            super::taskbar::draw_window_preview(fb);
        }
        if is_full
            || super::notifications::NOTIFICATIONS
                .lock()
                .has_visible_toasts()
        {
            super::notifications::draw_toasts(fb);
        }
        if is_full || super::notifications::is_panel_open() {
            super::notifications::draw_notification_panel(fb);
        }

        // File picker modal overlay — drawn above everything except cursor
        if is_full || super::file_picker::is_visible() {
            super::file_picker::draw(fb);
        }

        // Keyboard shortcuts overlay
        if is_full || super::shortcuts_overlay::is_visible() {
            super::shortcuts_overlay::draw(fb);
        }

        // Tick system sounds (turns off speaker after tone duration)
        super::sounds::tick();

        if !is_full {
            fb.pop_clip();
        }

        // 9. Cursor — save background (clean, cursor-free), then draw cursor on top.
        update_cursor_for_position(mx, my);
        save_cursor_background(fb, mx, my);
        draw_cursor(fb, mx, my);
        set_last_cursor_pos(mx, my);

        // 9.5 Screen magnification (17.9) — Zoom lens around cursor
        if super::accessibility::is_zoomed() {
            apply_zoom_lens(fb, mx, my);
        }

        // 9.6 Night light — warm color temperature shift (blue light filter)
        if super::night_light::is_active() {
            super::night_light::apply(fb, damage_union);
        }

        // 10. Present the damaged region (which now includes old + new cursor area)
        // to HW framebuffer.
        fb.present_rect(
            damage_union.x,
            damage_union.y,
            damage_union.width,
            damage_union.height,
        );
    }
}

/// Legacy full-screen redraw (calls damage-based path with full screen rect).
pub fn draw_desktop() {
    let damage = super::take_damage();
    if damage.is_empty() {
        // No damage rects queued — full screen
        let (sw, sh) = super::cached_screen_size();
        draw_desktop_damaged(&[Rect::new(0, 0, sw as u32, sh as u32)]);
    } else {
        draw_desktop_damaged(&damage);
    }
}

/// Clamp a rect to screen bounds.
fn clamp_rect_to_screen(r: Rect, sw: u32, sh: u32) -> Rect {
    let x0 = r.x.max(0).min(sw as i32);
    let y0 = r.y.max(0).min(sh as i32);
    let x1 = (r.x + r.width as i32).max(0).min(sw as i32);
    let y1 = (r.y + r.height as i32).max(0).min(sh as i32);
    Rect::new(x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}

/// Timer tick handler - update clock and request redraw
pub fn on_timer_tick(ticks: u64) {
    // Capture old clock text for change detection
    let old_clock = taskbar::get_clock_text();
    taskbar::update_clock(ticks);
    let new_clock = taskbar::get_clock_text();

    // Tick notification animations — check if any are actively animating
    let mut notifs = super::notifications::NOTIFICATIONS.lock();
    let has_active = notifs
        .notifications
        .iter()
        .any(|n| !n.dismissed && (n.anim_progress < 255 || n.sliding_out));
    notifs.tick();
    drop(notifs);

    // Only request a full redraw if something actually changed
    if old_clock != new_clock || has_active {
        super::request_redraw();
    }
}
