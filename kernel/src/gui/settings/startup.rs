/// Startup Applications tab — login autostart toggles.
use crate::gui::colors;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{draw_focus_indicator, ttf, ttf_b, ttf_h};

// ═══════════════════════════════════════════════════════════════════════════
// STARTUP APPLICATIONS TAB (9.58)
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_startup_tab(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    focus_idx: i32,
) -> i32 {
    let pad: i32 = 20;
    let mut cy = y + 16;

    // Title
    ttf_h(
        fb,
        x + pad,
        cy,
        "Startup Applications",
        Pixel::rgb(230, 240, 255),
    );
    cy += 28;
    ttf(
        fb,
        x + pad,
        cy,
        "Manage applications that start automatically when you log in.",
        Pixel::rgb(140, 145, 160),
    );
    cy += 28;

    // Column headers
    ttf(fb, x + pad, cy, "Application", Pixel::rgb(100, 105, 120));
    ttf(
        fb,
        x + w - pad - 120,
        cy,
        "Delay",
        Pixel::rgb(100, 105, 120),
    );
    ttf(
        fb,
        x + w - pad - 50,
        cy,
        "Enabled",
        Pixel::rgb(100, 105, 120),
    );
    cy += 20;
    fb.draw_hline(x + pad, cy, (w - pad * 2) as u32, Pixel::rgb(50, 50, 60));
    cy += 8;

    let startup = crate::gui::settings_ext::STARTUP.lock();
    for (i, app) in startup.apps.iter().enumerate() {
        let row_rect = Rect::new(x + pad - 4, cy - 2, (w - pad * 2 + 8) as u32, 44);

        if focus_idx == i as i32 {
            fb.fill_rounded_rect_aa(row_rect, Pixel::new(255, 255, 255, 8), 6);
        }

        // App name + description
        let name_color = if app.enabled {
            Pixel::rgb(220, 225, 235)
        } else {
            Pixel::rgb(110, 115, 125)
        };
        ttf_b(fb, x + pad, cy + 4, &app.name, name_color);
        ttf(
            fb,
            x + pad,
            cy + 22,
            &app.description,
            Pixel::rgb(100, 105, 120),
        );

        // Delay
        let delay_str = if app.delay_secs == 0 {
            alloc::string::String::from("0s")
        } else {
            alloc::format!("{}s", app.delay_secs)
        };
        ttf(
            fb,
            x + w - pad - 120,
            cy + 12,
            &delay_str,
            Pixel::rgb(160, 165, 175),
        );

        // Toggle switch
        let toggle_x = x + w - pad - 42;
        let toggle_y = cy + 10;
        let track_color = if app.enabled {
            Pixel::rgb(0, 180, 200)
        } else {
            Pixel::rgb(70, 70, 80)
        };
        fb.fill_rounded_rect_aa(Rect::new(toggle_x, toggle_y, 36, 18), track_color, 9);
        let knob_x = if app.enabled {
            toggle_x + 20
        } else {
            toggle_x + 2
        };
        fb.fill_circle_aa(knob_x + 7, toggle_y + 9, 7, Pixel::rgb(255, 255, 255));

        draw_focus_indicator(fb, row_rect, focus_idx == i as i32);
        cy += 48;
    }
    drop(startup);

    // "Add application" hint
    cy += 12;
    fb.draw_hline(x + pad, cy, (w - pad * 2) as u32, Pixel::rgb(50, 50, 60));
    cy += 16;
    ttf(
        fb,
        x + pad,
        cy,
        "Use 'startup add <name> <command>' in terminal to add apps.",
        Pixel::rgb(120, 125, 140),
    );
    cy += 24;

    cy - y + 20
}
