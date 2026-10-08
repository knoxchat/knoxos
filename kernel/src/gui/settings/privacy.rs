/// Privacy & Security tab — permission toggles and status checks.
use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{draw_focus_indicator, ttf, ttf_b, ttf_h};

// ═══════════════════════════════════════════════════════════════════════════
// PRIVACY & SECURITY TAB (9.57)
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_privacy_tab(
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
        "Privacy & Security",
        Pixel::rgb(230, 240, 255),
    );
    cy += 28;
    ttf(
        fb,
        x + pad,
        cy,
        "Control what data KnoxOS collects and who can access devices.",
        Pixel::rgb(140, 145, 160),
    );
    cy += 28;

    // Privacy toggles
    let privacy = crate::gui::settings_ext::PRIVACY.lock();
    let toggles: [(&str, &str, bool); 6] = [
        (
            "Location Services",
            "Allow apps to determine your location",
            privacy.location_services,
        ),
        (
            "Analytics & Usage",
            "Send anonymous usage statistics",
            privacy.analytics_enabled,
        ),
        (
            "Camera Access",
            "Allow apps to use the camera",
            privacy.camera_access,
        ),
        (
            "Microphone Access",
            "Allow apps to use the microphone",
            privacy.microphone_access,
        ),
        (
            "Firewall",
            "Block unauthorized incoming connections",
            privacy.firewall_enabled,
        ),
        (
            "Automatic Updates",
            "Keep system up to date automatically",
            privacy.auto_updates_enabled,
        ),
    ];
    drop(privacy);

    for (i, (label, desc, enabled)) in toggles.iter().enumerate() {
        let row_rect = Rect::new(x + pad - 4, cy - 2, (w - pad * 2 + 8) as u32, 48);

        // Background on hover area
        if focus_idx == i as i32 {
            fb.fill_rounded_rect_aa(row_rect, Pixel::new(255, 255, 255, 8), 6);
        }

        // Label
        ttf_b(fb, x + pad, cy + 4, label, Pixel::rgb(220, 225, 235));
        // Description
        ttf(fb, x + pad, cy + 22, desc, Pixel::rgb(120, 125, 140));

        // Toggle switch (right-aligned)
        let toggle_x = x + w - pad - 42;
        let toggle_y = cy + 10;
        let track_color = if *enabled {
            Pixel::rgb(0, 180, 200)
        } else {
            Pixel::rgb(70, 70, 80)
        };
        fb.fill_rounded_rect_aa(Rect::new(toggle_x, toggle_y, 36, 18), track_color, 9);
        let knob_x = if *enabled {
            toggle_x + 20
        } else {
            toggle_x + 2
        };
        fb.fill_circle_aa(knob_x + 7, toggle_y + 9, 7, Pixel::rgb(255, 255, 255));

        draw_focus_indicator(fb, row_rect, focus_idx == i as i32);
        cy += 52;
    }

    // Security info section
    cy += 12;
    fb.draw_hline(x + pad, cy, (w - pad * 2) as u32, Pixel::rgb(50, 50, 60));
    cy += 16;
    ttf_b(
        fb,
        x + pad,
        cy,
        "Security Status",
        Pixel::rgb(200, 210, 225),
    );
    cy += 22;

    let checks = [
        ("Firewall", true),
        ("Disk Encryption", false),
        ("Secure Boot", false),
        ("Auto-lock", true),
    ];
    for (label, ok) in checks {
        let icon_color = if ok {
            Pixel::rgb(158, 206, 106) // green
        } else {
            Pixel::rgb(247, 118, 142) // red
        };
        let icon_char = if ok { '\u{2713}' } else { '\u{2717}' }; // ✓ or ✗
        fonts::draw_char_bold_compact(fb, x + pad, cy, icon_char, icon_color, 1);
        ttf(fb, x + pad + 16, cy, label, Pixel::rgb(180, 185, 195));
        cy += 20;
    }

    cy - y + 20
}
