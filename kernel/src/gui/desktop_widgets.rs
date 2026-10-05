/// Desktop Widgets — Single glass info panel on the desktop
/// Clock, memory, load, and uptime in one card (no stacked boxes, no corner rings).
use super::font_engine;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use alloc::string::String;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

static WIDGETS_ENABLED: AtomicBool = AtomicBool::new(true);
static CLOCK_ENABLED: AtomicBool = AtomicBool::new(true);
static STATS_ENABLED: AtomicBool = AtomicBool::new(true);
static UPTIME_ENABLED: AtomicBool = AtomicBool::new(true);

const WIDGET_MARGIN: i32 = 24;
const WIDGET_WIDTH: u32 = 248;
const PANEL_RADIUS: u32 = 16;
const PAD: i32 = 20;

pub fn set_widgets_enabled(on: bool) {
    WIDGETS_ENABLED.store(on, Ordering::Relaxed);
}

pub fn widgets_enabled() -> bool {
    WIDGETS_ENABLED.load(Ordering::Relaxed)
}

pub fn set_clock_enabled(on: bool) {
    CLOCK_ENABLED.store(on, Ordering::Relaxed);
}

pub fn set_stats_enabled(on: bool) {
    STATS_ENABLED.store(on, Ordering::Relaxed);
}

pub fn set_uptime_enabled(on: bool) {
    UPTIME_ENABLED.store(on, Ordering::Relaxed);
}

/// Bounding rect for damage tracking.
pub fn bounding_rect(screen_w: u32) -> Rect {
    let x = screen_w as i32 - WIDGET_WIDTH as i32 - WIDGET_MARGIN;
    Rect::new(x, 48, WIDGET_WIDTH + WIDGET_MARGIN as u32, 340)
}

/// Draw the desktop info panel.
pub fn draw(fb: &mut FrameBuffer) {
    if !WIDGETS_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let show_clock = CLOCK_ENABLED.load(Ordering::Relaxed);
    let show_stats = STATS_ENABLED.load(Ordering::Relaxed);
    let show_uptime = UPTIME_ENABLED.load(Ordering::Relaxed);
    if !show_clock && !show_stats && !show_uptime {
        return;
    }

    let wx = fb.width as i32 - WIDGET_WIDTH as i32 - WIDGET_MARGIN;
    let wy = 52i32;
    let mut content_h = PAD as u32;
    if show_clock {
        content_h += 86;
    }
    if show_stats {
        content_h += 92;
    }
    if show_uptime {
        content_h += 36;
    }
    content_h += PAD as u32;

    let panel = Rect::new(wx, wy, WIDGET_WIDTH, content_h);
    draw_panel_shell(fb, panel);

    let mut y = wy + PAD;
    let inner_x = wx + PAD;
    let inner_w = WIDGET_WIDTH - PAD as u32 * 2;

    if show_clock {
        y = draw_clock_block(fb, inner_x, y, inner_w);
    }
    if show_stats {
        if show_clock {
            y = draw_divider(fb, inner_x, y, inner_w);
        }
        y = draw_stats_block(fb, inner_x, y, inner_w);
    }
    if show_uptime {
        if show_clock || show_stats {
            y = draw_divider(fb, inner_x, y, inner_w);
        }
        draw_uptime_block(fb, inner_x, y, inner_w);
    }
}

fn draw_panel_shell(fb: &mut FrameBuffer, rect: Rect) {
    // Soft drop shadow (no circle-outline borders)
    for i in 0..6 {
        let a = (18 - i * 3).max(4) as u8;
        fb.fill_rounded_rect_aa(
            Rect::new(
                rect.x + i / 2,
                rect.y + 2 + i,
                rect.width,
                rect.height.saturating_sub(i as u32 / 2),
            ),
            Pixel::new(0, 0, 0, a),
            PANEL_RADIUS + i as u32,
        );
    }

    let top = Pixel::new(28, 26, 34, 210);
    let bot = Pixel::new(18, 16, 22, 200);
    fb.fill_rounded_rect_gradient_aa(rect, top, bot, PANEL_RADIUS);

    // Hairline along the top straight edge only — avoids the full-circle
    // corner rings that draw_rounded_rect produces.
    let r = PANEL_RADIUS as i32;
    fb.draw_hline(
        rect.x + r + 2,
        rect.y + 1,
        rect.width.saturating_sub((r as u32 + 2) * 2),
        Pixel::new(255, 230, 210, 28),
    );
}

fn draw_divider(fb: &mut FrameBuffer, x: i32, y: i32, w: u32) -> i32 {
    fb.draw_hline(x, y + 6, w, Pixel::new(255, 255, 255, 16));
    y + 18
}

fn muted() -> Pixel {
    Pixel::new(168, 156, 148, 220)
}

fn bright() -> Pixel {
    Pixel::new(236, 228, 220, 255)
}

fn draw_clock_block(fb: &mut FrameBuffer, x: i32, y: i32, _w: u32) -> i32 {
    let dt = crate::rtc::read_rtc();
    let accent = theme::accent_color();

    let mut hm = String::new();
    let _ = write!(hm, "{:02}:{:02}", dt.hour, dt.minute);
    font_engine::draw_ui_bold(fb, x, y, &hm, 28, accent);

    let mut sec = String::new();
    let _ = write!(sec, "{:02}", dt.second);
    let hm_w = font_engine::measure_ui_text(&hm, 28) as i32;
    font_engine::draw_ui_text(fb, x + hm_w + 8, y + 10, &sec, 13, muted());

    let day_name = match dt.day_of_week {
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        _ => "Sunday",
    };
    let month_name = match dt.month {
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
    };
    let mut date = String::new();
    let _ = write!(date, "{} {} {}", month_name, dt.day, dt.year);
    font_engine::draw_ui_text(fb, x, y + 36, day_name, 13, bright());
    font_engine::draw_ui_text(fb, x, y + 54, &date, 12, muted());

    y + 78
}

fn draw_stats_block(fb: &mut FrameBuffer, x: i32, y: i32, w: u32) -> i32 {
    let info = crate::sysinfo::get_sysinfo();
    let used = info.totalram.saturating_sub(info.freeram);
    let mem_pct = (used * 100).checked_div(info.totalram).unwrap_or(0) as u32;
    let used_mb = used / (1024 * 1024);
    let total_mb = info.totalram / (1024 * 1024);

    font_engine::draw_ui_text(fb, x, y, "Memory", 12, muted());
    let mut pct = String::new();
    let _ = write!(pct, "{}%", mem_pct.min(100));
    let pct_w = font_engine::measure_ui_text(&pct, 12) as i32;
    font_engine::draw_ui_bold(fb, x + w as i32 - pct_w, y, &pct, 12, bright());

    let bar_y = y + 22;
    let bar_h = 5u32;
    fb.fill_rounded_rect_aa(
        Rect::new(x, bar_y, w, bar_h),
        Pixel::new(255, 255, 255, 18),
        3,
    );
    let fill_w = (w as u64 * mem_pct.min(100) as u64 / 100) as u32;
    if fill_w > 1 {
        fb.fill_rounded_rect_aa(Rect::new(x, bar_y, fill_w, bar_h), theme::accent_color(), 3);
    }

    let mut mem_text = String::new();
    let _ = write!(mem_text, "{} / {} MB", used_mb, total_mb);
    font_engine::draw_ui_text(fb, x, y + 34, &mem_text, 11, muted());

    let load_1 = info.loads[0] / 65536;
    let load_frac = (info.loads[0] % 65536) * 100 / 65536;
    let mut load_text = String::new();
    let _ = write!(load_text, "{}.{:02}", load_1, load_frac);

    draw_kv_row(fb, x, y + 54, w, "Load", &load_text);

    let mut proc_text = String::new();
    let _ = write!(proc_text, "{}", info.procs);
    draw_kv_row(fb, x, y + 72, w, "Processes", &proc_text);

    y + 92
}

fn draw_kv_row(fb: &mut FrameBuffer, x: i32, y: i32, w: u32, key: &str, value: &str) {
    font_engine::draw_ui_text(fb, x, y, key, 12, muted());
    let vw = font_engine::measure_ui_text(value, 12) as i32;
    font_engine::draw_ui_bold(fb, x + w as i32 - vw, y, value, 12, bright());
}

fn draw_uptime_block(fb: &mut FrameBuffer, x: i32, y: i32, w: u32) {
    let info = crate::sysinfo::get_sysinfo();
    let secs = info.uptime as u64;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let mut up = String::new();
    if hours > 0 {
        let _ = write!(up, "{}h {}m", hours, mins);
    } else {
        let _ = write!(up, "{}m {}s", mins, secs % 60);
    }
    draw_kv_row(fb, x, y, w, "Uptime", &up);
}
