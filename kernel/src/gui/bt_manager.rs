use alloc::format;
/// Bluetooth Manager — GUI for viewing/managing Bluetooth devices
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use super::window::{self, WindowContentType, WindowId};

struct BtManagerState {
    window_id: WindowId,
    selected: i32,
    scanning: bool,
}

lazy_static! {
    static ref STATES: Mutex<Vec<BtManagerState>> = Mutex::new(Vec::new());
}

const ROW_HEIGHT: i32 = 36;
const HEADER_HEIGHT: i32 = 56;

// ─── Public API ─────────────────────────────────────────────────────

pub fn open() {
    let mut win = window::Window::new("Bluetooth", 220, 100, 500, 420);
    win.content_type = WindowContentType::BluetoothManager;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    let mut states = STATES.lock();
    states.push(BtManagerState {
        window_id: wid,
        selected: -1,
        scanning: false,
    });
    drop(states);

    super::request_redraw();
}

pub fn on_window_closed(wid: WindowId) {
    let mut states = STATES.lock();
    states.retain(|s| s.window_id != wid);
}

// ─── Drawing ────────────────────────────────────────────────────────

pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, content: Rect, scroll_y: i32) {
    let states = STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let tc = theme::colors();
    let bg = tc.bg_surface;
    fb.fill_rect(content, bg);

    let bt_enabled = super::popups::QUICK_SETTINGS.lock().bluetooth_enabled;

    // Header with toggle
    let header = Rect::new(content.x, content.y, content.width, HEADER_HEIGHT as u32);
    fb.fill_rect(header, colors::darken(bg, 15));

    // Bluetooth icon
    let icon_x = content.x + 16;
    let icon_y = content.y + 12;
    let bt_color = if bt_enabled {
        theme::accent_color()
    } else {
        Pixel::rgb(120, 120, 120)
    };
    fb.fill_rounded_rect_aa(
        Rect::new(icon_x, icon_y, 28, 28),
        bt_color.with_alpha(40),
        6,
    );
    // Simple BT symbol
    fb.fill_rect(Rect::new(icon_x + 13, icon_y + 4, 2, 20), bt_color);
    fb.draw_line_aa(icon_x + 8, icon_y + 8, icon_x + 18, icon_y + 18, bt_color);
    fb.draw_line_aa(icon_x + 8, icon_y + 18, icon_x + 18, icon_y + 8, bt_color);

    let status = if bt_enabled {
        "Bluetooth On"
    } else {
        "Bluetooth Off"
    };
    fonts::draw_string_bold_compact(fb, icon_x + 36, content.y + 12, status, tc.text_primary, 1);

    let sub = if bt_enabled {
        "Discoverable as \"KnoxOS\""
    } else {
        "Turn on to connect devices"
    };
    fonts::draw_string_compact(fb, icon_x + 36, content.y + 30, sub, tc.text_secondary, 1);

    // Toggle button
    let toggle_x = content.x + content.width as i32 - 56;
    let toggle_y = content.y + 18;
    draw_toggle(fb, toggle_x, toggle_y, bt_enabled);

    // Scan button (only enabled if BT is on)
    let scan_x = content.x + content.width as i32 - 120;
    let scan_y = content.y + 16;
    if bt_enabled {
        let scan_rect = Rect::new(scan_x, scan_y, 56, 24);
        let label = if state.scanning { "Stop" } else { "Scan" };
        let btn_bg = if state.scanning {
            Pixel::rgb(180, 60, 60)
        } else {
            theme::accent_color()
        };
        fb.fill_rounded_rect_aa(scan_rect, btn_bg, 4);
        fonts::draw_string_centered_compact(
            fb,
            scan_x,
            scan_y,
            56,
            24,
            label,
            Pixel::rgb(255, 255, 255),
            1,
        );
    }

    if !bt_enabled {
        // Show "disabled" message
        let msg_y = content.y + HEADER_HEIGHT + 60;
        fonts::draw_string_centered_compact(
            fb,
            content.x,
            msg_y,
            content.width,
            20,
            "Bluetooth is turned off",
            tc.text_secondary,
            1,
        );
        return;
    }

    // Device list
    let devices = crate::bluetooth::get_discovered_devices();
    let list_y = content.y + HEADER_HEIGHT;

    // Section: Paired devices
    fonts::draw_string_bold_compact(
        fb,
        content.x + 16,
        list_y + 6,
        "Devices",
        tc.text_secondary,
        1,
    );

    let row_start_y = list_y + 26;
    let visible_h = content.height as i32 - HEADER_HEIGHT - 26;

    if devices.is_empty() {
        fonts::draw_string_compact(
            fb,
            content.x + 16,
            row_start_y + 10,
            "No devices found. Tap Scan to search.",
            tc.text_secondary,
            1,
        );
    }

    for (i, dev) in devices.iter().enumerate() {
        let ey = row_start_y + (i as i32 * ROW_HEIGHT) - scroll_y;
        if ey + ROW_HEIGHT < row_start_y || ey > content.y + content.height as i32 {
            continue;
        }

        // Selection highlight
        if i as i32 == state.selected {
            fb.fill_rect(
                Rect::new(content.x, ey, content.width, ROW_HEIGHT as u32),
                theme::accent_color().with_alpha(50),
            );
        } else if i % 2 == 1 {
            fb.fill_rect(
                Rect::new(content.x, ey, content.width, ROW_HEIGHT as u32),
                colors::lighten(bg, 3),
            );
        }

        // Device icon (headphones/speaker/generic)
        let dev_icon_x = content.x + 16;
        let dev_icon_y = ey + 6;
        let ic = if dev.connected {
            theme::accent_color()
        } else {
            Pixel::rgb(140, 140, 160)
        };
        fb.fill_circle_aa(dev_icon_x + 10, dev_icon_y + 10, 10, ic.with_alpha(40));
        fb.fill_circle_aa(dev_icon_x + 10, dev_icon_y + 10, 5, ic);

        // Device name
        let name = if dev.name.is_empty() {
            format!("{}", dev.address)
        } else {
            dev.name.clone()
        };
        fonts::draw_string_compact(fb, content.x + 42, ey + 4, &name, tc.text_primary, 1);

        // Status text
        let status_text = if dev.connected {
            "Connected"
        } else if dev.paired {
            "Paired"
        } else {
            "Available"
        };
        fonts::draw_string_compact(
            fb,
            content.x + 42,
            ey + 19,
            status_text,
            tc.text_secondary,
            1,
        );

        // RSSI (signal strength)
        let rssi_x = content.x + content.width as i32 - 60;
        let mut rssi_str = String::new();
        let _ = write!(rssi_str, "{} dBm", dev.rssi);
        fonts::draw_string_compact(fb, rssi_x, ey + 10, &rssi_str, tc.text_secondary, 1);
    }

    // Update max scroll
    let total_h = devices.len() as i32 * ROW_HEIGHT + 26;
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.max_scroll_y = (total_h - visible_h).max(0);
    }
}

// ─── Click Handling ────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let content = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w.content_rect(),
        None => return false,
    };
    let scroll_y = wm
        .windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.scroll_y)
        .unwrap_or(0);
    drop(wm);

    // Toggle button hit test
    let toggle_x = content.x + content.width as i32 - 56;
    let toggle_y = content.y + 18;
    if x >= toggle_x && x < toggle_x + 44 && y >= toggle_y && y < toggle_y + 22 {
        let mut qs = super::popups::QUICK_SETTINGS.lock();
        qs.bluetooth_enabled = !qs.bluetooth_enabled;
        return true;
    }

    let bt_enabled = super::popups::QUICK_SETTINGS.lock().bluetooth_enabled;
    if !bt_enabled {
        return false;
    }

    // Scan button
    let scan_x = content.x + content.width as i32 - 120;
    let scan_y = content.y + 16;
    if x >= scan_x && x < scan_x + 56 && y >= scan_y && y < scan_y + 24 {
        let mut states = STATES.lock();
        if let Some(s) = states.iter_mut().find(|s| s.window_id == wid) {
            s.scanning = !s.scanning;
        }
        return true;
    }

    // Row selection
    let list_y = content.y + HEADER_HEIGHT + 26;
    if y >= list_y {
        let row = (y - list_y + scroll_y) / ROW_HEIGHT;
        let devices = crate::bluetooth::get_discovered_devices();
        if row >= 0 && (row as usize) < devices.len() {
            let mut states = STATES.lock();
            if let Some(s) = states.iter_mut().find(|s| s.window_id == wid) {
                s.selected = row;
            }
            return true;
        }
    }

    false
}

// ─── Helpers ───────────────────────────────────────────────────────

fn draw_toggle(fb: &mut FrameBuffer, x: i32, y: i32, on: bool) {
    let w = 44u32;
    let h = 22u32;
    let track = Rect::new(x, y, w, h);
    let bg = if on {
        theme::accent_color()
    } else {
        Pixel::rgb(80, 80, 80)
    };
    fb.fill_rounded_rect_aa(track, bg, 11);

    // Knob
    let knob_x = if on { x + w as i32 - 20 } else { x + 2 };
    fb.fill_circle_aa(knob_x + 9, y + 11, 8, Pixel::rgb(255, 255, 255));
}
