/// Keyboard Shortcuts Overlay — Shows a reference card of all keyboard shortcuts
/// Triggered by Ctrl+/ or from the start menu / help.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

static VISIBLE: AtomicBool = AtomicBool::new(false);

pub fn toggle() {
    VISIBLE.fetch_xor(true, Ordering::Relaxed);
    super::request_redraw();
}

pub fn show() {
    VISIBLE.store(true, Ordering::Relaxed);
    super::request_redraw();
}

pub fn hide() {
    VISIBLE.store(false, Ordering::Relaxed);
    super::request_redraw();
}

pub fn is_visible() -> bool {
    VISIBLE.load(Ordering::Relaxed)
}

struct ShortcutGroup {
    title: &'static str,
    items: &'static [(&'static str, &'static str)],
}

const GROUPS: &[ShortcutGroup] = &[
    ShortcutGroup {
        title: "General",
        items: &[
            ("Ctrl+C / Ctrl+V", "Copy / Paste"),
            ("Ctrl+X", "Cut"),
            ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
            ("Ctrl+A", "Select All"),
            ("Ctrl+S", "Save"),
            ("Ctrl+F", "Find"),
            ("Ctrl+/", "Show Shortcuts"),
            ("Alt+F4", "Close Window"),
        ],
    },
    ShortcutGroup {
        title: "Window Management",
        items: &[
            ("Alt+Tab", "Switch Windows"),
            ("Super+D", "Show Desktop"),
            ("Super+↑", "Maximize"),
            ("Super+↓", "Restore / Minimize"),
            ("Super+← / →", "Snap Left / Right"),
            ("Super+L", "Lock Screen"),
        ],
    },
    ShortcutGroup {
        title: "System",
        items: &[
            ("PrtSc", "Screenshot (Full)"),
            ("Super+Shift+S", "Screenshot (Region)"),
            ("Ctrl+Alt+T", "New Terminal"),
            ("Ctrl+Alt+Del", "System Menu"),
            ("Ctrl+Shift+Esc", "Task Manager"),
            ("Super+E", "File Explorer"),
            ("Super+Space", "App Launcher"),
        ],
    },
    ShortcutGroup {
        title: "Terminal",
        items: &[
            ("Ctrl+Shift+C", "Copy (Terminal)"),
            ("Ctrl+Shift+V", "Paste (Terminal)"),
            ("Ctrl+Shift+T", "New Tab"),
            ("Ctrl+Shift+W", "Close Tab"),
            ("Ctrl+Tab", "Next Tab"),
            ("Ctrl+L", "Clear"),
        ],
    },
];

const OVERLAY_WIDTH: u32 = 680;
const OVERLAY_HEIGHT: u32 = 520;

pub fn draw(fb: &mut FrameBuffer) {
    if !VISIBLE.load(Ordering::Relaxed) {
        return;
    }

    let sw = fb.width as i32;
    let sh = fb.height as i32;
    let x = (sw - OVERLAY_WIDTH as i32) / 2;
    let y = (sh - OVERLAY_HEIGHT as i32) / 2;

    // Dimmed backdrop
    fb.fill_rect(
        Rect::new(0, 0, sw as u32, sh as u32),
        Pixel::new(0, 0, 0, 140),
    );

    // Background
    fb.fill_rounded_rect_aa(
        Rect::new(x, y, OVERLAY_WIDTH, OVERLAY_HEIGHT),
        Pixel::new(20, 24, 36, 245),
        12,
    );
    fb.draw_rounded_rect(
        Rect::new(x, y, OVERLAY_WIDTH, OVERLAY_HEIGHT),
        Pixel::new(60, 90, 140, 100),
        12,
        1,
    );

    // Title
    fonts::draw_string_compact(
        fb,
        x + 24,
        y + 18,
        "Keyboard Shortcuts",
        Pixel::rgb(230, 235, 245),
        2,
    );

    // Close hint
    fonts::draw_string_compact(
        fb,
        x + OVERLAY_WIDTH as i32 - 120,
        y + 22,
        "Press Esc to close",
        Pixel::rgb(100, 110, 130),
        1,
    );

    // Separator
    fb.fill_rect(
        Rect::new(x + 16, y + 44, OVERLAY_WIDTH - 32, 1),
        Pixel::new(60, 70, 100, 80),
    );

    // Draw groups in a 2-column layout
    let col_w = (OVERLAY_WIDTH - 48) / 2;
    let start_y = y + 56;

    for (gi, group) in GROUPS.iter().enumerate() {
        let col = gi % 2;
        let row = gi / 2;
        let gx = x + 20 + (col as i32 * (col_w as i32 + 8));
        let gy = start_y + row as i32 * 220;

        // Group title
        let accent = super::colors::accent();
        fonts::draw_string_compact(fb, gx, gy, group.title, accent, 2);

        // Items
        for (ii, (key, desc)) in group.items.iter().enumerate() {
            let iy = gy + 24 + ii as i32 * 22;

            // Key badge background
            let key_w = (key.len() as u32 * 7 + 12).min(col_w / 2);
            fb.fill_rounded_rect_aa(
                Rect::new(gx, iy - 1, key_w, 18),
                Pixel::new(40, 50, 70, 180),
                4,
            );
            fonts::draw_string_compact(fb, gx + 6, iy + 3, key, Pixel::rgb(180, 200, 240), 1);

            // Description
            fonts::draw_string_compact(
                fb,
                gx + key_w as i32 + 8,
                iy + 3,
                desc,
                Pixel::rgb(180, 185, 195),
                1,
            );
        }
    }
}

/// Handle click — close overlay if clicking outside, or clicking anywhere
pub fn handle_click(_mx: i32, _my: i32) -> bool {
    if !VISIBLE.load(Ordering::Relaxed) {
        return false;
    }
    hide();
    true
}
