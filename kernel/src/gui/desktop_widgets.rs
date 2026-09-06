/// Desktop Widgets — Semi-transparent overlay widgets on the desktop surface
/// Renders clock, system stats, and uptime directly on the wallpaper layer.
use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use alloc::string::String;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

// ─── Widget visibility toggles ──────────────────────────────────────
static WIDGETS_ENABLED: AtomicBool = AtomicBool::new(true);
static CLOCK_ENABLED: AtomicBool = AtomicBool::new(true);
static STATS_ENABLED: AtomicBool = AtomicBool::new(true);
static UPTIME_ENABLED: AtomicBool = AtomicBool::new(true);

// Widget column position (right margin from screen edge)
const WIDGET_MARGIN: i32 = 20;
const WIDGET_WIDTH: u32 = 220;
const WIDGET_GAP: i32 = 12;
const WIDGET_RADIUS: u32 = 10;

/// Glass background color — semi-transparent dark
fn glass_bg() -> Pixel {
    let tc = theme::colors();
    Pixel::new(
        tc.bg_surface.r / 2,
        tc.bg_surface.g / 2,
        tc.bg_surface.b / 2,
        160,
    )
}

/// Glass border color
fn glass_border() -> Pixel {
    Pixel::new(255, 255, 255, 30)
}

/// Label color (dimmed)
fn label_color() -> Pixel {
    let tc = theme::colors();
    Pixel::new(
        tc.text_secondary.r,
        tc.text_secondary.g,
        tc.text_secondary.b,
        200,
    )
}

/// Value color (bright)
fn value_color() -> Pixel {
    let tc = theme::colors();
    tc.text_primary
}

// ─── Public API ─────────────────────────────────────────────────────

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

/// Returns the bounding rect that contains all visible widgets.
/// Used for damage tracking — only redraw if damage overlaps this area.
pub fn bounding_rect(screen_w: u32) -> Rect {
    let x = screen_w as i32 - WIDGET_WIDTH as i32 - WIDGET_MARGIN;
    // Top margin 60 (below any top bar), enough height for 3 widgets
    Rect::new(x, 60, WIDGET_WIDTH + WIDGET_MARGIN as u32, 400)
}

/// Draw all enabled desktop widgets onto the framebuffer.
/// Called from desktop.rs after wallpaper restore, before icons.
pub fn draw(fb: &mut FrameBuffer) {
    if !WIDGETS_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let screen_w = fb.width as u32;
    let wx = screen_w as i32 - WIDGET_WIDTH as i32 - WIDGET_MARGIN;
    let mut y = 60i32;

    if CLOCK_ENABLED.load(Ordering::Relaxed) {
        let h = draw_clock_widget(fb, wx, y);
        y += h as i32 + WIDGET_GAP;
    }

    if STATS_ENABLED.load(Ordering::Relaxed) {
        let h = draw_stats_widget(fb, wx, y);
        y += h as i32 + WIDGET_GAP;
    }

    if UPTIME_ENABLED.load(Ordering::Relaxed) {
        draw_uptime_widget(fb, wx, y);
    }
}

// ─── Clock Widget ───────────────────────────────────────────────────

fn draw_clock_widget(fb: &mut FrameBuffer, x: i32, y: i32) -> u32 {
    let height = 90u32;
    let rect = Rect::new(x, y, WIDGET_WIDTH, height);

    // Glass background
    fb.fill_rounded_rect_aa(rect, glass_bg(), WIDGET_RADIUS);
    fb.draw_rounded_rect(rect, glass_border(), WIDGET_RADIUS, 1);

    let dt = crate::rtc::read_rtc();

    // Large time — HH:MM
    let mut time_buf = String::new();
    let _ = write!(time_buf, "{:02}:{:02}", dt.hour, dt.minute);
    // Draw at scale 2 for large time display
    let accent = theme::accent_color();
    fonts::draw_string_bold(fb, x + 16, y + 12, &time_buf, accent, 2);

    // Seconds — smaller, dimmed
    let mut sec_buf = String::new();
    let _ = write!(sec_buf, ":{:02}", dt.second);
    fonts::draw_string(
        fb,
        x + 16 + fonts::measure_string_width(&time_buf, 2) as i32,
        y + 22,
        &sec_buf,
        label_color(),
        1,
    );

    // Date line
    let mut date_buf = String::new();
    let day_name = match dt.day_of_week {
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "Sun",
    };
    let month_name = match dt.month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    };
    let _ = write!(
        date_buf,
        "{}, {} {} {}",
        day_name, month_name, dt.day, dt.year
    );
    fonts::draw_string_compact(fb, x + 16, y + 60, &date_buf, value_color(), 1);

    height
}

// ─── System Stats Widget ────────────────────────────────────────────

fn draw_stats_widget(fb: &mut FrameBuffer, x: i32, y: i32) -> u32 {
    let height = 110u32;
    let rect = Rect::new(x, y, WIDGET_WIDTH, height);

    fb.fill_rounded_rect_aa(rect, glass_bg(), WIDGET_RADIUS);
    fb.draw_rounded_rect(rect, glass_border(), WIDGET_RADIUS, 1);

    let info = crate::sysinfo::get_sysinfo();

    // Title
    fonts::draw_string_bold_compact(fb, x + 16, y + 10, "System", value_color(), 1);

    // Memory bar
    let used = info.totalram.saturating_sub(info.freeram);
    let mem_pct = (used * 100).checked_div(info.totalram).unwrap_or(0) as u32;
    draw_stat_bar(fb, x + 16, y + 32, "Memory", mem_pct);

    // Memory text (MB)
    let used_mb = used / (1024 * 1024);
    let total_mb = info.totalram / (1024 * 1024);
    let mut mem_text = String::new();
    let _ = write!(mem_text, "{} / {} MB", used_mb, total_mb);
    fonts::draw_string_compact(fb, x + 16, y + 52, &mem_text, label_color(), 1);

    // Load average
    let load_1 = info.loads[0] / 65536;
    let load_frac = (info.loads[0] % 65536) * 100 / 65536;
    let mut load_text = String::new();
    let _ = write!(load_text, "Load: {}.{:02}", load_1, load_frac);
    fonts::draw_string_compact(fb, x + 16, y + 72, &load_text, label_color(), 1);

    // Processes
    let mut proc_text = String::new();
    let _ = write!(proc_text, "Processes: {}", info.procs);
    fonts::draw_string_compact(fb, x + 16, y + 90, &proc_text, label_color(), 1);

    height
}

/// Draw a labeled progress bar
fn draw_stat_bar(fb: &mut FrameBuffer, x: i32, y: i32, _label: &str, percent: u32) {
    let bar_w = WIDGET_WIDTH - 32;
    let bar_h = 6u32;
    let bar_rect = Rect::new(x, y + 10, bar_w, bar_h);

    // Background track
    fb.fill_rounded_rect_aa(bar_rect, Pixel::new(255, 255, 255, 20), 3);

    // Filled portion
    let fill_w = (bar_w as u64 * percent.min(100) as u64 / 100) as u32;
    if fill_w > 0 {
        let accent = theme::accent_color();
        let fill_rect = Rect::new(x, y + 10, fill_w, bar_h);
        fb.fill_rounded_rect_aa(fill_rect, accent, 3);
    }
}

// ─── Uptime Widget ──────────────────────────────────────────────────

fn draw_uptime_widget(fb: &mut FrameBuffer, x: i32, y: i32) -> u32 {
    let height = 60u32;
    let rect = Rect::new(x, y, WIDGET_WIDTH, height);

    fb.fill_rounded_rect_aa(rect, glass_bg(), WIDGET_RADIUS);
    fb.draw_rounded_rect(rect, glass_border(), WIDGET_RADIUS, 1);

    let info = crate::sysinfo::get_sysinfo();
    let secs = info.uptime as u64;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;

    fonts::draw_string_bold_compact(fb, x + 16, y + 10, "Uptime", value_color(), 1);

    let mut up_text = String::new();
    if hours > 0 {
        let _ = write!(up_text, "{}h {}m", hours, mins);
    } else {
        let _ = write!(up_text, "{}m {}s", mins, secs % 60);
    }
    fonts::draw_string_compact(fb, x + 16, y + 32, &up_text, label_color(), 1);

    height
}
