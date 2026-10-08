/// Date & Time tab — NTP, format, timezone list.
use crate::gui::colors;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{draw_focus_indicator, draw_section_header, ttf, ttf_h};

// ═══════════════════════════════════════════════════════════════════════════
// DATE & TIME TAB (9.56)
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_datetime_tab(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    focus_idx: i32,
) -> i32 {
    let mut cy = y + 20;
    let pad = 20i32;

    // Section: Current Time
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Current Date & Time");
    cy += 30;

    // Display current time from RTC
    let rtc_time = crate::rtc::read_rtc();
    let time_str = alloc::format!(
        "{:04}-{:02}-{:02}  {:02}:{:02}:{:02}",
        rtc_time.year,
        rtc_time.month,
        rtc_time.day,
        rtc_time.hour,
        rtc_time.minute,
        rtc_time.second
    );
    ttf_h(fb, x + pad, cy, &time_str, Pixel::rgb(0, 200, 220));
    cy += 28;

    // Section: NTP
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Network Time");
    cy += 30;

    let (use_ntp, use_24h, show_seconds) = crate::gui::settings_ext::get_datetime();

    // NTP toggle
    let ntp_rect = Rect::new(x + pad, cy, (w - pad * 2) as u32, 28);
    let ntp_bg = if use_ntp {
        Pixel::new(0, 140, 200, 40)
    } else {
        Pixel::new(60, 60, 60, 40)
    };
    fb.fill_rounded_rect_aa(ntp_rect, ntp_bg, 6);
    ttf(
        fb,
        x + pad + 12,
        cy + 8,
        "Automatic time (NTP)",
        Pixel::rgb(200, 210, 230),
    );
    // Toggle indicator
    let toggle_x = x + w - pad - 44;
    let toggle_bg = if use_ntp {
        Pixel::rgb(0, 180, 220)
    } else {
        Pixel::rgb(80, 80, 85)
    };
    fb.fill_rounded_rect_aa(Rect::new(toggle_x, cy + 6, 36, 16), toggle_bg, 8);
    let knob_x = if use_ntp { toggle_x + 20 } else { toggle_x + 2 };
    fb.fill_circle_aa(knob_x + 7, cy + 14, 6, colors::WHITE);
    draw_focus_indicator(fb, ntp_rect, focus_idx == 0);
    cy += 36;

    // NTP server info
    ttf(
        fb,
        x + pad + 12,
        cy,
        "Server: pool.ntp.org",
        Pixel::rgb(120, 140, 170),
    );
    cy += 24;

    // Section: Format
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Time Format");
    cy += 30;

    // 24-hour toggle
    let fmt_rect = Rect::new(x + pad, cy, (w - pad * 2) as u32, 28);
    let fmt_bg = if use_24h {
        Pixel::new(0, 140, 200, 40)
    } else {
        Pixel::new(60, 60, 60, 40)
    };
    fb.fill_rounded_rect_aa(fmt_rect, fmt_bg, 6);
    ttf(
        fb,
        x + pad + 12,
        cy + 8,
        "Use 24-hour format",
        Pixel::rgb(200, 210, 230),
    );
    let toggle_x2 = x + w - pad - 44;
    let toggle_bg2 = if use_24h {
        Pixel::rgb(0, 180, 220)
    } else {
        Pixel::rgb(80, 80, 85)
    };
    fb.fill_rounded_rect_aa(Rect::new(toggle_x2, cy + 6, 36, 16), toggle_bg2, 8);
    let knob_x2 = if use_24h {
        toggle_x2 + 20
    } else {
        toggle_x2 + 2
    };
    fb.fill_circle_aa(knob_x2 + 7, cy + 14, 6, colors::WHITE);
    draw_focus_indicator(fb, fmt_rect, focus_idx == 1);
    cy += 36;

    // Show seconds toggle
    let sec_rect = Rect::new(x + pad, cy, (w - pad * 2) as u32, 28);
    let sec_bg = if show_seconds {
        Pixel::new(0, 140, 200, 40)
    } else {
        Pixel::new(60, 60, 60, 40)
    };
    fb.fill_rounded_rect_aa(sec_rect, sec_bg, 6);
    ttf(
        fb,
        x + pad + 12,
        cy + 8,
        "Show seconds in clock",
        Pixel::rgb(200, 210, 230),
    );
    let toggle_x3 = x + w - pad - 44;
    let toggle_bg3 = if show_seconds {
        Pixel::rgb(0, 180, 220)
    } else {
        Pixel::rgb(80, 80, 85)
    };
    fb.fill_rounded_rect_aa(Rect::new(toggle_x3, cy + 6, 36, 16), toggle_bg3, 8);
    let knob_x3 = if show_seconds {
        toggle_x3 + 20
    } else {
        toggle_x3 + 2
    };
    fb.fill_circle_aa(knob_x3 + 7, cy + 14, 6, colors::WHITE);
    draw_focus_indicator(fb, sec_rect, focus_idx == 2);
    cy += 36;

    // Section: Timezone
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Timezone");
    cy += 30;

    let timezones = crate::gui::settings_ext::common_timezones();
    let current_tz = {
        let dt = crate::gui::settings_ext::get_datetime();
        // Get timezone name from settings_ext
        dt // We'll read it differently
    };

    for (i, tz) in timezones.iter().enumerate() {
        let row_rect = Rect::new(x + pad, cy, (w - pad * 2) as u32, 28);
        let is_selected = i == 0; // Default to UTC as selected

        if is_selected {
            fb.fill_rounded_rect_aa(row_rect, Pixel::new(0, 180, 220, 30), 6);
        }

        // Radio indicator
        let radio_x = x + pad + 12;
        let radio_cy = cy + 14;
        fb.draw_circle_aa(radio_x + 5, radio_cy, 5, Pixel::rgb(120, 140, 170));
        if is_selected {
            fb.fill_circle_aa(radio_x + 5, radio_cy, 3, Pixel::rgb(0, 200, 220));
        }

        // Timezone name + offset
        let offset_h = tz.utc_offset_minutes / 60;
        let offset_m = (tz.utc_offset_minutes % 60).abs();
        let offset_str = if offset_h >= 0 {
            alloc::format!(
                "{} ({}) UTC+{:02}:{:02}",
                tz.name,
                tz.abbreviation,
                offset_h,
                offset_m
            )
        } else {
            alloc::format!(
                "{} ({}) UTC-{:02}:{:02}",
                tz.name,
                tz.abbreviation,
                -offset_h,
                offset_m
            )
        };
        ttf(
            fb,
            x + pad + 28,
            cy + 8,
            &offset_str,
            if is_selected {
                Pixel::rgb(220, 240, 255)
            } else {
                Pixel::rgb(160, 170, 190)
            },
        );
        draw_focus_indicator(fb, row_rect, focus_idx == 3 + i as i32);
        cy += 32;
    }

    cy - y + 20
}
