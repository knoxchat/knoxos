use alloc::format;
/// Log Viewer — GUI for viewing kernel ring buffer (dmesg) messages
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use core::sync::atomic::{AtomicU8, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use super::window::{self, WindowContentType, WindowId};
use crate::dmesg::{self, LogEntry, LogLevel};

struct LogViewState {
    window_id: WindowId,
    /// Minimum level to display (0=Emergency..7=Debug)
    filter_level: u8,
    /// Last seen sequence for auto-refresh detection
    last_seq: u64,
    /// Cached entries
    entries: Vec<LogEntry>,
}

lazy_static! {
    static ref STATES: Mutex<Vec<LogViewState>> = Mutex::new(Vec::new());
}

/// Currently selected filter level (persists across views)
static FILTER_LEVEL: AtomicU8 = AtomicU8::new(7); // Debug = show all

/// Open the log viewer window
pub fn open() {
    let entries = dmesg::read_all();
    let last_seq = entries.last().map(|e| e.seq).unwrap_or(0);

    let mut win = window::Window::new("Log Viewer", 120, 80, 700, 520);
    win.content_type = WindowContentType::LogViewer;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    let state = LogViewState {
        window_id: wid,
        filter_level: FILTER_LEVEL.load(Ordering::Relaxed),
        last_seq,
        entries,
    };
    STATES.lock().push(state);
    super::request_redraw();
}

/// Draw log viewer content
pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, area: Rect, scroll_y: i32) {
    let tc = theme::colors();
    let accent = colors::accent();
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    // Refresh entries
    state.entries = dmesg::read_all();
    state.filter_level = FILTER_LEVEL.load(Ordering::Relaxed);

    let x0 = area.x;
    let y0 = area.y;
    let w = area.width as i32;

    // -- Header bar --
    fb.fill_rect(
        Rect::new(x0, y0, area.width, 36),
        tc.bg_surface.with_alpha(230),
    );

    // Title
    fonts::draw_string_bold_compact(fb, x0 + 10, y0 + 6, "Kernel Log", tc.text_primary, 1);

    // Entry count
    let filtered: Vec<&LogEntry> = state
        .entries
        .iter()
        .filter(|e| (e.level as u8) <= state.filter_level)
        .collect();
    let count_str = format!("{} entries", filtered.len());
    let cw = fonts::measure_string_width_compact(&count_str, 1) as i32;
    fonts::draw_string_compact(
        fb,
        x0 + w - cw - 10,
        y0 + 10,
        &count_str,
        tc.text_secondary,
        1,
    );

    // Filter buttons
    let filter_labels = ["ALL", "ERR", "WARN", "INFO", "DBG"];
    let filter_levels: [u8; 5] = [7, 3, 4, 6, 7]; // Debug, Error, Warning, Info, Debug
    let mut bx = x0 + 10;
    let by = y0 + 22;
    for (i, label) in filter_labels.iter().enumerate() {
        let bw = 36i32;
        let is_active = match i {
            0 => state.filter_level == 7,
            1 => state.filter_level == 3,
            2 => state.filter_level == 4,
            3 => state.filter_level == 6,
            4 => state.filter_level == 7,
            _ => false,
        };
        // Just use first 4 filters (ALL, ERR, WARN, INFO)
        if i >= 4 {
            break;
        }
        let bg = if is_active { accent } else { tc.bg_tertiary };
        let fg = if is_active {
            Pixel::rgb(255, 255, 255)
        } else {
            tc.text_secondary
        };
        fb.fill_rounded_rect_aa(Rect::new(bx, by, bw as u32, 12), bg, 3);
        fonts::draw_string_compact(fb, bx + 3, by + 2, label, fg, 1);
        bx += bw + 4;
    }

    // Refresh button
    let rbx = x0 + w - 60;
    fb.fill_rounded_rect_aa(Rect::new(rbx, by, 50, 12), tc.bg_tertiary, 3);
    fonts::draw_string_compact(fb, rbx + 4, by + 2, "Refresh", tc.text_primary, 1);

    // -- Log entries --
    let list_y = y0 + 40;
    let list_h = area.height as i32 - 40;
    let row_h = 16i32;
    let clip = Rect::new(x0, list_y, area.width, list_h as u32);

    for (i, entry) in filtered.iter().enumerate() {
        let ry = list_y + (i as i32) * row_h + scroll_y;
        if ry + row_h < clip.y || ry > clip.y + clip.height as i32 {
            continue;
        }

        // Alternating row background
        if i % 2 == 0 {
            fb.fill_rect(
                Rect::new(x0, ry, area.width, row_h as u32),
                tc.bg_tertiary.with_alpha(60),
            );
        }

        // Level indicator dot
        let dot_color = level_color(entry.level);
        fb.fill_circle_aa(x0 + 8, ry + row_h / 2, 3, dot_color);

        // Timestamp
        let secs = entry.timestamp_usec / 1_000_000;
        let usecs = (entry.timestamp_usec % 1_000_000) / 1000;
        let ts = format!("[{:>5}.{:03}]", secs, usecs);
        fonts::draw_string_compact(fb, x0 + 16, ry + 2, &ts, tc.text_secondary, 1);

        // Level prefix
        let level_str = entry.level.prefix();
        let lx = x0 + 90;
        fonts::draw_string_compact(fb, lx, ry + 2, level_str, dot_color, 1);

        // Facility
        let fx = x0 + 130;
        fonts::draw_string_compact(fb, fx, ry + 2, entry.facility, tc.text_secondary, 1);

        // Message (truncated to fit)
        let mx = x0 + 170;
        let max_msg_w = (w - 180) as u32;
        let msg = truncate_to_width(&entry.message, max_msg_w);
        fonts::draw_string_compact(fb, mx, ry + 2, msg, tc.text_primary, 1);
    }
}

/// Handle click events
pub fn handle_click(wid: WindowId, x: i32, y: i32) {
    let states = STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return,
    };
    let area = win.content_rect();
    drop(wm);

    let x0 = area.x;
    let y0 = area.y;
    let w = area.width as i32;

    // Filter buttons row (y0+22, height 12)
    let by = y0 + 22;
    if y >= by && y < by + 12 {
        let mut bx = x0 + 10;
        let bw = 36i32;
        let filter_values: [u8; 4] = [7, 3, 4, 6]; // ALL=7(Debug), ERR=3, WARN=4, INFO=6
        for &level in &filter_values {
            if x >= bx && x < bx + bw {
                FILTER_LEVEL.store(level, Ordering::Relaxed);
                drop(states);
                super::request_redraw();
                return;
            }
            bx += bw + 4;
        }
        // Refresh button
        let rbx = x0 + w - 60;
        if x >= rbx && x < rbx + 50 {
            // Just trigger redraw (entries refresh on draw)
            drop(states);
            super::request_redraw();
        }
    }
}

fn level_color(level: LogLevel) -> Pixel {
    match level {
        LogLevel::Emergency | LogLevel::Alert | LogLevel::Critical => Pixel::rgb(220, 50, 50),
        LogLevel::Error => Pixel::rgb(220, 80, 60),
        LogLevel::Warning => Pixel::rgb(220, 180, 40),
        LogLevel::Notice => Pixel::rgb(80, 180, 220),
        LogLevel::Info => Pixel::rgb(80, 200, 120),
        LogLevel::Debug => Pixel::rgb(160, 160, 160),
    }
}

fn truncate_to_width(s: &str, max_w: u32) -> &str {
    let full_w = fonts::measure_string_width_compact(s, 1);
    if full_w <= max_w {
        return s;
    }
    // Binary search for the longest prefix that fits
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > 0 {
        end -= 1;
        // Find char boundary
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        let sub = &s[..end];
        if fonts::measure_string_width_compact(sub, 1) + 12 <= max_w {
            return sub;
        }
    }
    ""
}
