use alloc::format;
/// Calendar App — Full-window calendar with month grid navigation
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

struct CalendarState {
    window_id: WindowId,
    /// Currently displayed year
    year: u16,
    /// Currently displayed month (1-12)
    month: u8,
    /// Today's day (for highlighting)
    today_day: u8,
    today_month: u8,
    today_year: u16,
}

lazy_static! {
    static ref STATES: Mutex<Vec<CalendarState>> = Mutex::new(Vec::new());
}

const CELL_W: i32 = 48;
const CELL_H: i32 = 40;
const HEADER_H: i32 = 60;
const DOW_H: i32 = 24;

// ─── Public API ─────────────────────────────────────────────────────

pub fn open() {
    let dt = crate::rtc::read_rtc();
    let mut win = window::Window::new("Calendar", 200, 80, 370, 400);
    win.content_type = WindowContentType::CalendarApp;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    let mut states = STATES.lock();
    states.push(CalendarState {
        window_id: wid,
        year: dt.year,
        month: dt.month,
        today_day: dt.day,
        today_month: dt.month,
        today_year: dt.year,
    });
    drop(states);

    super::request_redraw();
}

pub fn on_window_closed(wid: WindowId) {
    let mut states = STATES.lock();
    states.retain(|s| s.window_id != wid);
}

// ─── Drawing ────────────────────────────────────────────────────────

pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, content: Rect, _scroll_y: i32) {
    let states = STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let tc = theme::colors();
    let bg = tc.bg_surface;
    fb.fill_rect(content, bg);

    let cx = content.x;
    let cy = content.y;

    // ── Header: < Month Year > ──
    let header = Rect::new(cx, cy, content.width, HEADER_H as u32);
    fb.fill_rect(header, colors::darken(bg, 12));

    let month_name = month_str(state.month);
    let mut title = String::new();
    let _ = write!(title, "{} {}", month_name, state.year);
    let title_w = fonts::measure_string_width(&title, 1);
    let title_x = cx + (content.width as i32 - title_w as i32) / 2;
    fonts::draw_string_bold(fb, title_x, cy + 18, &title, tc.text_primary, 1);

    // Navigation arrows
    let arrow_y = cy + 18;
    // Left arrow <
    let left_x = cx + 16;
    fonts::draw_string_bold(fb, left_x, arrow_y, "<", theme::accent_color(), 1);
    // Right arrow >
    let right_x = cx + content.width as i32 - 28;
    fonts::draw_string_bold(fb, right_x, arrow_y, ">", theme::accent_color(), 1);

    // ── Day-of-week header ──
    let dow_y = cy + HEADER_H;
    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    for (i, &day) in days.iter().enumerate() {
        let dx = cx + 8 + i as i32 * CELL_W;
        fonts::draw_string_compact(fb, dx + 8, dow_y + 5, day, tc.text_secondary, 1);
    }

    // ── Calendar grid ──
    let grid_y = dow_y + DOW_H;
    let first_dow = day_of_week(state.year, state.month, 1);
    let days_in = days_in_month(state.year, state.month);

    let is_current_month = state.year == state.today_year && state.month == state.today_month;

    for d in 1..=days_in {
        let cell_idx = (first_dow + d as u32 - 1) as i32;
        let col = cell_idx % 7;
        let row = cell_idx / 7;
        let cell_x = cx + 8 + col * CELL_W;
        let cell_y = grid_y + row * CELL_H;

        // Today highlight
        if is_current_month && d == state.today_day {
            let accent = theme::accent_color();
            fb.fill_circle_aa(cell_x + CELL_W / 2, cell_y + CELL_H / 2, 16, accent);
            let mut day_str = String::new();
            let _ = write!(day_str, "{}", d);
            let dw = fonts::measure_string_width_compact(&day_str, 1);
            fonts::draw_string_compact(
                fb,
                cell_x + (CELL_W - dw as i32) / 2,
                cell_y + (CELL_H - 14) / 2,
                &day_str,
                Pixel::rgb(255, 255, 255),
                1,
            );
        } else {
            let mut day_str = String::new();
            let _ = write!(day_str, "{}", d);
            let dw = fonts::measure_string_width_compact(&day_str, 1);
            // Weekend days slightly dimmed
            let text_color = if col == 0 || col == 6 {
                tc.text_secondary
            } else {
                tc.text_primary
            };
            fonts::draw_string_compact(
                fb,
                cell_x + (CELL_W - dw as i32) / 2,
                cell_y + (CELL_H - 14) / 2,
                &day_str,
                text_color,
                1,
            );
        }
    }

    // ── Current time display at bottom ──
    let dt = crate::rtc::read_rtc();
    let mut time_str = String::new();
    let _ = write!(time_str, "{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second);
    let tw = fonts::measure_string_width(&time_str, 1);
    let time_y = content.y + content.height as i32 - 30;
    fonts::draw_string(
        fb,
        cx + (content.width as i32 - tw as i32) / 2,
        time_y,
        &time_str,
        theme::accent_color(),
        1,
    );
}

// ─── Click Handling ────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let content = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w.content_rect(),
        None => return false,
    };
    drop(wm);

    let cx = content.x;
    let cy = content.y;

    // Left arrow
    let arrow_y = cy + 18;
    let left_x = cx + 16;
    if x >= left_x - 6 && x < left_x + 20 && y >= arrow_y - 4 && y < arrow_y + 22 {
        let mut states = STATES.lock();
        if let Some(s) = states.iter_mut().find(|s| s.window_id == wid) {
            if s.month == 1 {
                s.month = 12;
                s.year -= 1;
            } else {
                s.month -= 1;
            }
        }
        return true;
    }

    // Right arrow
    let right_x = cx + content.width as i32 - 28;
    if x >= right_x - 6 && x < right_x + 20 && y >= arrow_y - 4 && y < arrow_y + 22 {
        let mut states = STATES.lock();
        if let Some(s) = states.iter_mut().find(|s| s.window_id == wid) {
            if s.month == 12 {
                s.month = 1;
                s.year += 1;
            } else {
                s.month += 1;
            }
        }
        return true;
    }

    false
}

// ─── Calendar Math ─────────────────────────────────────────────────

fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 => 31,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        _ => 31,
    }
}

/// Zeller's formula — returns 0=Sun, 1=Mon, ..., 6=Sat
fn day_of_week(year: u16, month: u8, day: u8) -> u32 {
    let mut y = year as i32;
    let mut m = month as i32;
    if m < 3 {
        m += 12;
        y -= 1;
    }
    let q = day as i32;
    let k = y % 100;
    let j = y / 100;
    let h = (q + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    // Convert from Zeller (0=Sat) to 0=Sun
    ((h + 6) % 7) as u32
}

fn month_str(m: u8) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}
