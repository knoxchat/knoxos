/// System Popups — Calendar, Volume, Brightness, and Quick Settings panels
/// These are popup panels that appear when clicking system tray icons or the clock.
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

// ═══════════════════════════════════════════════════════════════════════════
// CALENDAR POPUP — Shows when clicking the clock area
// ═══════════════════════════════════════════════════════════════════════════

/// Calendar popup state
pub struct CalendarPopup {
    pub visible: bool,
    pub year: u32,
    pub month: u32, // 1-12
    pub day: u32,   // 1-31
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

lazy_static::lazy_static! {
    pub static ref CALENDAR: Mutex<CalendarPopup> = Mutex::new(CalendarPopup {
        visible: false,
        year: 2026,
        month: 2,
        day: 19,
        hour: 12,
        minute: 0,
        second: 0,
    });
}

const CALENDAR_WIDTH: u32 = 320;
const CALENDAR_HEIGHT: u32 = 350;

/// Month names
const MONTHS: &[&str] = &[
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Day names (short)
const DAYS: &[&str] = &["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

/// Simple day-of-week calculation (Zeller's congruence simplified)
fn day_of_week(year: u32, month: u32, day: u32) -> u32 {
    let mut y = year;
    let mut m = month;
    if m < 3 {
        m += 12;
        y -= 1;
    }
    let q = day;
    let k = y % 100;
    let j = y / 100;
    let h = (q + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    // Convert: 0=Sat, 1=Sun, 2=Mon, ...
    (h + 6) % 7 // 0=Sun, 1=Mon, ..., 6=Sat
}

/// Days in a given month
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Draw the calendar popup
pub fn draw_calendar(fb: &mut FrameBuffer) {
    let cal = CALENDAR.lock();
    if !cal.visible {
        return;
    }

    let screen_w = fb.width as i32;
    let taskbar_y = fb.height as i32 - super::scale::taskbar_height() as i32;
    let panel_x = screen_w - CALENDAR_WIDTH as i32 - 8;
    let panel_y = taskbar_y - CALENDAR_HEIGHT as i32 - 4;

    // Shadow
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x + 4, panel_y + 4, CALENDAR_WIDTH, CALENDAR_HEIGHT),
        Pixel::new(0, 0, 0, 90),
        8,
    );

    // Background
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x, panel_y, CALENDAR_WIDTH, CALENDAR_HEIGHT),
        Pixel::new(33, 33, 36, 245),
        8,
    );

    // Border
    fb.draw_rounded_rect(
        Rect::new(panel_x, panel_y, CALENDAR_WIDTH, CALENDAR_HEIGHT),
        Pixel::rgb(60, 60, 64),
        8,
        1,
    );

    // ── Current time (large) ───────────────
    let time_str = {
        let h = if cal.hour == 0 {
            12
        } else if cal.hour > 12 {
            cal.hour - 12
        } else {
            cal.hour
        };
        let ampm = if cal.hour < 12 { "AM" } else { "PM" };
        alloc::format!("{}:{:02} {}", h, cal.minute, ampm)
    };
    fonts::draw_string_bold(fb, panel_x + 16, panel_y + 14, &time_str, colors::WHITE, 2);

    // ── Date string ───────────────────────
    let date_str = alloc::format!(
        "{}, {} {}, {}",
        match day_of_week(cal.year, cal.month, cal.day) {
            0 => "Sunday",
            1 => "Monday",
            2 => "Tuesday",
            3 => "Wednesday",
            4 => "Thursday",
            5 => "Friday",
            6 => "Saturday",
            _ => "Monday",
        },
        MONTHS[(cal.month - 1) as usize],
        cal.day,
        cal.year
    );
    fonts::draw_string_compact(
        fb,
        panel_x + 16,
        panel_y + 52,
        &date_str,
        Pixel::rgb(180, 180, 180),
        1,
    );

    // Separator
    fb.draw_hline(
        panel_x + 12,
        panel_y + 70,
        CALENDAR_WIDTH - 24,
        Pixel::rgb(60, 60, 64),
    );

    // ── Month/Year header ──────────────────
    let header_y = panel_y + 78;
    let month_name = MONTHS[(cal.month - 1) as usize];
    let header_text = alloc::format!("{} {}", month_name, cal.year);
    fonts::draw_string_bold_compact(fb, panel_x + 16, header_y, &header_text, colors::WHITE, 1);

    // Navigation arrows
    fonts::draw_string_bold_compact(
        fb,
        panel_x + CALENDAR_WIDTH as i32 - 40,
        header_y,
        "<  >",
        Pixel::rgb(120, 120, 120),
        1,
    );

    // ── Day headers ───────────────────────
    let days_y = header_y + 20;
    let cell_w = (CALENDAR_WIDTH - 24) / 7;
    for (i, day_name) in DAYS.iter().enumerate() {
        let dx = panel_x + 12 + (i as i32 * cell_w as i32);
        let color = if i == 0 || i == 6 {
            Pixel::rgb(247, 118, 142) // Weekend color
        } else {
            Pixel::rgb(120, 120, 120)
        };
        fonts::draw_string_compact(fb, dx + 4, days_y, day_name, color, 1);
    }

    // Separator under day headers
    fb.draw_hline(
        panel_x + 12,
        days_y + 14,
        CALENDAR_WIDTH - 24,
        Pixel::rgb(50, 50, 54),
    );

    // ── Calendar grid ──────────────────────
    let first_dow = day_of_week(cal.year, cal.month, 1) as i32;
    let total_days = days_in_month(cal.year, cal.month) as i32;
    let grid_y = days_y + 20;
    let cell_h: i32 = 24;

    for d in 1..=total_days {
        let cell_idx = first_dow + d - 1;
        let col = cell_idx % 7;
        let row = cell_idx / 7;
        let cx = panel_x + 12 + (col * cell_w as i32);
        let cy = grid_y + (row * cell_h);

        // Highlight today
        let is_today = d == cal.day as i32;
        if is_today {
            fb.fill_circle_aa(cx + cell_w as i32 / 2, cy + 6, 10, Pixel::rgb(82, 139, 255));
        }

        let mut day_str = String::new();
        write!(day_str, "{:>2}", d).ok();
        let text_color = if is_today {
            colors::WHITE
        } else if col == 0 || col == 6 {
            Pixel::rgb(247, 118, 142)
        } else {
            Pixel::rgb(200, 200, 200)
        };

        fonts::draw_string_compact(fb, cx + 4, cy + 2, &day_str, text_color, 1);
    }
}

/// Toggle calendar visibility
pub fn toggle_calendar() {
    let mut cal = CALENDAR.lock();
    cal.visible = !cal.visible;

    // Update time from RTC/ticks
    if cal.visible {
        let ticks = crate::interrupts::get_ticks();
        let total_secs = ticks / 18;
        cal.hour = ((total_secs / 3600) % 24) as u32;
        cal.minute = ((total_secs / 60) % 60) as u32;
        cal.second = (total_secs % 60) as u32;
    }
}

/// Check if calendar is visible
pub fn is_calendar_visible() -> bool {
    CALENDAR.lock().visible
}

/// Handle click on calendar popup
pub fn handle_calendar_click(x: i32, y: i32, screen_w: i32, screen_h: i32) -> bool {
    let mut cal = CALENDAR.lock();
    if !cal.visible {
        return false;
    }

    let taskbar_y = screen_h - super::scale::taskbar_height() as i32;
    let panel_x = screen_w - CALENDAR_WIDTH as i32 - 8;
    let panel_y = taskbar_y - CALENDAR_HEIGHT as i32 - 4;
    let panel_rect = Rect::new(panel_x, panel_y, CALENDAR_WIDTH, CALENDAR_HEIGHT);

    if !panel_rect.contains(x, y) {
        cal.visible = false;
        return false;
    }

    // Month navigation arrows
    let header_y = panel_y + 78;
    if y >= header_y && y <= header_y + 14 {
        // Previous month
        if x >= panel_x + CALENDAR_WIDTH as i32 - 40 && x < panel_x + CALENDAR_WIDTH as i32 - 28 {
            if cal.month == 1 {
                cal.month = 12;
                cal.year -= 1;
            } else {
                cal.month -= 1;
            }
            return true;
        }
        // Next month
        if x >= panel_x + CALENDAR_WIDTH as i32 - 20 {
            if cal.month == 12 {
                cal.month = 1;
                cal.year += 1;
            } else {
                cal.month += 1;
            }
            return true;
        }
    }

    true
}

// ═══════════════════════════════════════════════════════════════════════════
// VOLUME POPUP — Shows when clicking the volume icon in system tray
// ═══════════════════════════════════════════════════════════════════════════

pub struct VolumePopup {
    pub visible: bool,
    pub volume: u8, // 0-100
    pub muted: bool,
}

lazy_static::lazy_static! {
    pub static ref VOLUME: Mutex<VolumePopup> = Mutex::new(VolumePopup {
        visible: false,
        volume: 75,
        muted: false,
    });
}

const VOLUME_POPUP_WIDTH: u32 = 52;
const VOLUME_POPUP_HEIGHT: u32 = 200;

/// Draw the volume slider popup
pub fn draw_volume_popup(fb: &mut FrameBuffer) {
    let vol = VOLUME.lock();
    if !vol.visible {
        return;
    }

    let screen_w = fb.width as i32;
    let taskbar_y = fb.height as i32 - super::scale::taskbar_height() as i32;
    let panel_x = screen_w - 100;
    let panel_y = taskbar_y - VOLUME_POPUP_HEIGHT as i32 - 4;

    // Shadow
    fb.fill_rounded_rect_aa(
        Rect::new(
            panel_x + 3,
            panel_y + 3,
            VOLUME_POPUP_WIDTH,
            VOLUME_POPUP_HEIGHT,
        ),
        Pixel::new(0, 0, 0, 80),
        8,
    );

    // Background
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x, panel_y, VOLUME_POPUP_WIDTH, VOLUME_POPUP_HEIGHT),
        Pixel::new(38, 38, 42, 245),
        8,
    );

    // Border
    fb.draw_rounded_rect(
        Rect::new(panel_x, panel_y, VOLUME_POPUP_WIDTH, VOLUME_POPUP_HEIGHT),
        Pixel::rgb(60, 60, 64),
        8,
        1,
    );

    // Volume percentage
    let vol_str = if vol.muted {
        String::from("M")
    } else {
        alloc::format!("{}", vol.volume)
    };
    let text_x = panel_x + (VOLUME_POPUP_WIDTH as i32 - vol_str.len() as i32 * 8) / 2;
    fonts::draw_string_bold_compact(fb, text_x, panel_y + 8, &vol_str, colors::WHITE, 1);

    // Slider track
    let track_x = panel_x + VOLUME_POPUP_WIDTH as i32 / 2 - 2;
    let track_y = panel_y + 28;
    let track_h: u32 = VOLUME_POPUP_HEIGHT - 56;

    fb.fill_rounded_rect_aa(
        Rect::new(track_x, track_y, 4, track_h),
        Pixel::rgb(50, 50, 55),
        2,
    );

    // Slider fill
    let fill_h = if vol.muted {
        0
    } else {
        (track_h * vol.volume as u32) / 100
    };
    if fill_h > 0 {
        fb.fill_rounded_rect_aa(
            Rect::new(track_x, track_y + (track_h - fill_h) as i32, 4, fill_h),
            Pixel::rgb(82, 139, 255),
            2,
        );
    }

    // Slider thumb
    let thumb_y = if vol.muted {
        track_y + track_h as i32
    } else {
        track_y + ((track_h as i32 * (100 - vol.volume as i32)) / 100)
    };
    fb.fill_circle_aa(
        panel_x + VOLUME_POPUP_WIDTH as i32 / 2,
        thumb_y,
        6,
        colors::WHITE,
    );

    // Speaker icon at bottom
    let icon_y = panel_y + VOLUME_POPUP_HEIGHT as i32 - 22;
    let icon_x = panel_x + VOLUME_POPUP_WIDTH as i32 / 2 - 8;
    let icon_color = if vol.muted {
        Pixel::rgb(247, 118, 142)
    } else {
        Pixel::rgb(180, 180, 180)
    };
    fb.fill_rounded_rect_aa(Rect::new(icon_x, icon_y + 3, 4, 6), icon_color, 1);
    fb.fill_rounded_rect_aa(Rect::new(icon_x + 4, icon_y + 1, 3, 10), icon_color, 1);
    if !vol.muted {
        fb.draw_circle_aa(icon_x + 10, icon_y + 6, 3, icon_color);
    } else {
        // Mute X
        fb.draw_line_aa(
            icon_x + 9,
            icon_y + 2,
            icon_x + 14,
            icon_y + 10,
            Pixel::rgb(247, 118, 142),
        );
        fb.draw_line_aa(
            icon_x + 14,
            icon_y + 2,
            icon_x + 9,
            icon_y + 10,
            Pixel::rgb(247, 118, 142),
        );
    }
}

/// Toggle volume popup
pub fn toggle_volume() {
    let mut vol = VOLUME.lock();
    vol.visible = !vol.visible;
}

/// Handle click on volume popup
pub fn handle_volume_click(x: i32, y: i32, screen_w: i32, screen_h: i32) -> bool {
    let mut vol = VOLUME.lock();
    if !vol.visible {
        return false;
    }

    let taskbar_y = screen_h - super::scale::taskbar_height() as i32;
    let panel_x = screen_w - 100;
    let panel_y = taskbar_y - VOLUME_POPUP_HEIGHT as i32 - 4;
    let panel_rect = Rect::new(panel_x, panel_y, VOLUME_POPUP_WIDTH, VOLUME_POPUP_HEIGHT);

    if !panel_rect.contains(x, y) {
        vol.visible = false;
        return false;
    }

    // Click on slider track area
    let track_y = panel_y + 28;
    let track_h = (VOLUME_POPUP_HEIGHT - 56) as i32;
    if y >= track_y && y <= track_y + track_h {
        let relative_y = y - track_y;
        let new_vol = (100 - (relative_y * 100 / track_h)).clamp(0, 100) as u8;
        vol.volume = new_vol;
        vol.muted = false;
        return true;
    }

    // Click on mute icon
    let icon_y = panel_y + VOLUME_POPUP_HEIGHT as i32 - 22;
    if y >= icon_y {
        vol.muted = !vol.muted;
        return true;
    }

    true
}

// ═══════════════════════════════════════════════════════════════════════════
// QUICK SETTINGS — Combined panel with WiFi, Bluetooth, brightness, etc.
// ═══════════════════════════════════════════════════════════════════════════

pub struct QuickSettings {
    pub visible: bool,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub night_light: bool,
    pub airplane_mode: bool,
    pub brightness: u8, // 0-100
}

lazy_static::lazy_static! {
    pub static ref QUICK_SETTINGS: Mutex<QuickSettings> = Mutex::new(QuickSettings {
        visible: false,
        wifi_enabled: true,
        bluetooth_enabled: false,
        night_light: false,
        airplane_mode: false,
        brightness: 80,
    });
}

const QS_WIDTH: u32 = 380;
const QS_HEIGHT: u32 = 320;

/// Draw the quick settings panel
pub fn draw_quick_settings(fb: &mut FrameBuffer) {
    let qs = QUICK_SETTINGS.lock();
    if !qs.visible {
        return;
    }

    let screen_w = fb.width as i32;
    let taskbar_y = fb.height as i32 - super::scale::taskbar_height() as i32;
    let panel_x = screen_w - QS_WIDTH as i32 - 8;
    let panel_y = taskbar_y - QS_HEIGHT as i32 - 4;

    // Shadow
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x + 4, panel_y + 4, QS_WIDTH, QS_HEIGHT),
        Pixel::new(0, 0, 0, 90),
        8,
    );

    // Background
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x, panel_y, QS_WIDTH, QS_HEIGHT),
        Pixel::new(33, 33, 36, 245),
        8,
    );

    // Border
    fb.draw_rounded_rect(
        Rect::new(panel_x, panel_y, QS_WIDTH, QS_HEIGHT),
        Pixel::rgb(60, 60, 64),
        8,
        1,
    );

    // ── Toggle buttons (2×2 grid) ──────────
    let tile_w: u32 = 138;
    let tile_h: u32 = 50;
    let tile_gap: i32 = 8;
    let grid_x = panel_x + 12;
    let grid_y = panel_y + 12;

    struct ToggleTile {
        label: &'static str,
        enabled: bool,
        icon_char: char,
    }

    let tiles = [
        ToggleTile {
            label: "Wi-Fi",
            enabled: qs.wifi_enabled,
            icon_char: 'W',
        },
        ToggleTile {
            label: "Bluetooth",
            enabled: qs.bluetooth_enabled,
            icon_char: 'B',
        },
        ToggleTile {
            label: "Night Light",
            enabled: qs.night_light,
            icon_char: 'N',
        },
        ToggleTile {
            label: "Airplane",
            enabled: qs.airplane_mode,
            icon_char: 'A',
        },
    ];

    for (i, tile) in tiles.iter().enumerate() {
        let col = i % 2;
        let row = i / 2;
        let tx = grid_x + (col as i32 * (tile_w as i32 + tile_gap));
        let ty = grid_y + (row as i32 * (tile_h as i32 + tile_gap));

        let bg = if tile.enabled {
            Pixel::rgb(82, 139, 255)
        } else {
            Pixel::rgb(55, 55, 60)
        };

        fb.fill_rounded_rect_aa(Rect::new(tx, ty, tile_w, tile_h), bg, 6);

        // Icon circle
        let icon_bg = if tile.enabled {
            Pixel::rgb(60, 110, 210)
        } else {
            Pixel::rgb(70, 70, 75)
        };
        fb.fill_circle_aa(tx + 20, ty + tile_h as i32 / 2, 12, icon_bg);
        fonts::draw_char_bold_compact(
            fb,
            tx + 16,
            ty + tile_h as i32 / 2 - 6,
            tile.icon_char,
            colors::WHITE,
            1,
        );

        // Label
        fonts::draw_string_bold_compact(
            fb,
            tx + 38,
            ty + (tile_h as i32 - 12) / 2,
            tile.label,
            colors::WHITE,
            1,
        );
    }

    // ── Brightness slider ──────────────────
    let slider_y = grid_y + 2 * (tile_h as i32 + tile_gap) + 12;
    fonts::draw_string_bold_compact(
        fb,
        grid_x,
        slider_y,
        "Brightness",
        Pixel::rgb(180, 180, 180),
        1,
    );

    let bar_y = slider_y + 18;
    let bar_w = QS_WIDTH - 28;
    fb.fill_rounded_rect_aa(
        Rect::new(grid_x, bar_y, bar_w, 6),
        Pixel::rgb(50, 50, 55),
        3,
    );
    let fill_w = (bar_w * qs.brightness as u32) / 100;
    if fill_w > 0 {
        fb.fill_rounded_rect_aa(
            Rect::new(grid_x, bar_y, fill_w, 6),
            Pixel::rgb(224, 175, 104),
            3,
        );
    }
    // Thumb
    fb.fill_circle_aa(
        grid_x + fill_w as i32,
        bar_y + 3,
        8,
        Pixel::rgb(224, 175, 104),
    );
    fb.fill_circle_aa(grid_x + fill_w as i32, bar_y + 3, 5, colors::WHITE);

    // ── Volume slider ──────────────────────
    let vol_y = bar_y + 26;
    fonts::draw_string_bold_compact(fb, grid_x, vol_y, "Volume", Pixel::rgb(180, 180, 180), 1);

    let vol_bar_y = vol_y + 18;
    let vol = VOLUME.lock();
    fb.fill_rounded_rect_aa(
        Rect::new(grid_x, vol_bar_y, bar_w, 6),
        Pixel::rgb(50, 50, 55),
        3,
    );
    let vol_fill = (bar_w * vol.volume as u32) / 100;
    if vol_fill > 0 && !vol.muted {
        fb.fill_rounded_rect_aa(
            Rect::new(grid_x, vol_bar_y, vol_fill, 6),
            Pixel::rgb(82, 139, 255),
            3,
        );
    }
    let vol_thumb_color = if vol.muted {
        Pixel::rgb(247, 118, 142)
    } else {
        Pixel::rgb(82, 139, 255)
    };
    let vol_thumb_x = if vol.muted {
        grid_x
    } else {
        grid_x + vol_fill as i32
    };
    fb.fill_circle_aa(vol_thumb_x, vol_bar_y + 3, 8, vol_thumb_color);
    fb.fill_circle_aa(vol_thumb_x, vol_bar_y + 3, 5, colors::WHITE);
    drop(vol);

    // ── Bottom info bar ────────────────────
    let info_y = panel_y + QS_HEIGHT as i32 - 32;
    fb.draw_hline(panel_x + 12, info_y, QS_WIDTH - 24, Pixel::rgb(55, 55, 60));
    fonts::draw_string_bold_compact(
        fb,
        panel_x + 16,
        info_y + 10,
        "KnoxOS v0.1.0",
        Pixel::rgb(100, 100, 100),
        1,
    );

    // Battery indicator
    fonts::draw_string_bold_compact(
        fb,
        panel_x + QS_WIDTH as i32 - 60,
        info_y + 10,
        "100%",
        Pixel::rgb(158, 206, 106),
        1,
    );
    // Battery icon
    let bat_x = panel_x + QS_WIDTH as i32 - 80;
    fb.fill_rounded_rect_aa(
        Rect::new(bat_x, info_y + 12, 14, 8),
        Pixel::rgb(158, 206, 106),
        2,
    );
    fb.draw_rounded_rect(
        Rect::new(bat_x, info_y + 12, 14, 8),
        Pixel::rgb(200, 200, 200),
        2,
        1,
    );
    fb.fill_rounded_rect_aa(
        Rect::new(bat_x + 14, info_y + 14, 2, 4),
        Pixel::rgb(200, 200, 200),
        1,
    );
}

/// Toggle quick settings panel
pub fn toggle_quick_settings() {
    let mut qs = QUICK_SETTINGS.lock();
    qs.visible = !qs.visible;
}

/// Handle click on quick settings panel
pub fn handle_quick_settings_click(x: i32, y: i32, screen_w: i32, screen_h: i32) -> bool {
    let mut qs = QUICK_SETTINGS.lock();
    if !qs.visible {
        return false;
    }

    let taskbar_y = screen_h - super::scale::taskbar_height() as i32;
    let panel_x = screen_w - QS_WIDTH as i32 - 8;
    let panel_y = taskbar_y - QS_HEIGHT as i32 - 4;
    let panel_rect = Rect::new(panel_x, panel_y, QS_WIDTH, QS_HEIGHT);

    if !panel_rect.contains(x, y) {
        qs.visible = false;
        return false;
    }

    let tile_w: i32 = 138;
    let tile_h: i32 = 50;
    let tile_gap: i32 = 8;
    let grid_x = panel_x + 12;
    let grid_y = panel_y + 12;

    // Check toggle tile clicks
    for i in 0..4 {
        let col = i % 2;
        let row = i / 2;
        let tx = grid_x + (col * (tile_w + tile_gap));
        let ty = grid_y + (row * (tile_h + tile_gap));
        let tile_rect = Rect::new(tx, ty, tile_w as u32, tile_h as u32);

        if tile_rect.contains(x, y) {
            match i {
                0 => qs.wifi_enabled = !qs.wifi_enabled,
                1 => qs.bluetooth_enabled = !qs.bluetooth_enabled,
                2 => qs.night_light = !qs.night_light,
                3 => qs.airplane_mode = !qs.airplane_mode,
                _ => {}
            }
            return true;
        }
    }

    // Brightness slider click
    let slider_y = grid_y + 2 * (tile_h + tile_gap) + 12 + 18;
    let bar_w = (QS_WIDTH - 28) as i32;
    if y >= slider_y - 8 && y <= slider_y + 14 && x >= grid_x && x <= grid_x + bar_w {
        let new_brightness = ((x - grid_x) * 100 / bar_w).clamp(0, 100) as u8;
        qs.brightness = new_brightness;
        return true;
    }

    // Volume slider click
    let vol_slider_y = slider_y + 44;
    if y >= vol_slider_y - 8 && y <= vol_slider_y + 14 && x >= grid_x && x <= grid_x + bar_w {
        let new_vol = ((x - grid_x) * 100 / bar_w).clamp(0, 100) as u8;
        drop(qs);
        let mut vol = VOLUME.lock();
        vol.volume = new_vol;
        vol.muted = false;
        return true;
    }

    true
}

/// Close all popups (when clicking elsewhere)
pub fn close_all_popups() {
    CALENDAR.lock().visible = false;
    VOLUME.lock().visible = false;
    QUICK_SETTINGS.lock().visible = false;
}

/// Check if any popup is open
pub fn any_popup_open() -> bool {
    CALENDAR.lock().visible || VOLUME.lock().visible || QUICK_SETTINGS.lock().visible
}
