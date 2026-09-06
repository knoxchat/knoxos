/// System Tray — Embedded tray icons in the Nebula Floating Dock
/// Provides Wi-Fi, Volume, Battery, and Notification bell indicators
/// with hover effects and click actions (opens popups).
///
/// The tray lives *inside* the dock pill, between the app entry separator
/// and the clock. Each icon is 16×16 drawn within a 28×28 hover cell.
use alloc::string::String;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

// ═══════════════════════════════════════════════════════════════════════
// TRAY LAYOUT CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// Size of each tray icon cell (icon + hover background)
pub const TRAY_ITEM_SIZE: u32 = 28;

/// Gap between tray icons
pub const TRAY_ITEM_GAP: i32 = 4;

/// Number of tray icons (Wi-Fi, Volume, Battery, Bluetooth, Notifications)
pub const TRAY_ICON_COUNT: u32 = 5;

/// Total width of the tray area
pub fn tray_width() -> u32 {
    TRAY_ICON_COUNT * TRAY_ITEM_SIZE + (TRAY_ICON_COUNT - 1) * TRAY_ITEM_GAP as u32
}

// ═══════════════════════════════════════════════════════════════════════
// TRAY STATE — Global indicators for system status
// ═══════════════════════════════════════════════════════════════════════

/// Wi-Fi signal strength: 0 = disconnected, 1-4 = signal bars
static WIFI_STRENGTH: AtomicU8 = AtomicU8::new(3);
/// Whether Wi-Fi is enabled
static WIFI_ENABLED: AtomicBool = AtomicBool::new(true);

/// Volume level: 0-100
static VOLUME_LEVEL: AtomicU8 = AtomicU8::new(75);
/// Whether volume is muted
static VOLUME_MUTED: AtomicBool = AtomicBool::new(false);

/// Battery percentage: 0-100
static BATTERY_PERCENT: AtomicU8 = AtomicU8::new(85);
/// Whether battery is charging
static BATTERY_CHARGING: AtomicBool = AtomicBool::new(true);

/// Bluetooth enabled
static BT_ENABLED: AtomicBool = AtomicBool::new(true);
/// Bluetooth connected to a device
static BT_CONNECTED: AtomicBool = AtomicBool::new(false);

/// Which tray icon is hovered (255 = none, 0-4 = icon index)
static TRAY_HOVERED: AtomicU8 = AtomicU8::new(255);

// ═══════════════════════════════════════════════════════════════════════
// TRAY CONTEXT MENU — Right-click popup menus for each tray icon
// ═══════════════════════════════════════════════════════════════════════

/// Context menu state
struct TrayContextMenu {
    /// Which icon was right-clicked (0-3), or 255 = closed
    icon: u8,
    /// Menu position (top-left of menu)
    x: i32,
    y: i32,
    /// Hovered item index (-1 = none)
    hovered: i32,
}

lazy_static::lazy_static! {
    static ref TRAY_CTX_MENU: Mutex<TrayContextMenu> = Mutex::new(TrayContextMenu {
        icon: 255,
        x: 0,
        y: 0,
        hovered: -1,
    });
}

/// Menu items for each tray icon
fn menu_items(icon: u8) -> &'static [&'static str] {
    match icon {
        0 => &["Toggle Wi-Fi", "Network Settings", "Available Networks"],
        1 => &["Toggle Mute", "Volume Up", "Volume Down", "Sound Settings"],
        2 => &["Battery Settings", "Power Saver Mode", "Battery Health"],
        3 => &["Toggle Bluetooth", "Paired Devices", "Bluetooth Settings"],
        4 => &["Clear All", "Do Not Disturb", "Notification Settings"],
        _ => &[],
    }
}

pub fn is_context_menu_open() -> bool {
    TRAY_CTX_MENU.lock().icon != 255
}

pub fn close_context_menu() {
    TRAY_CTX_MENU.lock().icon = 255;
}

/// Show a right-click context menu for the given tray icon index
fn show_context_menu(icon: u8, tray_x: i32, dock_cy: i32) {
    let ix = tray_x + icon as i32 * (TRAY_ITEM_SIZE as i32 + TRAY_ITEM_GAP);
    let items = menu_items(icon);
    let menu_w: i32 = 180;
    let item_h: i32 = 26;
    let menu_h = items.len() as i32 * item_h + 12;
    let menu_x = ix + TRAY_ITEM_SIZE as i32 / 2 - menu_w / 2;
    let menu_y = dock_cy - TRAY_ITEM_SIZE as i32 / 2 - menu_h - 4;
    let mut ctx = TRAY_CTX_MENU.lock();
    ctx.icon = icon;
    ctx.x = menu_x;
    ctx.y = menu_y;
    ctx.hovered = -1;
}

/// Handle right-click inside the tray area. Returns true if consumed.
pub fn handle_right_click(x: i32, y: i32, tray_x: i32, dock_cy: i32) -> bool {
    let half = TRAY_ITEM_SIZE as i32 / 2;
    for i in 0..TRAY_ICON_COUNT {
        let ix = tray_x + i as i32 * (TRAY_ITEM_SIZE as i32 + TRAY_ITEM_GAP);
        let item_rect = Rect::new(ix, dock_cy - half, TRAY_ITEM_SIZE, TRAY_ITEM_SIZE);
        if item_rect.contains(x, y) {
            super::popups::close_all_popups();
            super::notifications::close_panel();
            super::startmenu::close();
            show_context_menu(i as u8, tray_x, dock_cy);
            crate::serial_println!("[KnoxOS] Tray: right-click on icon {}", i);
            return true;
        }
    }
    false
}

/// Update hover state for the context menu
pub fn update_context_menu_hover(mx: i32, my: i32) {
    let mut ctx = TRAY_CTX_MENU.lock();
    if ctx.icon == 255 {
        return;
    }
    let items = menu_items(ctx.icon);
    let item_h: i32 = 26;
    let menu_w: i32 = 180;
    let items_y = ctx.y + 6;
    if mx >= ctx.x && mx < ctx.x + menu_w {
        for (i, _) in items.iter().enumerate() {
            let iy = items_y + i as i32 * item_h;
            if my >= iy && my < iy + item_h {
                ctx.hovered = i as i32;
                return;
            }
        }
    }
    ctx.hovered = -1;
}

/// Handle a click on the context menu. Returns true if consumed.
pub fn handle_context_menu_click(x: i32, y: i32) -> bool {
    let ctx = TRAY_CTX_MENU.lock();
    if ctx.icon == 255 {
        return false;
    }
    let items = menu_items(ctx.icon);
    let item_h: i32 = 26;
    let menu_w: i32 = 180;
    let items_y = ctx.y + 6;
    let icon = ctx.icon;
    // Check if click is inside the menu
    if x >= ctx.x
        && x < ctx.x + menu_w
        && y >= ctx.y
        && y < ctx.y + items.len() as i32 * item_h + 12
    {
        for (i, _) in items.iter().enumerate() {
            let iy = items_y + i as i32 * item_h;
            if y >= iy && y < iy + item_h {
                drop(ctx);
                execute_menu_action(icon, i);
                close_context_menu();
                return true;
            }
        }
        drop(ctx);
        close_context_menu();
        return true;
    }
    // Click outside — close
    drop(ctx);
    close_context_menu();
    true
}

/// Execute a tray context menu action
fn execute_menu_action(icon: u8, item: usize) {
    match icon {
        0 => match item {
            0 => {
                // Toggle Wi-Fi
                let cur = WIFI_ENABLED.load(Ordering::Relaxed);
                WIFI_ENABLED.store(!cur, Ordering::Relaxed);
                if cur {
                    WIFI_STRENGTH.store(0, Ordering::Relaxed);
                } else {
                    WIFI_STRENGTH.store(3, Ordering::Relaxed);
                }
                let msg = if cur {
                    "Wi-Fi disabled"
                } else {
                    "Wi-Fi enabled"
                };
                super::notifications::info("Network", msg);
            }
            1 => super::popups::toggle_quick_settings(), // Network Settings
            2 => super::popups::toggle_quick_settings(), // Available Networks
            _ => {}
        },
        1 => match item {
            0 => {
                // Toggle Mute
                let cur = VOLUME_MUTED.load(Ordering::Relaxed);
                VOLUME_MUTED.store(!cur, Ordering::Relaxed);
                let msg = if cur {
                    "Volume unmuted"
                } else {
                    "Volume muted"
                };
                super::notifications::info("Sound", msg);
            }
            1 => {
                // Volume Up
                let cur = VOLUME_LEVEL.load(Ordering::Relaxed);
                VOLUME_LEVEL.store(cur.saturating_add(10).min(100), Ordering::Relaxed);
                VOLUME_MUTED.store(false, Ordering::Relaxed);
            }
            2 => {
                // Volume Down
                let cur = VOLUME_LEVEL.load(Ordering::Relaxed);
                VOLUME_LEVEL.store(cur.saturating_sub(10), Ordering::Relaxed);
            }
            3 => super::popups::toggle_volume(), // Sound Settings
            _ => {}
        },
        2 => match item {
            0 => super::popups::toggle_quick_settings(), // Battery Settings
            1 => {
                // Power Saver Mode
                super::notifications::info("Battery", "Power Saver mode toggled");
            }
            2 => {
                // Battery Health
                let pct = BATTERY_PERCENT.load(Ordering::Relaxed);
                let charging = if BATTERY_CHARGING.load(Ordering::Relaxed) {
                    "charging"
                } else {
                    "discharging"
                };
                let msg = alloc::format!("Battery: {}%, {}", pct, charging);
                super::notifications::info("Battery", &msg);
            }
            _ => {}
        },
        3 => match item {
            0 => {
                // Toggle Bluetooth
                let cur = BT_ENABLED.load(Ordering::Relaxed);
                BT_ENABLED.store(!cur, Ordering::Relaxed);
                if cur {
                    BT_CONNECTED.store(false, Ordering::Relaxed);
                }
                let msg = if cur {
                    "Bluetooth disabled"
                } else {
                    "Bluetooth enabled"
                };
                super::notifications::info("Bluetooth", msg);
            }
            1 => {
                // Paired Devices
                super::notifications::info("Bluetooth", "No paired devices");
            }
            2 => super::popups::toggle_quick_settings(), // Bluetooth Settings
            _ => {}
        },
        4 => match item {
            0 => {
                // Clear All
                super::notifications::NOTIFICATIONS.lock().clear_all();
                super::notifications::info("Notifications", "All notifications cleared");
            }
            1 => {
                // Do Not Disturb
                super::notifications::info("Notifications", "Do Not Disturb toggled");
            }
            2 => super::notifications::toggle_panel(), // Notification Settings
            _ => {}
        },
        _ => {}
    }
}

/// Draw the tray context menu (if open)
pub fn draw_context_menu(fb: &mut FrameBuffer) {
    let ctx = TRAY_CTX_MENU.lock();
    if ctx.icon == 255 {
        return;
    }
    let items = menu_items(ctx.icon);
    if items.is_empty() {
        return;
    }

    let menu_w: u32 = 180;
    let item_h: i32 = 26;
    let menu_h = items.len() as u32 * item_h as u32 + 12;

    // Background with shadow
    fb.fill_rounded_rect_aa(
        Rect::new(ctx.x - 2, ctx.y - 2, menu_w + 4, menu_h + 4),
        Pixel::new(0, 0, 0, 80),
        10,
    );
    fb.fill_rounded_rect_aa(
        Rect::new(ctx.x, ctx.y, menu_w, menu_h),
        Pixel::rgb(28, 30, 36),
        8,
    );
    fb.draw_rounded_rect(
        Rect::new(ctx.x, ctx.y, menu_w, menu_h),
        Pixel::new(255, 255, 255, 25),
        8,
        1,
    );

    let items_y = ctx.y + 6;
    for (i, label) in items.iter().enumerate() {
        let iy = items_y + i as i32 * item_h;
        // Hover highlight
        if ctx.hovered == i as i32 {
            fb.fill_rounded_rect_aa(
                Rect::new(ctx.x + 4, iy, menu_w - 8, item_h as u32),
                Pixel::new(0, 200, 220, 30),
                4,
            );
        }
        // Icon indicator dot
        let dot_color = match ctx.icon {
            0 => Pixel::rgb(0, 200, 220),  // Wi-Fi: cyan
            1 => Pixel::rgb(120, 200, 80), // Volume: green
            2 => Pixel::rgb(255, 200, 60), // Battery: yellow
            3 => Pixel::rgb(80, 120, 255), // Bluetooth: blue
            4 => Pixel::rgb(220, 80, 160), // Notifications: pink
            _ => Pixel::rgb(180, 180, 180),
        };
        fb.fill_circle_aa(ctx.x + 14, iy + item_h / 2, 3, dot_color);
        // Label text
        fonts::draw_string_compact(fb, ctx.x + 26, iy + 7, label, Pixel::rgb(220, 220, 220), 1);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API — Get/Set system tray state
// ═══════════════════════════════════════════════════════════════════════

pub fn set_wifi_strength(strength: u8) {
    WIFI_STRENGTH.store(strength.min(4), Ordering::Relaxed);
}
pub fn set_wifi_enabled(enabled: bool) {
    WIFI_ENABLED.store(enabled, Ordering::Relaxed);
}
pub fn get_wifi_strength() -> u8 {
    WIFI_STRENGTH.load(Ordering::Relaxed)
}

pub fn set_volume(level: u8) {
    VOLUME_LEVEL.store(level.min(100), Ordering::Relaxed);
}
pub fn set_volume_muted(muted: bool) {
    VOLUME_MUTED.store(muted, Ordering::Relaxed);
}
pub fn get_volume() -> u8 {
    VOLUME_LEVEL.load(Ordering::Relaxed)
}
pub fn is_muted() -> bool {
    VOLUME_MUTED.load(Ordering::Relaxed)
}

pub fn set_battery(percent: u8) {
    BATTERY_PERCENT.store(percent.min(100), Ordering::Relaxed);
}
pub fn set_battery_charging(charging: bool) {
    BATTERY_CHARGING.store(charging, Ordering::Relaxed);
}
pub fn get_battery() -> u8 {
    BATTERY_PERCENT.load(Ordering::Relaxed)
}
pub fn is_charging() -> bool {
    BATTERY_CHARGING.load(Ordering::Relaxed)
}

pub fn set_bluetooth_enabled(enabled: bool) {
    BT_ENABLED.store(enabled, Ordering::Relaxed);
}
pub fn set_bluetooth_connected(connected: bool) {
    BT_CONNECTED.store(connected, Ordering::Relaxed);
}
pub fn is_bluetooth_enabled() -> bool {
    BT_ENABLED.load(Ordering::Relaxed)
}
pub fn is_bluetooth_connected() -> bool {
    BT_CONNECTED.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// HOVER / HIT-TEST
// ═══════════════════════════════════════════════════════════════════════

/// Update tray hover state. `tray_x` is the left edge of the tray area,
/// `dock_cy` is the vertical center of the dock.
pub fn update_hover(mx: i32, my: i32, tray_x: i32, dock_cy: i32) {
    let half = TRAY_ITEM_SIZE as i32 / 2;
    let mut found: u8 = 255; // none

    for i in 0..TRAY_ICON_COUNT {
        let ix = tray_x + i as i32 * (TRAY_ITEM_SIZE as i32 + TRAY_ITEM_GAP);
        let item_rect = Rect::new(ix, dock_cy - half, TRAY_ITEM_SIZE, TRAY_ITEM_SIZE);
        if item_rect.contains(mx, my) {
            found = i as u8;
            break;
        }
    }
    TRAY_HOVERED.store(found, Ordering::Relaxed);
}

/// Clear tray hover
pub fn clear_hover() {
    TRAY_HOVERED.store(255, Ordering::Relaxed);
}

/// Handle a click inside the tray area. Returns true if consumed.
/// `tray_x` is the left edge of the tray area, `dock_cy` the vertical center.
pub fn handle_click(x: i32, y: i32, tray_x: i32, dock_cy: i32) -> bool {
    let half = TRAY_ITEM_SIZE as i32 / 2;

    for i in 0..TRAY_ICON_COUNT {
        let ix = tray_x + i as i32 * (TRAY_ITEM_SIZE as i32 + TRAY_ITEM_GAP);
        let item_rect = Rect::new(ix, dock_cy - half, TRAY_ITEM_SIZE, TRAY_ITEM_SIZE);
        if item_rect.contains(x, y) {
            match i {
                0 => {
                    // Wi-Fi → toggle quick settings
                    super::popups::close_all_popups();
                    super::notifications::close_panel();
                    super::startmenu::close();
                    super::popups::toggle_quick_settings();
                    crate::serial_println!("[KnoxOS] Tray: Wi-Fi clicked → quick settings");
                }
                1 => {
                    // Volume → toggle volume popup
                    super::popups::close_all_popups();
                    super::notifications::close_panel();
                    super::startmenu::close();
                    super::popups::toggle_volume();
                    crate::serial_println!("[KnoxOS] Tray: Volume clicked");
                }
                2 => {
                    // Battery → toggle quick settings
                    super::popups::close_all_popups();
                    super::notifications::close_panel();
                    super::startmenu::close();
                    super::popups::toggle_quick_settings();
                    crate::serial_println!("[KnoxOS] Tray: Battery clicked → quick settings");
                }
                3 => {
                    // Bluetooth → toggle quick settings
                    super::popups::close_all_popups();
                    super::notifications::close_panel();
                    super::startmenu::close();
                    super::popups::toggle_quick_settings();
                    crate::serial_println!("[KnoxOS] Tray: Bluetooth clicked → quick settings");
                }
                4 => {
                    // Notifications bell → toggle notification panel
                    super::popups::close_all_popups();
                    super::startmenu::close();
                    super::notifications::toggle_panel();
                    crate::serial_println!("[KnoxOS] Tray: Notifications clicked");
                }
                _ => {}
            }
            return true;
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════
// DRAWING — Render all tray icons
// ═══════════════════════════════════════════════════════════════════════

/// Draw the system tray icons. Called from draw_taskbar().
/// `tray_x`: left edge of tray area, `dock_cy`: vertical center of dock.
pub fn draw(fb: &mut FrameBuffer, tray_x: i32, dock_cy: i32) {
    let hovered = TRAY_HOVERED.load(Ordering::Relaxed);
    let half = TRAY_ITEM_SIZE as i32 / 2;

    for i in 0..TRAY_ICON_COUNT {
        let ix = tray_x + i as i32 * (TRAY_ITEM_SIZE as i32 + TRAY_ITEM_GAP);
        let item_rect = Rect::new(ix, dock_cy - half, TRAY_ITEM_SIZE, TRAY_ITEM_SIZE);

        // Hover background glow
        if hovered == i as u8 {
            fb.fill_rounded_rect_aa(item_rect, colors::TASKBAR_HOVER, 6);
        }

        // Icon center coordinates
        let cx = ix + TRAY_ITEM_SIZE as i32 / 2;
        let cy = dock_cy;

        match i {
            0 => draw_wifi_icon(fb, cx, cy),
            1 => draw_volume_icon(fb, cx, cy),
            2 => draw_battery_icon(fb, cx, cy),
            3 => draw_bluetooth_icon(fb, cx, cy),
            4 => draw_notification_icon(fb, cx, cy),
            _ => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INDIVIDUAL ICON RENDERERS (all drawn centered on cx, cy)
// ═══════════════════════════════════════════════════════════════════════

/// Wi-Fi icon — concentric arcs representing signal strength
fn draw_wifi_icon(fb: &mut FrameBuffer, cx: i32, cy: i32) {
    let enabled = WIFI_ENABLED.load(Ordering::Relaxed);
    let strength = WIFI_STRENGTH.load(Ordering::Relaxed);

    if !enabled {
        // Disabled — draw gray X
        let gray = Pixel::new(100, 110, 130, 180);
        fb.draw_line_aa(cx - 5, cy - 5, cx + 5, cy + 5, gray);
        fb.draw_line_aa(cx + 5, cy - 5, cx - 5, cy + 5, gray);
        return;
    }

    // Signal bars (ascending height, left-to-right)
    let bar_w: u32 = 2;
    let bar_gap: i32 = 3;
    let base_y = cy + 6; // Bottom of bars

    for bar in 0u8..4 {
        let bar_h = 3 + bar as i32 * 3; // Heights: 3, 6, 9, 12
        let bx = cx - 7 + bar as i32 * bar_gap;
        let by = base_y - bar_h;

        let color = if bar < strength {
            // Active bar — cyan glow
            Pixel::new(0, 220, 255, 230)
        } else {
            // Inactive bar — dim
            Pixel::new(60, 80, 110, 120)
        };
        fb.fill_rounded_rect_aa(Rect::new(bx, by, bar_w, bar_h as u32), color, 1);
    }
}

/// Volume icon — speaker with sound waves
fn draw_volume_icon(fb: &mut FrameBuffer, cx: i32, cy: i32) {
    let muted = VOLUME_MUTED.load(Ordering::Relaxed);
    let level = VOLUME_LEVEL.load(Ordering::Relaxed);

    // Speaker body (small rectangle + triangle cone)
    let speaker_color = if muted {
        Pixel::new(100, 110, 130, 180)
    } else {
        Pixel::new(200, 220, 255, 230)
    };

    // Speaker rectangle (left part)
    fb.fill_rect(Rect::new(cx - 6, cy - 2, 3, 5), speaker_color);
    // Speaker cone (triangle to the right)
    for i in 0i32..5 {
        let w = 1 + i / 2;
        fb.fill_rect(Rect::new(cx - 3, cy - 2 + i, w as u32, 1), speaker_color);
    }

    if muted {
        // Red X for muted
        let red = Pixel::new(255, 80, 100, 220);
        fb.draw_line_aa(cx + 2, cy - 4, cx + 7, cy + 4, red);
        fb.draw_line_aa(cx + 7, cy - 4, cx + 2, cy + 4, red);
    } else {
        // Sound wave arcs based on volume
        let wave_color = Pixel::new(0, 200, 255, 180);
        let dim_color = Pixel::new(60, 80, 110, 100);

        // Wave 1 (small) — always shown if not muted
        let c1 = if level > 10 { wave_color } else { dim_color };
        fb.draw_line_aa(cx + 2, cy - 2, cx + 3, cy, c1);
        fb.draw_line_aa(cx + 3, cy, cx + 2, cy + 2, c1);

        // Wave 2 (medium)
        let c2 = if level > 40 { wave_color } else { dim_color };
        fb.draw_line_aa(cx + 4, cy - 4, cx + 6, cy, c2);
        fb.draw_line_aa(cx + 6, cy, cx + 4, cy + 4, c2);

        // Wave 3 (large)
        let c3 = if level > 70 { wave_color } else { dim_color };
        fb.draw_line_aa(cx + 6, cy - 5, cx + 8, cy, c3);
        fb.draw_line_aa(cx + 8, cy, cx + 6, cy + 5, c3);
    }
}

/// Battery icon — horizontal battery with fill level and optional charging bolt
fn draw_battery_icon(fb: &mut FrameBuffer, cx: i32, cy: i32) {
    let percent = BATTERY_PERCENT.load(Ordering::Relaxed);
    let charging = BATTERY_CHARGING.load(Ordering::Relaxed);

    // Battery outline (horizontal pill)
    let bw: u32 = 14;
    let bh: u32 = 8;
    let bx = cx - bw as i32 / 2;
    let by = cy - bh as i32 / 2;

    // Outline
    let outline_color = Pixel::new(140, 160, 200, 200);
    fb.draw_rounded_rect(Rect::new(bx, by, bw, bh), outline_color, 2, 1);

    // Positive terminal nub on right side
    fb.fill_rounded_rect_aa(Rect::new(bx + bw as i32, cy - 2, 2, 4), outline_color, 1);

    // Fill level
    let fill_w = ((bw - 4) * percent as u32 / 100).max(1);
    let fill_color = if percent <= 15 {
        Pixel::new(255, 70, 85, 230) // Red — critical
    } else if percent <= 30 {
        Pixel::new(255, 180, 50, 230) // Amber — low
    } else {
        Pixel::new(0, 220, 140, 230) // Green — good
    };

    fb.fill_rounded_rect_aa(Rect::new(bx + 2, by + 2, fill_w, bh - 4), fill_color, 1);

    // Charging bolt overlay (⚡)
    if charging {
        let bolt_color = Pixel::new(255, 220, 50, 255);
        // Simple lightning bolt shape
        fb.draw_line_aa(cx + 1, cy - 3, cx - 1, cy, bolt_color);
        fb.draw_line_aa(cx - 1, cy, cx + 1, cy, bolt_color);
        fb.draw_line_aa(cx + 1, cy, cx - 1, cy + 3, bolt_color);
    }
}

/// Bluetooth icon — "B" rune shape
fn draw_bluetooth_icon(fb: &mut FrameBuffer, cx: i32, cy: i32) {
    let enabled = BT_ENABLED.load(Ordering::Relaxed);
    let connected = BT_CONNECTED.load(Ordering::Relaxed);

    if !enabled {
        // Disabled — gray X
        let gray = Pixel::new(100, 110, 130, 180);
        fb.draw_line_aa(cx - 4, cy - 5, cx + 4, cy + 5, gray);
        fb.draw_line_aa(cx + 4, cy - 5, cx - 4, cy + 5, gray);
        return;
    }

    let color = if connected {
        Pixel::new(80, 130, 255, 240) // Bright blue when connected
    } else {
        Pixel::new(80, 130, 255, 140) // Dim blue when on but not connected
    };

    // Bluetooth rune: vertical line with two arrow-like chevrons
    // Vertical center line
    fb.draw_line_aa(cx, cy - 7, cx, cy + 7, color);
    // Top-right arrow: from center-top diag right, then diag back left
    fb.draw_line_aa(cx, cy - 7, cx + 4, cy - 3, color);
    fb.draw_line_aa(cx + 4, cy - 3, cx - 3, cy + 3, color);
    // Bottom-right arrow: from center-bottom diag right, then diag back left
    fb.draw_line_aa(cx, cy + 7, cx + 4, cy + 3, color);
    fb.draw_line_aa(cx + 4, cy + 3, cx - 3, cy - 3, color);

    // Connected indicator: small dot
    if connected {
        fb.fill_circle_aa(cx + 6, cy, 2, Pixel::new(100, 255, 100, 200));
    }
}

/// Notification bell icon with unread count badge
fn draw_notification_icon(fb: &mut FrameBuffer, cx: i32, cy: i32) {
    let unread = super::notifications::unread_count();

    let bell_color = if unread > 0 {
        Pixel::new(0, 220, 255, 240) // Bright cyan when there are notifications
    } else {
        Pixel::new(160, 180, 210, 200) // Muted when no notifications
    };

    // Bell body (rounded trapezoid shape)
    // Top: small circle (bell dome)
    fb.fill_circle_aa(cx, cy - 3, 2, bell_color);

    // Bell body (widening downward)
    for row in 0i32..6 {
        let half_w = 2 + row / 2; // Widens: 2, 2, 3, 3, 4, 4
        fb.draw_hline(
            cx - half_w,
            cy - 2 + row,
            (half_w * 2 + 1) as u32,
            bell_color,
        );
    }

    // Bell rim (wider bar at bottom)
    fb.draw_hline(cx - 5, cy + 4, 11, bell_color);

    // Clapper (small dot at bottom center)
    fb.fill_circle_aa(cx, cy + 6, 1, bell_color);

    // Little handle/hook at very top
    fb.fill_circle_aa(cx, cy - 6, 1, bell_color);

    // Unread badge (red circle with count)
    if unread > 0 {
        let badge_x = cx + 5;
        let badge_y = cy - 6;

        // Red badge circle
        fb.fill_circle_aa(badge_x, badge_y, 5, Pixel::new(255, 60, 80, 250));
        // Glow
        fb.fill_circle_aa(badge_x, badge_y, 7, Pixel::new(255, 60, 80, 40));

        // Count text (single digit or "9+")
        if unread < 10 {
            let mut buf = [0u8; 4];
            let ch = char::from_digit(unread, 10).unwrap_or('0');
            let s = ch.encode_utf8(&mut buf);
            fonts::draw_string_bold_compact(
                fb,
                badge_x - 3,
                badge_y - 5,
                s,
                Pixel::new(255, 255, 255, 255),
                1,
            );
        } else {
            fonts::draw_string_bold_compact(
                fb,
                badge_x - 5,
                badge_y - 5,
                "9+",
                Pixel::new(255, 255, 255, 255),
                1,
            );
        }
    }
}

/// Initialize the system tray (set default states)
pub fn init() {
    // Defaults are set via atomics above
    crate::serial_println!("[KnoxOS] System tray initialized");
}
