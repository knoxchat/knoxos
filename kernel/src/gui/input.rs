use conquer_once::spin::OnceCell;
/// Input Handling - Mouse and keyboard event processing
/// PS/2 mouse driver and high-level input event dispatching
use core::task::Poll;
use crossbeam_queue::ArrayQueue;
use spin::Mutex;

use super::desktop;
use super::startmenu;
use super::taskbar;
use super::window;
use super::window::WindowContentType;

// ═══════════════════════════════════════════════════════════════════════
// MOUSE SETTINGS (sensitivity, natural scrolling, middle-click paste)
// ═══════════════════════════════════════════════════════════════════════

/// Mouse / trackpad configuration
#[derive(Debug, Clone, Copy)]
pub struct MouseSettings {
    /// Mouse sensitivity / speed multiplier (0.25 = slow, 1.0 = default, 3.0 = fast)
    pub sensitivity: f32,
    /// Enable mouse acceleration curve
    pub acceleration_enabled: bool,
    /// Natural (reverse) scrolling — scroll content follows finger direction
    pub natural_scrolling: bool,
    /// Scroll lines per wheel notch (default: 3)
    pub scroll_lines: u8,
    /// Middle-click pastes from clipboard
    pub middle_click_paste: bool,
    /// Left-handed mode (swap left/right buttons)
    pub left_handed: bool,
}

impl MouseSettings {
    pub fn default_settings() -> Self {
        Self {
            sensitivity: 1.0,
            acceleration_enabled: false,
            natural_scrolling: false,
            scroll_lines: 3,
            middle_click_paste: true,
            left_handed: false,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref MOUSE_SETTINGS: Mutex<MouseSettings> = Mutex::new(MouseSettings::default_settings());
}

/// Get current mouse settings
pub fn mouse_settings() -> MouseSettings {
    *MOUSE_SETTINGS.lock()
}

/// Update mouse settings
pub fn set_mouse_settings(settings: MouseSettings) {
    *MOUSE_SETTINGS.lock() = settings;
}

// ─── Focus Mode ──────────────────────────────────────────────────────

/// Focus mode: ClickToFocus (default) or FocusFollowsMouse
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    /// Window gains focus only when clicked (default)
    ClickToFocus,
    /// Window gains focus when the mouse pointer enters it
    FocusFollowsMouse,
}

/// Global focus mode setting
static FOCUS_MODE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Set the focus mode
pub fn set_focus_mode(mode: FocusMode) {
    let val = match mode {
        FocusMode::ClickToFocus => 0,
        FocusMode::FocusFollowsMouse => 1,
    };
    FOCUS_MODE.store(val, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current focus mode
pub fn focus_mode() -> FocusMode {
    match FOCUS_MODE.load(core::sync::atomic::Ordering::Relaxed) {
        1 => FocusMode::FocusFollowsMouse,
        _ => FocusMode::ClickToFocus,
    }
}

// ─── Mouse State ─────────────────────────────────────────────────────

/// PS/2 mouse packet queue
static MOUSE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

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
    let (screen_w, screen_h) = super::cached_screen_size();

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
                    super::request_redraw();
                }
            } else {
                drop(wm);
                super::request_redraw();
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
            super::system_tray::close_context_menu();

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
            super::request_redraw();
        } else if mouse.right_button && !prev_right {
            // Right-click
            drop(mouse);
            handle_right_click(x, y, screen_w as u32, screen_h as u32);
            super::request_redraw();
        } else if mouse.middle_button && !prev_middle && middle_paste {
            // Middle-click paste — pastes clipboard content at cursor position
            drop(mouse);
            handle_middle_click_paste(x, y);
            super::request_redraw();
        } else if !mouse.left_button && prev_left {
            // Mouse up - stop dragging/resizing and apply snap zones
            drop(mouse);
            handle_mouse_up(x, y);
            super::request_redraw();
        } else if mouse.left_button {
            // Mouse drag — handle_drag pushes targeted damage rects
            drop(mouse);
            handle_drag(x, y);
        } else if moved {
            drop(mouse);
            // Always update cursor type based on position first (resize cursors, hand, etc.)
            // This is lightweight — just sets an atomic u8.
            super::desktop::update_cursor_for_position(x, y);

            // Exposé hover tracking
            if super::expose::is_visible() {
                super::expose::handle_mouse_move(x, y);
            }

            // Focus-follows-mouse: if enabled, focus the window under the cursor
            if focus_mode() == FocusMode::FocusFollowsMouse {
                let taskbar_y = screen_h - super::scale::taskbar_height() as i32;
                // Only apply FFM in the window area (not on taskbar, start menu, etc.)
                if y < taskbar_y && !startmenu::is_visible() && !super::popups::any_popup_open() {
                    let mut wm = window::WINDOW_MANAGER.lock();
                    if let Some(hover_id) = wm.window_at(x, y) {
                        if wm.focused_window != Some(hover_id) {
                            wm.focus_window(hover_id);
                            drop(wm);
                            super::request_redraw();
                        }
                    }
                }
            }

            // Hot corners — check if mouse is in a screen corner
            super::hot_corners::update(x, y, screen_w, screen_h);

            // When hovering over interactive overlay elements that have visible hover
            // highlights (context menu items, start menu, popups), use a full redraw.
            // For everything else (taskbar, title bars, resize edges, desktop), use
            // the fast cursor-only path — the cursor type was already updated above.
            if needs_overlay_hover_redraw(x, y, screen_w, screen_h) {
                super::request_redraw();
            } else {
                super::request_cursor_redraw();
            }
        }
    }
}

/// Check if the mouse is over an interactive OVERLAY element that has visible
/// hover-highlight state changes. Only these truly need a full desktop redraw.
///
/// Elements that just change cursor shape (resize edges, title bar, desktop icons)
/// do NOT need a full redraw — the cursor type is updated separately via
/// `update_cursor_for_position()` and the cursor-only fast path handles it.
///
/// This dramatically reduces full redraws during normal mouse movement, keeping
/// the cursor responsive even with expensive AA rendering.
fn needs_overlay_hover_redraw(x: i32, y: i32, screen_w: i32, screen_h: i32) -> bool {
    // Context menu visible — items have hover highlight
    if desktop::CONTEXT_MENU.lock().visible {
        return true;
    }
    // Start menu visible — items have hover highlight
    if startmenu::is_visible() {
        return true;
    }
    // System popups visible — need redraw for interaction
    if super::popups::any_popup_open() {
        return true;
    }
    // Notification panel open — need redraw for interaction
    if super::notifications::is_panel_open() {
        return true;
    }
    // If dragging or resizing a window, the drag handler pushes targeted
    // damage rects — no full redraw needed on the hover path.
    // (This check is defensive: mouse.left_button should be false here
    // since we're in the 'else if moved' branch, not the drag branch.)
    {
        let wm = window::WINDOW_MANAGER.lock();
        if wm.any_dragging() || wm.any_resizing() {
            return false;
        }
    }
    // Over the taskbar — only redraw if hover state actually changed
    let taskbar_y = screen_h - super::scale::taskbar_height() as i32;
    // Update tray context menu hover even if above taskbar
    if super::system_tray::is_context_menu_open() {
        super::system_tray::update_context_menu_hover(x, y);
    }
    if y >= taskbar_y {
        // Pre-compute the new hover state and compare with current
        // If unchanged, skip the full redraw (just cursor-only)
        let old_hover = taskbar::TASKBAR.lock().hovered_entry;
        // update_hover is called in draw_desktop, but we can peek here cheaply
        // by checking if the mouse is over any entry
        // If we return true and old_hover == new_hover, it's wasted work.
        // So instead: always update hover, and only redraw if it changed.
        taskbar::update_hover(x, y, screen_w as u32, screen_h as u32);
        taskbar::update_window_preview(screen_w as u32, screen_h as u32);
        super::system_tray::update_context_menu_hover(x, y);
        let new_hover = taskbar::TASKBAR.lock().hovered_entry;
        if old_hover != new_hover {
            return true;
        }
        // Hover didn't change — cursor-only is fine
        return false;
    }

    // Window title-bar buttons have visible hover highlights (close→red,
    // max/min→subtle glow). Trigger a full redraw so these highlights render.
    {
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(wid) = wm.window_at(x, y) {
            if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
                if (win.closeable && win.close_button_rect().contains(x, y))
                    || (win.maximizable && win.maximize_button_rect().contains(x, y))
                    || (win.minimizable && win.minimize_button_rect().contains(x, y))
                {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if a point is on any window's title-bar button (close/max/min).
/// Used to prevent double-click detection from swallowing button clicks.
fn is_on_window_button(x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            return (win.closeable && win.close_button_rect().contains(x, y))
                || (win.maximizable && win.maximize_button_rect().contains(x, y))
                || (win.minimizable && win.minimize_button_rect().contains(x, y));
        }
    }
    false
}

/// Handle mouse scroll wheel events
/// scroll_z: positive = scroll up, negative = scroll down
fn handle_scroll(x: i32, y: i32, scroll_z: i8, screen_w: u32, screen_h: u32) {
    // File picker modal — intercept scroll when visible
    if super::file_picker::handle_scroll(x, y, scroll_z) {
        return;
    }

    let settings = MOUSE_SETTINGS.lock();
    let lines = settings.scroll_lines as usize;
    drop(settings);

    // Check if scrolling over a terminal window
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at_inner(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::Terminal {
                // Route scroll to active terminal tab
                let active_term_id = if win.terminal_tabs.is_empty() {
                    wid
                } else {
                    win.terminal_tabs
                        .get(win.terminal_active_tab)
                        .copied()
                        .unwrap_or(wid)
                };
                drop(wm);
                if scroll_z > 0 {
                    crate::terminal::scroll_window(active_term_id, true, lines);
                } else {
                    crate::terminal::scroll_window(active_term_id, false, lines);
                }
                return;
            }
            // For other window types, handle scroll_y if applicable
            if win.content_type == WindowContentType::FileExplorer
                || win.content_type == WindowContentType::Browser
                || win.content_type == WindowContentType::Settings
                || win.content_type == WindowContentType::AIAssistant
                || win.content_type == WindowContentType::TextEditor
                || win.content_type == WindowContentType::ArchiveViewer
                || win.content_type == WindowContentType::DiskUtility
                || win.content_type == WindowContentType::BluetoothManager
                || win.content_type == WindowContentType::CalendarApp
                || win.content_type == WindowContentType::LogViewer
                || win.content_type == WindowContentType::SoftwareUpdater
                || win.content_type == WindowContentType::SoftwareCenter
                || win.content_type == WindowContentType::SetupWizard
            {
                drop(wm);
                // Route scroll to generic window scroll state
                let mut wm = window::WINDOW_MANAGER.lock();
                if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                    if scroll_z > 0 {
                        win.scroll_by(-(lines as i32 * 20));
                    } else {
                        win.scroll_by(lines as i32 * 20);
                    }
                }
                return;
            }
        }
    }
    drop(wm);

    // Check if scrolling over start menu
    if startmenu::is_visible() {
        startmenu::handle_scroll(scroll_z);
    }
}

/// Handle right-click (context menu)
fn handle_right_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    let taskbar_y = screen_h as i32 - super::scale::taskbar_height() as i32;

    // Close any open tray context menu first
    if super::system_tray::is_context_menu_open()
        && super::system_tray::handle_context_menu_click(x, y)
    {
        return;
    }

    // On taskbar — check if right-clicking on a tray icon
    if y >= taskbar_y {
        if let Some((tray_x, dock_cy)) = super::taskbar::tray_layout(screen_w, screen_h) {
            if super::system_tray::handle_right_click(x, y, tray_x, dock_cy) {
                return;
            }
        }
        // Right-click elsewhere on taskbar — show taskbar context menu
        super::taskbar::show_context_menu(x, y);
        return;
    }

    // Check if right-clicking inside a file explorer window
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::FileExplorer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::explorer::handle_right_click(wid, x, y);
                    super::request_redraw();
                    return;
                }
            }
        }
        drop(wm);
        return; // Don't show desktop context menu on other windows
    }
    drop(wm);

    // Show desktop context menu
    desktop::show_context_menu(x, y);
}

/// Handle a single click
fn handle_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    crate::serial_println!(
        "[CLICK-DBG] handle_click({},{}) screen={}x{}",
        x,
        y,
        screen_w,
        screen_h
    );
    // If login screen is active, route clicks there
    if !super::login::is_logged_in() {
        crate::serial_println!("[CLICK-DBG] consumed by: login");
        super::login::handle_click(x, y, screen_w as i32, screen_h as i32);
        return;
    }

    // If screen is locked, route clicks to lock screen
    if super::lock_screen::is_locked() {
        crate::serial_println!("[CLICK-DBG] consumed by: lock_screen");
        super::lock_screen::handle_click(x, y, screen_w as i32, screen_h as i32);
        return;
    }

    // Check context menu first (if visible, clicking an item)
    if desktop::handle_context_menu_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: context_menu");
        return;
    }

    // Exposé / Mission Control — intercepts clicks when visible
    if super::expose::is_visible() {
        crate::serial_println!("[CLICK-DBG] consumed by: expose");
        if let Some(wid) = super::expose::handle_click(x, y) {
            let mut wm = window::WINDOW_MANAGER.lock();
            wm.focus_window(wid);
            drop(wm);
            taskbar::set_active(wid);
        }
        super::request_redraw();
        return;
    }

    // Check taskbar context menu
    if taskbar::handle_context_menu_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: taskbar_context_menu");
        return;
    }

    // File picker modal — intercepts all clicks when visible
    if super::file_picker::handle_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: file_picker");
        return;
    }

    // Keyboard shortcuts overlay — intercepts clicks when visible
    if super::shortcuts_overlay::handle_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: shortcuts_overlay");
        return;
    }

    // Check tray context menu
    if super::system_tray::is_context_menu_open()
        && super::system_tray::handle_context_menu_click(x, y)
    {
        crate::serial_println!("[CLICK-DBG] consumed by: tray_context_menu");
        return;
    }

    // Check toast notification clicks (top-right toasts)
    if super::notifications::handle_toast_click(x, y, screen_w as i32) {
        crate::serial_println!("[CLICK-DBG] consumed by: toast");
        return;
    }

    // Check notification panel clicks
    if super::notifications::is_panel_open()
        && super::notifications::handle_panel_click(x, y, screen_w as i32, screen_h as i32)
    {
        crate::serial_println!("[CLICK-DBG] consumed by: notification_panel");
        return;
    }

    // Check taskbar clicks BEFORE start menu, so clicking the start button
    // properly toggles the menu (instead of close→reopen race)
    let taskbar_y = screen_h as i32 - super::scale::taskbar_height() as i32;
    if y >= taskbar_y && taskbar::handle_click(x, y, screen_w, screen_h) {
        crate::serial_println!("[CLICK-DBG] consumed by: taskbar");
        return;
    }

    // Check start menu (if visible, clicking a menu item or outside to close)
    if startmenu::is_visible() {
        if let Some(item) = startmenu::handle_click(x, y) {
            crate::serial_println!("[KnoxOS] Start menu: Opening {}", item.name);
            desktop::open_application(&item.name, item.icon_type);
            return;
        }
        // If clicking outside the menu (and not on taskbar which was handled above), close it
        startmenu::close();
    }

    // Check system popups (calendar, volume, quick settings)
    if super::popups::any_popup_open() {
        let sw = screen_w as i32;
        let sh = screen_h as i32;
        if super::popups::handle_calendar_click(x, y, sw, sh) {
            crate::serial_println!("[CLICK-DBG] consumed by: calendar_popup");
            return;
        }
        if super::popups::handle_volume_click(x, y, sw, sh) {
            crate::serial_println!("[CLICK-DBG] consumed by: volume_popup");
            return;
        }
        if super::popups::handle_quick_settings_click(x, y, sw, sh) {
            crate::serial_println!("[CLICK-DBG] consumed by: quick_settings_popup");
            return;
        }
        // Click outside all popups — close them
        super::popups::close_all_popups();
    }

    // Check notification panel — clicking INSIDE the panel is handled by the
    // earlier handle_panel_click call.  Clicking OUTSIDE closes the panel but
    // lets the click fall through to the underlying element (consistent with
    // start menu / popup behavior).
    {
        let mut nc = super::notifications::NOTIFICATIONS.lock();
        if nc.panel_open {
            let panel_w: u32 = 340;
            let panel_x = screen_w as i32 - panel_w as i32 - 8;
            let panel_rect = super::framebuffer::Rect::new(panel_x, 36, panel_w, 400);
            if panel_rect.contains(x, y) {
                // Click inside panel — already handled above; consume
                crate::serial_println!("[CLICK-DBG] consumed by: notification_panel_inside");
                drop(nc);
                return;
            }
            // Click outside — close panel and fall through
            nc.panel_open = false;
        }
    }

    // Check for resize edge on any visible window (before checking interior clicks)
    {
        let wm = window::WINDOW_MANAGER.lock();
        let (resize_wid, edge) = wm.resize_edge_at(x, y);
        if let Some(wid) = resize_wid {
            if edge.is_resizing() {
                crate::serial_println!(
                    "[CLICK-DBG] consumed by: resize_edge wid={} edge={:?}",
                    wid,
                    edge
                );
                drop(wm);
                let mut wm = window::WINDOW_MANAGER.lock();
                wm.focus_window(wid);
                if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                    win.resizing = edge;
                }
                drop(wm);
                taskbar::set_active(wid);
                return;
            }
        }
    }

    // Check windows (top to bottom z-order)
    crate::serial_println!("[CLICK-DBG] reached window check at ({},{})", x, y);
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        drop(wm);

        let mut wm = window::WINDOW_MANAGER.lock();
        wm.focus_window(wid);

        // Check title bar buttons (using scale-aware button rects)
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            let close_rect = win.close_button_rect();
            crate::serial_println!(
                "[HIT] wid={} click=({},{}) close_rect=({},{},{}x{}) closeable={}",
                wid,
                x,
                y,
                close_rect.x,
                close_rect.y,
                close_rect.width,
                close_rect.height,
                win.closeable
            );
            if win.closeable && close_rect.contains(x, y) {
                let id = win.id;
                let is_terminal = win.content_type == WindowContentType::Terminal;
                let is_browser = win.content_type == WindowContentType::Browser;
                let is_ai = win.content_type == WindowContentType::AIAssistant;
                // Collect terminal tab IDs before closing
                let tab_ids: alloc::vec::Vec<u32> = win.terminal_tabs.clone();
                drop(wm);
                // Emit CloseRequested event
                super::window_events::push_event(
                    id,
                    super::window_events::WindowEvent::CloseRequested,
                );
                window::WINDOW_MANAGER.lock().close_window(id);
                // Clean up event queue
                super::window_events::unregister_window(id);
                taskbar::remove_entry(id);
                // Clean up terminal instance if this was a terminal window
                if is_terminal {
                    crate::terminal::destroy_for_window(id);
                    // Also destroy any additional tab terminals
                    for tab_id in &tab_ids {
                        if *tab_id != id {
                            crate::terminal::destroy_tab(*tab_id);
                        }
                    }
                }
                // Clean up browser instance if this was a browser window
                if is_browser {
                    super::browser::destroy_for_window(id);
                }
                // Clean up AI assistant state if this was an AI window
                if is_ai {
                    super::ai_assistant::destroy_for_window(id);
                }
                return;
            }
            if win.maximizable && win.maximize_button_rect().contains(x, y) {
                let id = win.id;
                drop(wm);
                window::WINDOW_MANAGER
                    .lock()
                    .toggle_maximize(id, screen_w, screen_h);
                // Emit Maximized/Restored event
                let wm2 = window::WINDOW_MANAGER.lock();
                if let Some(w) = wm2.windows.iter().find(|w| w.id == id) {
                    let evt = match w.state {
                        window::WindowState::Maximized => {
                            super::window_events::WindowEvent::Maximized
                        }
                        _ => super::window_events::WindowEvent::Restored,
                    };
                    super::window_events::push_event(id, evt);
                }
                return;
            }
            if win.minimizable && win.minimize_button_rect().contains(x, y) {
                let id = win.id;
                drop(wm);
                super::window_events::push_event(id, super::window_events::WindowEvent::Minimized);
                let mut wm = window::WINDOW_MANAGER.lock();
                wm.minimize_window(id);
                // Sync taskbar active state with newly focused window
                let new_focus = wm.focused_window;
                drop(wm);
                if let Some(fid) = new_focus {
                    taskbar::set_active(fid);
                } else {
                    // No windows left visible — deactivate all entries
                    let mut tb = taskbar::TASKBAR.lock();
                    for e in tb.entries.iter_mut() {
                        e.active = false;
                    }
                }
                return;
            }

            // Check for clicks on window content (settings tabs, etc.)
            if win.content_type == WindowContentType::Settings {
                let content = win.content_rect();
                crate::serial_println!(
                    "[CLICK-DBG] Settings content check: mouse=({},{}) rect=({},{},{}x{})",
                    x,
                    y,
                    content.x,
                    content.y,
                    content.width,
                    content.height
                );
                if content.contains(x, y) {
                    drop(wm);
                    handle_settings_content_click(x, y, wid);
                    taskbar::set_active(wid);
                    return;
                }
            }

            // Check for clicks on AI assistant content
            if win.content_type == WindowContentType::AIAssistant {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::ai_assistant::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on browser window content
            if win.content_type == WindowContentType::Browser {
                let content = win.content_rect();
                if content.contains(x, y) {
                    let scroll = win.scroll_y;
                    drop(wm);
                    super::browser::handle_browser_click(
                        wid,
                        x,
                        y,
                        content.x,
                        content.y,
                        content.width,
                        content.height,
                        scroll,
                    );
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on file explorer content
            if win.content_type == WindowContentType::FileExplorer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::explorer::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on archive viewer content
            if win.content_type == WindowContentType::ArchiveViewer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::archive_manager::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on disk utility content
            if win.content_type == WindowContentType::DiskUtility {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::disk_utility::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on bluetooth manager content
            if win.content_type == WindowContentType::BluetoothManager {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::bt_manager::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on calendar app content
            if win.content_type == WindowContentType::CalendarApp {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::calendar_app::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on log viewer content
            if win.content_type == WindowContentType::LogViewer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::log_viewer::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on software updater content
            if win.content_type == WindowContentType::SoftwareUpdater {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::software_updater::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on software center content
            if win.content_type == WindowContentType::SoftwareCenter {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::software_center::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on setup wizard content
            if win.content_type == WindowContentType::SetupWizard {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::setup_wizard::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on task manager content
            if win.content_type == WindowContentType::TaskManager {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::task_manager::handle_click(wid, content, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on calculator content
            if win.content_type == WindowContentType::Calculator {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::calculator::handle_click(wid, content, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on image viewer content
            if win.content_type == WindowContentType::ImageViewer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    super::image_viewer::handle_click(wid, content, x, y);
                    taskbar::set_active(wid);
                    super::request_redraw();
                    return;
                }
            }

            // ── Ctrl+Click on terminal content → open URL ──
            if win.content_type == WindowContentType::Terminal {
                let content = win.content_rect();
                let tab_bar_h: i32 = if win.terminal_tabs.len() > 1 { 26 } else { 0 };
                let term_area = crate::gui::framebuffer::Rect::new(
                    content.x,
                    content.y + tab_bar_h,
                    content.width,
                    content.height.saturating_sub(tab_bar_h as u32),
                );
                if term_area.contains(x, y) && crate::task::keyboard::is_ctrl_held() {
                    let term_id = if !win.terminal_tabs.is_empty() {
                        win.terminal_tabs
                            .get(win.terminal_active_tab)
                            .copied()
                            .unwrap_or(wid)
                    } else {
                        wid
                    };
                    drop(wm);
                    crate::terminal::handle_ctrl_click(term_id, x, y, term_area);
                    super::request_redraw();
                    return;
                }
            }

            // Check for clicks on terminal tab bar
            if win.content_type == WindowContentType::Terminal && win.terminal_tabs.len() > 1 {
                let content = win.content_rect();
                let tab_bar_h = 26i32;
                if y >= content.y && y < content.y + tab_bar_h {
                    let tab_w =
                        140i32.min(content.width as i32 / win.terminal_tabs.len().max(1) as i32);
                    let rel_x = x - content.x;

                    // Check "+" button
                    let plus_x = (win.terminal_tabs.len() as i32) * tab_w;
                    if rel_x >= plus_x && rel_x < plus_x + 24 {
                        // New tab
                        let wid_copy = wid;
                        let tab_idx = win.terminal_tabs.len();
                        drop(wm);
                        let tab_id = crate::terminal::create_tab(wid_copy, tab_idx);
                        let mut wm2 = window::WINDOW_MANAGER.lock();
                        if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid_copy) {
                            w.terminal_tabs.push(tab_id);
                            w.terminal_active_tab = tab_idx;
                        }
                        drop(wm2);
                        super::request_redraw();
                        return;
                    }

                    // Check tab clicks
                    let tab_idx = (rel_x / tab_w) as usize;
                    if tab_idx < win.terminal_tabs.len() {
                        // Check close button area (last 20px of tab)
                        let tab_local_x = rel_x - (tab_idx as i32 * tab_w);
                        if tab_local_x >= tab_w - 20 && win.terminal_tabs.len() > 1 {
                            // Close this tab
                            let tab_id = win.terminal_tabs[tab_idx];
                            let wid_copy = wid;
                            drop(wm);
                            crate::terminal::destroy_tab(tab_id);
                            let mut wm2 = window::WINDOW_MANAGER.lock();
                            if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid_copy) {
                                w.terminal_tabs.retain(|t| *t != tab_id);
                                if w.terminal_active_tab >= w.terminal_tabs.len() {
                                    w.terminal_active_tab = w.terminal_tabs.len().saturating_sub(1);
                                }
                            }
                            drop(wm2);
                        } else {
                            // Switch to this tab
                            let wid_mut = wid;
                            drop(wm);
                            let mut wm2 = window::WINDOW_MANAGER.lock();
                            if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid_mut) {
                                w.terminal_active_tab = tab_idx;
                            }
                            drop(wm2);
                        }
                        super::request_redraw();
                        return;
                    }
                }
            }

            // Check for scrollbar thumb click — start scrollbar drag
            if win.content_type != WindowContentType::Terminal
                && win.content_type != WindowContentType::Empty
            {
                let content = win.content_rect();
                let sb_w = 12i32; // hit target wider than visual 6-8px
                let sb_x = content.x + content.width as i32 - sb_w;
                let sb_top = content.y;
                let sb_bot = content.y + content.height as i32;
                if x >= sb_x && x <= sb_x + sb_w && y >= sb_top && y <= sb_bot {
                    let max_sc = win.max_scroll_y;
                    let id = win.id;
                    let track_h = content.height as i32;
                    drop(wm);
                    if max_sc > 0 && track_h > 0 {
                        let mut wm2 = window::WINDOW_MANAGER.lock();
                        if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == id) {
                            // Jump scroll to the click position on the track
                            let click_ratio = (y - sb_top) as f32 / track_h as f32;
                            let new_scroll = (click_ratio * max_sc as f32) as i32;
                            w.scroll_y = new_scroll.clamp(0, max_sc);
                            // Start drag from this new position
                            w.scrollbar_dragging = true;
                            w.scrollbar_drag_start_y = y;
                            w.scrollbar_drag_start_scroll = w.scroll_y;
                        }
                        drop(wm2);
                        super::request_redraw();
                    }
                    taskbar::set_active(wid);
                    return;
                }
            }

            // Start dragging if on title bar but NOT on any button
            let on_button = (win.closeable && win.close_button_rect().contains(x, y))
                || (win.maximizable && win.maximize_button_rect().contains(x, y))
                || (win.minimizable && win.minimize_button_rect().contains(x, y));
            if win.hit_test_titlebar(x, y) && !on_button {
                let win_state = win.state;
                drop(wm);
                let mut wm = window::WINDOW_MANAGER.lock();
                if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                    // If maximized or snapped, un-maximize/unsnap on drag start
                    // and reposition so the title bar follows the cursor
                    if win_state == window::WindowState::Maximized {
                        win.state = window::WindowState::Normal;
                        win.rect = win.saved_rect;
                        // Center the title bar under the cursor
                        win.rect.x = x - win.rect.width as i32 / 2;
                        win.rect.y = y.max(0);
                    } else if win_state == window::WindowState::SnappedLeft
                        || win_state == window::WindowState::SnappedRight
                        || win_state == window::WindowState::SnappedTopLeft
                        || win_state == window::WindowState::SnappedTopRight
                        || win_state == window::WindowState::SnappedBottomLeft
                        || win_state == window::WindowState::SnappedBottomRight
                    {
                        win.state = window::WindowState::Normal;
                        win.rect = win.pre_snap_rect;
                        win.rect.x = x - win.rect.width as i32 / 2;
                        win.rect.y = y.max(0);
                    }
                    win.dragging = true;
                    win.drag_offset_x = x - win.rect.x;
                    win.drag_offset_y = y - win.rect.y;
                }
            }
        }

        taskbar::set_active(wid);
        return;
    }

    drop(wm);

    // Click on desktop (no window found)
    crate::serial_println!("[CLICK] no window at ({},{}), desktop click", x, y);
    desktop::handle_click(x, y);
}

/// Handle double click
fn handle_double_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    // File picker modal — intercept double clicks when visible
    if super::file_picker::handle_double_click(x, y) {
        return;
    }

    let taskbar_y = screen_h as i32 - super::scale::taskbar_height() as i32;
    if y < taskbar_y {
        // Check for double-click on a window title bar → toggle maximize
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(wid) = wm.window_at(x, y) {
            if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
                // If double-clicking on a button, treat as single click (fire button action)
                let on_button = (win.closeable && win.close_button_rect().contains(x, y))
                    || (win.maximizable && win.maximize_button_rect().contains(x, y))
                    || (win.minimizable && win.minimize_button_rect().contains(x, y));
                if on_button {
                    drop(wm);
                    handle_click(x, y, screen_w, screen_h);
                    return;
                }
                // Only toggle maximize on title bar double-click (not on buttons)
                if win.maximizable && win.hit_test_titlebar(x, y) {
                    drop(wm);
                    window::WINDOW_MANAGER
                        .lock()
                        .toggle_maximize(wid, screen_w, screen_h);
                    return;
                }
                // Double-click in file explorer content → navigate/open
                if win.content_type == WindowContentType::FileExplorer {
                    let content = win.content_rect();
                    if content.contains(x, y) {
                        drop(wm);
                        super::explorer::handle_double_click(wid, x, y);
                        super::request_redraw();
                        return;
                    }
                }
                // For other content types (Settings, Browser, etc.),
                // treat double-click as a single click so the content
                // handler receives it.
                drop(wm);
                handle_click(x, y, screen_w, screen_h);
                return;
            }
        }
        drop(wm);
        desktop::handle_double_click(x, y);
    }
}

/// Snap preview zone: 0=none, 1=left, 2=right, 3=maximize
static LAST_SNAP_ZONE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Determine the snap zone at a given position
/// 0=none, 1=left, 2=right, 3=maximize,
/// 4=top-left, 5=top-right, 6=bottom-left, 7=bottom-right
#[inline]
fn snap_zone_at(x: i32, y: i32, screen_w: i32) -> u8 {
    let (_, screen_h) = super::cached_screen_size();
    let taskbar_h = super::scale::taskbar_height() as i32;
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
fn handle_drag(x: i32, y: i32) {
    let (screen_w, screen_h) = super::cached_screen_size();

    // ── Desktop icon drag ────────────────────────────────────
    if desktop::is_dragging_icon() {
        desktop::handle_icon_drag(x, y);
        return;
    }

    // ── Explorer file drag (9.70) ────────────────────────────
    if super::explorer::is_file_drag_active() {
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
                    super::explorer::start_file_drag(wid);
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
            let taskbar_top = screen_h - super::scale::taskbar_height() as i32;
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
                let taskbar_h = super::scale::taskbar_height();
                let h = screen_h as u32 - taskbar_h;
                let hw = screen_w as u32 / 2;
                let hh = h / 2;
                // Inline helper: push the rect for a given snap zone
                let push_zone = |z: u8| match z {
                    1 => super::push_damage(super::framebuffer::Rect::new(0, 0, hw, h)),
                    2 => super::push_damage(super::framebuffer::Rect::new(screen_w / 2, 0, hw, h)),
                    3 => {
                        super::push_damage(super::framebuffer::Rect::new(0, 0, screen_w as u32, h))
                    }
                    4 => super::push_damage(super::framebuffer::Rect::new(0, 0, hw, hh)),
                    5 => super::push_damage(super::framebuffer::Rect::new(screen_w / 2, 0, hw, hh)),
                    6 => super::push_damage(super::framebuffer::Rect::new(0, hh as i32, hw, hh)),
                    7 => super::push_damage(super::framebuffer::Rect::new(
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
fn push_window_damage(rect: super::framebuffer::Rect) {
    // Add padding for window shadow (16px) and resize grab area (6px)
    let pad = 20;
    super::push_damage(super::framebuffer::Rect::new(
        rect.x - pad,
        rect.y - pad,
        rect.width + pad as u32 * 2,
        rect.height + pad as u32 * 2,
    ));
}

/// Push a single merged damage rect covering two window positions (old + new).
/// More efficient than two separate pushes — results in one compositing pass.
#[inline]
fn push_merged_window_damage(a: super::framebuffer::Rect, b: super::framebuffer::Rect) {
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
    super::push_damage(super::framebuffer::Rect::new(
        ux0,
        uy0,
        (ux1 - ux0).max(0) as u32,
        (uy1 - uy0).max(0) as u32,
    ));
}

/// Handle mouse button release — apply snap zones if dragging near edges
fn handle_mouse_up(x: i32, y: i32) {
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
    if super::explorer::is_file_drag_active() {
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
            super::explorer::accept_file_drop(wid);
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
                if let Some(src) = super::explorer::dragged_file_path() {
                    desktop::accept_file_drop(&src);
                }
            }
            super::explorer::cancel_file_drag();
        }
        return;
    }

    // Notify the drag-and-drop state machine that the mouse was released.
    // Drop zones can pick up the payload during this frame.
    super::drag_and_drop::on_mouse_release();

    let (screen_w, screen_h) = super::cached_screen_size();

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

/// Set cursor position from an absolute pointing device (VirtIO tablet).
/// Coordinates are already in screen pixels. No mouse acceleration is applied.
/// Handles button transitions (click, release, drag) and scroll identically
/// to the PS/2 path but without relative-to-absolute conversion.
pub fn set_absolute_mouse(x: i32, y: i32, left: bool, right: bool, middle: bool, scroll: i8) {
    let (screen_w, screen_h) = super::cached_screen_size();

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
                super::request_redraw();
            }
        } else {
            drop(wm);
            super::request_redraw();
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
        super::system_tray::close_context_menu();

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
        super::request_redraw();
    } else if mouse.right_button && !prev_right {
        // Right-click
        drop(mouse);
        handle_right_click(cx, cy, screen_w as u32, screen_h as u32);
        super::request_redraw();
    } else if mouse.middle_button && !prev_middle && middle_paste {
        // Middle-click paste
        drop(mouse);
        handle_middle_click_paste(cx, cy);
        super::request_redraw();
    } else if !mouse.left_button && prev_left {
        // Mouse up — stop dragging, apply snap zones
        drop(mouse);
        handle_mouse_up(cx, cy);
        super::request_redraw();
    } else if mouse.left_button {
        // Mouse drag
        drop(mouse);
        handle_drag(cx, cy);
    } else if moved {
        drop(mouse);
        // Update cursor type (resize cursors, hand, etc.)
        super::desktop::update_cursor_for_position(cx, cy);

        if super::expose::is_visible() {
            super::expose::handle_mouse_move(cx, cy);
        }

        // Focus-follows-mouse
        if focus_mode() == FocusMode::FocusFollowsMouse {
            let taskbar_y = screen_h - super::scale::taskbar_height() as i32;
            if cy < taskbar_y && !startmenu::is_visible() && !super::popups::any_popup_open() {
                let mut wm = window::WINDOW_MANAGER.lock();
                if let Some(hover_id) = wm.window_at(cx, cy) {
                    if wm.focused_window != Some(hover_id) {
                        wm.focus_window(hover_id);
                        drop(wm);
                        super::request_redraw();
                    }
                }
            }
        }

        super::hot_corners::update(cx, cy, screen_w, screen_h);
        if needs_overlay_hover_redraw(cx, cy, screen_w, screen_h) {
            super::request_redraw();
        } else {
            super::request_cursor_redraw();
        }
    } else {
        drop(mouse);
    }
}

/// Initialize PS/2 mouse (x86_64 only — uses port I/O)
pub fn init_mouse() {
    #[cfg(target_arch = "x86_64")]
    {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;

        // Disable interrupts during init so ACK bytes don't go into the queue
        crate::arch_compat::instructions::interrupts::without_interrupts(|| {
            unsafe {
                let mut cmd_port = Port::<u8>::new(0x64);
                let mut data_port = Port::<u8>::new(0x60);

                // Enable auxiliary device
                wait_write();
                cmd_port.write(0xA8);

                // Enable interrupts on PS/2 controller
                wait_write();
                cmd_port.write(0x20);
                wait_read();
                let status = data_port.read() | 0x02;
                wait_write();
                cmd_port.write(0x60);
                wait_write();
                data_port.write(status);

                // Use default settings
                write_mouse(0xF6);
                read_mouse(); // consume ACK

                // ── Enable IntelliMouse protocol (4-byte packets with scroll wheel) ──
                // Magic sequence: set sample rate 200, 100, 80, then request device ID
                write_mouse(0xF3);
                read_mouse(); // set sample rate command, ACK
                write_mouse(200);
                read_mouse(); // rate = 200, ACK
                write_mouse(0xF3);
                read_mouse(); // set sample rate command, ACK
                write_mouse(100);
                read_mouse(); // rate = 100, ACK
                write_mouse(0xF3);
                read_mouse(); // set sample rate command, ACK
                write_mouse(80);
                read_mouse(); // rate = 80, ACK

                // Read device ID — should be 3 if IntelliMouse mode activated (was 0)
                write_mouse(0xF2); // Get Device ID
                read_mouse(); // ACK
                let device_id = read_mouse();
                let has_scroll = device_id == 3 || device_id == 4;

                if has_scroll {
                    MOUSE.lock().intellimouse = true;
                    crate::serial_println!(
                        "[KnoxOS] PS/2 Mouse: IntelliMouse mode (scroll wheel enabled, ID={})",
                        device_id
                    );
                } else {
                    crate::serial_println!(
                        "[KnoxOS] PS/2 Mouse: Standard mode (no scroll wheel, ID={})",
                        device_id
                    );
                }

                // Enable mouse data reporting
                write_mouse(0xF4);
                read_mouse(); // consume ACK

                // Flush any remaining bytes from the data port
                for _ in 0..16 {
                    let mut status_port = Port::<u8>::new(0x64);
                    if status_port.read() & 0x01 != 0 {
                        let _ = data_port.read();
                    } else {
                        break;
                    }
                }
            }
        });

        // Drain any bytes that leaked into our queue during init
        if let Ok(queue) = MOUSE_QUEUE.try_get() {
            while queue.pop().is_some() {}
        }

        crate::serial_println!("[KnoxOS] PS/2 Mouse initialized");
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // On aarch64/riscv64, mouse input comes from VirtIO input or device tree
        crate::serial_println!("[KnoxOS] PS/2 not available on this arch — using VirtIO/DT input");
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn wait_write() {
    let mut port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..10000 {
        if port.read() & 0x02 == 0 {
            return;
        }
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn wait_read() {
    let mut port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..10000 {
        if port.read() & 0x01 != 0 {
            return;
        }
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn write_mouse(byte: u8) {
    let mut cmd_port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    let mut data_port = crate::arch_compat::instructions::port::Port::<u8>::new(0x60);
    wait_write();
    cmd_port.write(0xD4);
    wait_write();
    data_port.write(byte);
}

#[cfg(target_arch = "x86_64")]
unsafe fn read_mouse() -> u8 {
    let mut data_port = crate::arch_compat::instructions::port::Port::<u8>::new(0x60);
    wait_read();
    data_port.read()
}

/// Handle middle-click paste — inserts clipboard contents at the focused widget
fn handle_middle_click_paste(x: i32, y: i32) {
    // Check if a terminal window is focused — paste into it
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::Terminal {
                let term_id = if !win.terminal_tabs.is_empty() {
                    win.terminal_tabs
                        .get(win.terminal_active_tab)
                        .copied()
                        .unwrap_or(wid)
                } else {
                    wid
                };
                drop(wm);
                // Read clipboard and paste into terminal as key events
                if let Some(clip) = crate::clipboard::paste_text() {
                    for ch in clip.chars() {
                        crate::terminal::handle_key_for_window(
                            term_id,
                            crate::terminal::TerminalKey::Char(ch),
                        );
                    }
                }
                return;
            }
        }
    }
    drop(wm);
    // For other contexts, log the paste attempt
    if crate::clipboard::has_text() {
        crate::serial_println!("[INPUT] Middle-click paste (non-terminal context)");
    }
}

/// Handle click on settings window content (tab switching)
fn handle_settings_content_click(x: i32, y: i32, wid: window::WindowId) {
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
        let content = win.content_rect();
        let scroll_y = win.scroll_y;
        drop(wm);
        let mut state = super::settings::SETTINGS_STATE.lock();
        super::settings::handle_settings_click(x, y, content, &mut state, scroll_y);
    }
}
