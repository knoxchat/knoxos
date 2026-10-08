/// Window / icon drag, resize, snap zones, and mouse-up handling
use crate::gui::desktop;
use crate::gui::window;
use crate::gui::window::WindowContentType;

/// Snap preview zone: 0=none, 1=left, 2=right, 3=maximize
static LAST_SNAP_ZONE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Determine the snap zone at a given position
/// 0=none, 1=left, 2=right, 3=maximize,
/// 4=top-left, 5=top-right, 6=bottom-left, 7=bottom-right
#[inline]
fn snap_zone_at(x: i32, y: i32, screen_w: i32) -> u8 {
    let (_, screen_h) = crate::gui::cached_screen_size();
    let taskbar_h = crate::gui::scale::taskbar_height() as i32;
    let usable_h = screen_h - taskbar_h;
    let edge = 12;
    let corner = 48; // corner detection zone extends further in from edges

    // Corner zones take priority over edge zones
    if x <= edge && y <= corner {
        return 4;
    } // top-left
    if x >= screen_w - edge && y <= corner {
        return 5;
    } // top-right
    if x <= edge && y >= usable_h - corner {
        return 6;
    } // bottom-left
    if x >= screen_w - edge && y >= usable_h - corner {
        return 7;
    } // bottom-right

    // Edge zones
    if x <= edge {
        return 1;
    } // left half
    if x >= screen_w - edge {
        return 2;
    } // right half
    if y <= edge {
        return 3;
    } // maximize (top edge)
    0 // none
}

/// Handle mouse drag — supports both window move and resize, plus snap zones.
/// Uses damage-based partial redraws instead of full screen redraws.
/// Key optimizations:
///  - Union old+new window rects into a single damage rect (instead of two)
///  - Only push snap preview damage when the snap zone actually changes
///  - Skip draw_all for windows entirely outside the damage clip region
pub(super) fn handle_drag(x: i32, y: i32) {
    let (screen_w, screen_h) = crate::gui::cached_screen_size();

    // ── Desktop icon drag ────────────────────────────────────
    if desktop::is_dragging_icon() {
        desktop::handle_icon_drag(x, y);
        return;
    }

    // ── Explorer file drag (9.70) ────────────────────────────
    if crate::gui::explorer::is_file_drag_active() {
        // File drag already started — nothing to do during move, just track
        return;
    }

    // ── Rubber band selection ────────────────────────────────
    if desktop::is_rubber_band_active() {
        desktop::handle_rubber_band_drag(x, y);
        return;
    }

    // ── Check if dragging from an explorer content area (9.70) ────
    // If no window is being dragged/resized and we're over an explorer with a
    // selected entry, start a file drag instead of a window drag.
    {
        let wm = window::WINDOW_MANAGER.lock();
        let no_win_drag = !wm.any_dragging() && !wm.any_resizing();
        if no_win_drag {
            if let Some(win) = wm
                .windows
                .iter()
                .rev()
                .find(|w| w.visible && w.rect.contains(x, y))
            {
                if win.content_type == WindowContentType::FileExplorer
                    && win.explorer_selected >= 0
                    && win.content_rect().contains(x, y)
                {
                    let wid = win.id;
                    drop(wm);
                    crate::gui::explorer::start_file_drag(wid);
                    return;
                }
            }
        }
    }

    let mut wm = window::WINDOW_MANAGER.lock();

    // First check if any window has a scrollbar being dragged
    for win in wm.windows.iter_mut() {
        if win.scrollbar_dragging {
            let content = win.content_rect();
            let track_h = content.height as i32;
            if track_h <= 0 || win.max_scroll_y <= 0 {
                win.scrollbar_dragging = false;
                continue;
            }
            let dy = y - win.scrollbar_drag_start_y;
            let scroll_range = win.max_scroll_y as f32;
            let ratio = dy as f32 / track_h as f32;
            let new_scroll = win.scrollbar_drag_start_scroll + (ratio * scroll_range) as i32;
            win.scroll_y = new_scroll.clamp(0, win.max_scroll_y);
            let rect = win.rect;
            drop(wm);
            push_window_damage(rect);
            return;
        }
    }

    // First check if any window is being resized
    for win in wm.windows.iter_mut() {
        if win.resizing.is_resizing() {
            let id = win.id;
            let edge = win.resizing;
            // Capture old rect before resize
            let old_rect = win.rect;
            drop(wm);
            window::WINDOW_MANAGER
                .lock()
                .apply_resize(id, edge, x, y, screen_h);
            // Get new rect after resize
            let new_rect = {
                let wm2 = window::WINDOW_MANAGER.lock();
                wm2.windows.iter().find(|w| w.id == id).map(|w| w.rect)
            };
            // Push ONE merged damage rect (union of old + new) with shadow padding
            if let Some(nr) = new_rect {
                push_merged_window_damage(old_rect, nr);
            } else {
                push_window_damage(old_rect);
            }
            return;
        }
    }

    // Then check if any window is being dragged
    for win in wm.windows.iter_mut() {
        if win.dragging {
            // Capture old position
            let old_rect = win.rect;

            let new_x = x - win.drag_offset_x;
            let taskbar_top = screen_h - crate::gui::scale::taskbar_height() as i32;
            let new_y = (y - win.drag_offset_y)
                .clamp(0, taskbar_top - window::scaled_title_bar_height() as i32);
            win.rect.x = new_x;
            win.rect.y = new_y;
            // Ensure window stays reachable (at least scaled MIN_VISIBLE_PX on screen)
            win.clamp_to_screen(screen_w, screen_h);

            let new_rect = win.rect;
            drop(wm);

            // Push ONE merged damage rect covering both old and new positions.
            // This results in a single compositing pass instead of two.
            push_merged_window_damage(old_rect, new_rect);

            // Snap preview: only trigger damage when the zone changes
            let new_zone = snap_zone_at(x, y, screen_w);
            let old_zone = LAST_SNAP_ZONE.swap(new_zone, core::sync::atomic::Ordering::Relaxed);
            if new_zone != old_zone {
                // Snap zone changed — need to redraw the overlay areas.
                let taskbar_h = crate::gui::scale::taskbar_height();
                let h = screen_h as u32 - taskbar_h;
                let hw = screen_w as u32 / 2;
                let hh = h / 2;
                // Inline helper: push the rect for a given snap zone
                let push_zone = |z: u8| match z {
                    1 => crate::gui::push_damage(crate::gui::framebuffer::Rect::new(0, 0, hw, h)),
                    2 => crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
                        screen_w / 2,
                        0,
                        hw,
                        h,
                    )),
                    3 => crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
                        0,
                        0,
                        screen_w as u32,
                        h,
                    )),
                    4 => crate::gui::push_damage(crate::gui::framebuffer::Rect::new(0, 0, hw, hh)),
                    5 => crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
                        screen_w / 2,
                        0,
                        hw,
                        hh,
                    )),
                    6 => crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
                        0, hh as i32, hw, hh,
                    )),
                    7 => crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
                        screen_w / 2,
                        hh as i32,
                        hw,
                        hh,
                    )),
                    _ => {}
                };
                push_zone(old_zone);
                push_zone(new_zone);
            }
            return;
        }
    }
}

/// Push a damage rect for a window, with padding for shadows and resize borders.
#[inline]
pub(super) fn push_window_damage(rect: crate::gui::framebuffer::Rect) {
    // Add padding for window shadow (16px) and resize grab area (6px)
    let pad = 20;
    crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
        rect.x - pad,
        rect.y - pad,
        rect.width + pad as u32 * 2,
        rect.height + pad as u32 * 2,
    ));
}

/// Push a single merged damage rect covering two window positions (old + new).
/// More efficient than two separate pushes — results in one compositing pass.
#[inline]
fn push_merged_window_damage(a: crate::gui::framebuffer::Rect, b: crate::gui::framebuffer::Rect) {
    let pad = 20i32;
    let ax0 = a.x - pad;
    let ay0 = a.y - pad;
    let ax1 = a.x + a.width as i32 + pad;
    let ay1 = a.y + a.height as i32 + pad;
    let bx0 = b.x - pad;
    let by0 = b.y - pad;
    let bx1 = b.x + b.width as i32 + pad;
    let by1 = b.y + b.height as i32 + pad;
    let ux0 = ax0.min(bx0);
    let uy0 = ay0.min(by0);
    let ux1 = ax1.max(bx1);
    let uy1 = ay1.max(by1);
    crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
        ux0,
        uy0,
        (ux1 - ux0).max(0) as u32,
        (uy1 - uy0).max(0) as u32,
    ));
}

/// Handle mouse button release — apply snap zones if dragging near edges
pub(super) fn handle_mouse_up(x: i32, y: i32) {
    // Reset snap preview zone tracking
    LAST_SNAP_ZONE.store(0, core::sync::atomic::Ordering::Relaxed);

    // ── Desktop icon drop ────────────────────────────────────
    if desktop::is_dragging_icon() {
        desktop::handle_icon_drop(x, y);
        return;
    }

    // ── Rubber band selection end ────────────────────────────
    if desktop::is_rubber_band_active() {
        desktop::finish_rubber_band();
        return;
    }

    // ── Explorer file drag-and-drop (9.70) ───────────────────
    if crate::gui::explorer::is_file_drag_active() {
        // Check if dropped on another explorer window
        let wm = window::WINDOW_MANAGER.lock();
        let target_wid = wm
            .windows
            .iter()
            .rev()
            .find(|w| {
                w.visible
                    && w.content_type == window::WindowContentType::FileExplorer
                    && w.rect.contains(x, y)
            })
            .map(|w| w.id);
        drop(wm);

        if let Some(wid) = target_wid {
            crate::gui::explorer::accept_file_drop(wid);
        } else {
            // Check if dropped on desktop (outside any window)
            let wm = window::WINDOW_MANAGER.lock();
            let on_window = wm
                .windows
                .iter()
                .any(|w| w.visible && w.rect.contains(x, y));
            drop(wm);
            if !on_window {
                // Drop on desktop — copy file to Desktop and add icon (9.27)
                if let Some(src) = crate::gui::explorer::dragged_file_path() {
                    desktop::accept_file_drop(&src);
                }
            }
            crate::gui::explorer::cancel_file_drag();
        }
        return;
    }

    // Notify the drag-and-drop state machine that the mouse was released.
    // Drop zones can pick up the payload during this frame.
    crate::gui::drag_and_drop::on_mouse_release();

    let (screen_w, screen_h) = crate::gui::cached_screen_size();

    let mut wm = window::WINDOW_MANAGER.lock();
    let mut snap_action: Option<(u32, u8)> = None; // (window_id, snap_zone)

    for win in wm.windows.iter_mut() {
        if win.scrollbar_dragging {
            win.scrollbar_dragging = false;
        }
        if win.dragging {
            win.dragging = false;

            // Use unified snap zone detection
            let zone = snap_zone_at(x, y, screen_w);
            if zone != 0 && win.state == window::WindowState::Normal {
                snap_action = Some((win.id, zone));
            }
        }
        if win.resizing.is_resizing() {
            win.resizing = window::ResizeEdge::None;
        }
    }

    // Apply snap action after releasing mutable borrow
    if let Some((wid, snap_type)) = snap_action {
        match snap_type {
            1 => wm.snap_left(wid, screen_w as u32, screen_h as u32),
            2 => wm.snap_right(wid, screen_w as u32, screen_h as u32),
            3 => wm.toggle_maximize(wid, screen_w as u32, screen_h as u32),
            4 => wm.snap_top_left(wid, screen_w as u32, screen_h as u32),
            5 => wm.snap_top_right(wid, screen_w as u32, screen_h as u32),
            6 => wm.snap_bottom_left(wid, screen_w as u32, screen_h as u32),
            7 => wm.snap_bottom_right(wid, screen_w as u32, screen_h as u32),
            _ => {}
        }
    }
}
