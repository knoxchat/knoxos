/// Mouse device state, PS/2 packet queue, and event dispatch
use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use spin::Mutex;

use crate::gui::desktop;
use crate::gui::startmenu;
use crate::gui::window;

use super::click::{
    handle_click, handle_double_click, handle_middle_click_paste, handle_right_click,
};
use super::config::{FocusMode, MOUSE_SETTINGS, focus_mode};
use super::drag::{handle_drag, handle_mouse_up, push_window_damage};
use super::hover::{is_on_window_button, needs_overlay_hover_redraw};
use super::scroll::handle_scroll;

// ─── Mouse State ─────────────────────────────────────────────────────

/// PS/2 mouse packet queue
pub(super) static MOUSE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

/// Mouse state
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub left_button: bool,
    pub right_button: bool,
    pub middle_button: bool,
    /// Scroll wheel delta (positive = up, negative = down)
    pub scroll_delta: i8,
    /// Whether 4-byte IntelliMouse mode is active (has scroll wheel)
    pub intellimouse: bool,
    packet_index: u8,
    packet: [u8; 4],
    last_click_x: i32,
    last_click_y: i32,
    last_click_tick: u64,
}

lazy_static::lazy_static! {
    pub static ref MOUSE: Mutex<MouseState> = Mutex::new(MouseState {
        x: 512,
        y: 384,
        left_button: false,
        right_button: false,
        middle_button: false,
        scroll_delta: 0,
        intellimouse: false,
        packet_index: 0,
        packet: [0; 4],
        last_click_x: 0,
        last_click_y: 0,
        last_click_tick: 0,
    });
}

/// Initialize the mouse queue early (called during boot, before interrupts are enabled)
pub fn init_mouse_queue() {
    MOUSE_QUEUE
        .try_init_once(|| ArrayQueue::new(512))
        .expect("Mouse queue already initialized");
}

/// Add a mouse byte from the interrupt handler
pub fn add_mouse_byte(byte: u8) {
    if let Ok(queue) = MOUSE_QUEUE.try_get() {
        let _ = queue.push(byte);
    }
}

/// Process mouse events (async task)
pub async fn process_mouse_events() {
    // Queue is already initialized by init_mouse_queue()
    if MOUSE_QUEUE.try_get().is_err() {
        init_mouse_queue();
    }

    loop {
        if let Ok(queue) = MOUSE_QUEUE.try_get() {
            // Drain ALL available mouse bytes in a tight loop.
            // This ensures we process every queued byte without yielding
            // between packets, so the final mouse position reflects ALL
            // accumulated deltas from all queued packets. The redraw loop
            // will then do a single cursor update at the final position.
            while let Some(byte) = queue.pop() {
                process_mouse_byte(byte);
            }
        }
        // Yield to other tasks — next poll will drain any newly arrived bytes
        crate::yield_once().await;
    }
}

/// Drain the mouse queue synchronously (called from redraw loop
/// to ensure the latest mouse position is used before drawing).
/// This eliminates one async yield cycle of latency.
pub fn drain_mouse_queue() {
    if let Ok(queue) = MOUSE_QUEUE.try_get() {
        while let Some(byte) = queue.pop() {
            process_mouse_byte(byte);
        }
    }
}

/// Apply non-linear mouse acceleration respecting user settings.
/// Small precise movements (|delta| ≤ 3) pass through at 1× speed.
/// Medium movements get a gentle 1.5× boost for comfortable traversal.
/// Fast flicks (|delta| > 8) get 2× acceleration so the cursor can cross
/// a 1920×1080 screen quickly without needing a huge mouse pad.
///
/// When acceleration is disabled, sensitivity is applied as a flat multiplier.
#[inline]
fn apply_mouse_acceleration(dx: i32, dy: i32) -> (i32, i32) {
    let settings = MOUSE_SETTINGS.lock();
    let sens = settings.sensitivity;
    let accel_on = settings.acceleration_enabled;
    drop(settings);

    if !accel_on {
        // Flat sensitivity multiplier, no acceleration curve
        return ((dx as f32 * sens) as i32, (dy as f32 * sens) as i32);
    }

    #[inline]
    fn accel(d: i32, s: f32) -> i32 {
        let abs = d.unsigned_abs();
        let base = if abs <= 2 {
            // Precision zone: 1:1
            d
        } else if abs <= 6 {
            // Comfort zone: 1.5×
            let sign = d.signum();
            sign * ((abs as i32 * 3 + 1) / 2)
        } else {
            // Speed zone: 2×
            d * 2
        };
        (base as f32 * s) as i32
    }
    (accel(dx, sens), accel(dy, sens))
}

fn process_mouse_byte(byte: u8) {
    let mut mouse = MOUSE.lock();

    // Use cached screen dimensions — never lock FRAMEBUFFER here!
    let (screen_w, screen_h) = crate::gui::cached_screen_size();

    let idx = mouse.packet_index as usize;
    let packet_size: u8 = if mouse.intellimouse { 4 } else { 3 };

    // PS/2 packet byte 0 must have bit 3 set (always-1 bit in PS/2 protocol)
    if idx == 0 && (byte & 0x08) == 0 {
        return;
    }

    mouse.packet[idx] = byte;
    mouse.packet_index += 1;

    if mouse.packet_index >= packet_size {
        mouse.packet_index = 0;

        let flags = mouse.packet[0];

        // Validate packet: bit 3 must be set
        if flags & 0x08 == 0 {
            return;
        }

        // Discard packets with X or Y overflow (bits 6,7)
        if flags & 0xC0 != 0 {
            return;
        }

        let raw_dx = mouse.packet[1] as i32 - ((flags as i32 & 0x10) << 4);
        let raw_dy = -(mouse.packet[2] as i32 - ((flags as i32 & 0x20) << 3));

        // ── Mouse acceleration ──────────────────────────────────────
        // Apply non-linear acceleration for a snappy, responsive feel.
        // Small movements (precision) are 1:1, larger flicks are amplified.
        // This is similar to macOS/Windows pointer acceleration curves.
        let (dx, dy) = apply_mouse_acceleration(raw_dx, raw_dy);

        // Extract scroll wheel delta from byte 4 (IntelliMouse)
        let scroll_z: i8 = if mouse.intellimouse {
            mouse.packet[3] as i8 // Signed: negative = down, positive = up
        } else {
            0
        };

        // Update position using PS/2 relative deltas
        mouse.x = (mouse.x + dx).clamp(0, screen_w - 1);
        mouse.y = (mouse.y + dy).clamp(0, screen_h - 1);

        // Sync with absolute tablet driver
        if crate::virtio_tablet::is_active() {
            crate::virtio_tablet::set_position(mouse.x, mouse.y);
        }

        let prev_left = mouse.left_button;
        let prev_right = mouse.right_button;
        let prev_middle = mouse.middle_button;

        // Apply left-handed button swap if enabled
        let settings = MOUSE_SETTINGS.lock();
        let left_handed = settings.left_handed;
        let middle_paste = settings.middle_click_paste;
        drop(settings);

        if left_handed {
            mouse.left_button = flags & 0x02 != 0; // swap
            mouse.right_button = flags & 0x01 != 0; // swap
        } else {
            mouse.left_button = flags & 0x01 != 0;
            mouse.right_button = flags & 0x02 != 0;
        }
        mouse.middle_button = flags & 0x04 != 0;

        let x = mouse.x;
        let y = mouse.y;
        let moved = dx != 0 || dy != 0;

        // Handle scroll wheel events (respects natural scrolling setting)
        if scroll_z != 0 {
            let settings = MOUSE_SETTINGS.lock();
            let effective_scroll = if settings.natural_scrolling {
                -scroll_z
            } else {
                scroll_z
            };
            drop(settings);
            mouse.scroll_delta = effective_scroll;
            drop(mouse);
            handle_scroll(x, y, effective_scroll, screen_w as u32, screen_h as u32);
            // Scroll only affects one window — push damage for it
            let wm = window::WINDOW_MANAGER.lock();
            if let Some(wid) = wm.window_at(x, y) {
                if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
                    push_window_damage(win.rect);
                } else {
                    drop(wm);
                    crate::gui::request_redraw();
                }
            } else {
                drop(wm);
                crate::gui::request_redraw();
            }
            return;
        }

        // Handle left click events
        if mouse.left_button && !prev_left {
            crate::serial_println!("[MOUSE] LEFT CLICK at ({}, {})", x, y);
            // Mouse down - check for double click (~400ms window).
            // APIC timer runs at 100Hz → 40 ticks ≈ 400ms.
            // Before APIC timer init, PIT runs at 18.2Hz → 40 ticks ≈ 2.2s (fine, no GUI yet).
            let ticks = crate::interrupts::get_ticks();
            let is_double_click = (ticks - mouse.last_click_tick) < 40
                && (x - mouse.last_click_x).abs() < 8
                && (y - mouse.last_click_y).abs() < 8;

            mouse.last_click_x = x;
            mouse.last_click_y = y;
            mouse.last_click_tick = ticks;

            drop(mouse);

            // Close context menus on any left click
            desktop::close_context_menu();
            crate::gui::system_tray::close_context_menu();

            if is_double_click {
                let on_button = is_on_window_button(x, y);
                if on_button {
                    handle_click(x, y, screen_w as u32, screen_h as u32);
                } else {
                    handle_double_click(x, y, screen_w as u32, screen_h as u32);
                }
            } else {
                handle_click(x, y, screen_w as u32, screen_h as u32);
            }
            crate::gui::request_redraw();
        } else if mouse.right_button && !prev_right {
            // Right-click
            drop(mouse);
            handle_right_click(x, y, screen_w as u32, screen_h as u32);
            crate::gui::request_redraw();
        } else if mouse.middle_button && !prev_middle && middle_paste {
            // Middle-click paste — pastes clipboard content at cursor position
            drop(mouse);
            handle_middle_click_paste(x, y);
            crate::gui::request_redraw();
        } else if !mouse.left_button && prev_left {
            // Mouse up - stop dragging/resizing and apply snap zones
            drop(mouse);
            handle_mouse_up(x, y);
            crate::gui::request_redraw();
        } else if mouse.left_button {
            // Mouse drag — handle_drag pushes targeted damage rects
            drop(mouse);
            handle_drag(x, y);
        } else if moved {
            drop(mouse);
            // Always update cursor type based on position first (resize cursors, hand, etc.)
            // This is lightweight — just sets an atomic u8.
            crate::gui::desktop::update_cursor_for_position(x, y);

            // Exposé hover tracking
            if crate::gui::expose::is_visible() {
                crate::gui::expose::handle_mouse_move(x, y);
            }

            // Focus-follows-mouse: if enabled, focus the window under the cursor
            if focus_mode() == FocusMode::FocusFollowsMouse {
                let taskbar_y = screen_h - crate::gui::scale::taskbar_height() as i32;
                // Only apply FFM in the window area (not on taskbar, start menu, etc.)
                if y < taskbar_y
                    && !startmenu::is_visible()
                    && !crate::gui::popups::any_popup_open()
                {
                    let mut wm = window::WINDOW_MANAGER.lock();
                    if let Some(hover_id) = wm.window_at(x, y) {
                        if wm.focused_window != Some(hover_id) {
                            wm.focus_window(hover_id);
                            drop(wm);
                            crate::gui::request_redraw();
                        }
                    }
                }
            }

            // Hot corners — check if mouse is in a screen corner
            crate::gui::hot_corners::update(x, y, screen_w, screen_h);

            // When hovering over interactive overlay elements that have visible hover
            // highlights (context menu items, start menu, popups), use a full redraw.
            // For everything else (taskbar, title bars, resize edges, desktop), use
            // the fast cursor-only path — the cursor type was already updated above.
            if needs_overlay_hover_redraw(x, y, screen_w, screen_h) {
                crate::gui::request_redraw();
            } else {
                crate::gui::request_cursor_redraw();
            }
        }
    }
}

/// Set cursor position from an absolute pointing device (VirtIO tablet).
/// Coordinates are already in screen pixels. No mouse acceleration is applied.
/// Handles button transitions (click, release, drag) and scroll identically
/// to the PS/2 path but without relative-to-absolute conversion.
pub fn set_absolute_mouse(x: i32, y: i32, left: bool, right: bool, middle: bool, scroll: i8) {
    let (screen_w, screen_h) = crate::gui::cached_screen_size();

    let mut mouse = MOUSE.lock();

    let prev_left = mouse.left_button;
    let prev_right = mouse.right_button;
    let prev_middle = mouse.middle_button;
    let old_x = mouse.x;
    let old_y = mouse.y;

    // Set position directly (clamped to screen bounds)
    mouse.x = x.clamp(0, screen_w - 1);
    mouse.y = y.clamp(0, screen_h - 1);

    // Apply left-handed button swap if enabled
    let settings = MOUSE_SETTINGS.lock();
    let left_handed = settings.left_handed;
    let middle_paste = settings.middle_click_paste;
    let natural_scrolling = settings.natural_scrolling;
    drop(settings);

    if left_handed {
        mouse.left_button = right;
        mouse.right_button = left;
    } else {
        mouse.left_button = left;
        mouse.right_button = right;
    }
    mouse.middle_button = middle;

    // Sync with tablet driver for cursor rendering
    crate::virtio_tablet::set_position(mouse.x, mouse.y);
    crate::virtio_tablet::set_buttons(mouse.left_button, mouse.right_button, mouse.middle_button);

    let cx = mouse.x;
    let cy = mouse.y;
    let moved = cx != old_x || cy != old_y;

    // Handle scroll wheel
    if scroll != 0 {
        let effective_scroll = if natural_scrolling { -scroll } else { scroll };
        mouse.scroll_delta = effective_scroll;
        drop(mouse);
        handle_scroll(cx, cy, effective_scroll, screen_w as u32, screen_h as u32);
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(wid) = wm.window_at(cx, cy) {
            if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
                push_window_damage(win.rect);
            } else {
                drop(wm);
                crate::gui::request_redraw();
            }
        } else {
            drop(wm);
            crate::gui::request_redraw();
        }
        return;
    }

    // Handle left click (button down)
    if mouse.left_button && !prev_left {
        let ticks = crate::interrupts::get_ticks();
        let is_double_click = (ticks - mouse.last_click_tick) < 40
            && (cx - mouse.last_click_x).abs() < 8
            && (cy - mouse.last_click_y).abs() < 8;

        mouse.last_click_x = cx;
        mouse.last_click_y = cy;
        mouse.last_click_tick = ticks;

        drop(mouse);

        desktop::close_context_menu();
        crate::gui::system_tray::close_context_menu();

        if is_double_click {
            let on_button = is_on_window_button(cx, cy);
            if on_button {
                handle_click(cx, cy, screen_w as u32, screen_h as u32);
            } else {
                handle_double_click(cx, cy, screen_w as u32, screen_h as u32);
            }
        } else {
            handle_click(cx, cy, screen_w as u32, screen_h as u32);
        }
        crate::gui::request_redraw();
    } else if mouse.right_button && !prev_right {
        // Right-click
        drop(mouse);
        handle_right_click(cx, cy, screen_w as u32, screen_h as u32);
        crate::gui::request_redraw();
    } else if mouse.middle_button && !prev_middle && middle_paste {
        // Middle-click paste
        drop(mouse);
        handle_middle_click_paste(cx, cy);
        crate::gui::request_redraw();
    } else if !mouse.left_button && prev_left {
        // Mouse up — stop dragging, apply snap zones
        drop(mouse);
        handle_mouse_up(cx, cy);
        crate::gui::request_redraw();
    } else if mouse.left_button {
        // Mouse drag
        drop(mouse);
        handle_drag(cx, cy);
    } else if moved {
        drop(mouse);
        // Update cursor type (resize cursors, hand, etc.)
        crate::gui::desktop::update_cursor_for_position(cx, cy);

        if crate::gui::expose::is_visible() {
            crate::gui::expose::handle_mouse_move(cx, cy);
        }

        // Focus-follows-mouse
        if focus_mode() == FocusMode::FocusFollowsMouse {
            let taskbar_y = screen_h - crate::gui::scale::taskbar_height() as i32;
            if cy < taskbar_y && !startmenu::is_visible() && !crate::gui::popups::any_popup_open() {
                let mut wm = window::WINDOW_MANAGER.lock();
                if let Some(hover_id) = wm.window_at(cx, cy) {
                    if wm.focused_window != Some(hover_id) {
                        wm.focus_window(hover_id);
                        drop(wm);
                        crate::gui::request_redraw();
                    }
                }
            }
        }

        crate::gui::hot_corners::update(cx, cy, screen_w, screen_h);
        if needs_overlay_hover_redraw(cx, cy, screen_w, screen_h) {
            crate::gui::request_redraw();
        } else {
            crate::gui::request_cursor_redraw();
        }
    } else {
        drop(mouse);
    }
}
