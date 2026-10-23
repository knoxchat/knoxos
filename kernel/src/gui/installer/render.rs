use core::sync::atomic::Ordering;

use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::draw::{ACCENT_CYAN, TEXT_WHITE, char_to_str, draw_button, draw_text};
use super::state::{CURRENT_STEP, INSTALL_COMPLETE, INSTALL_PROGRESS, INSTALLER_STATE, is_active};
use super::steps::{
    render_bootloader, render_disk_setup, render_filesystem, render_install, render_timezone,
    render_user_account, render_welcome,
};
use super::types::{BUTTON_WIDTH, CARD_HEIGHT, CARD_WIDTH, SIDEBAR_WIDTH, STEP_COUNT, STEP_NAMES};

// ─── Rendering ───────────────────────────────────────────────────────

/// Render the full installer screen onto the framebuffer
pub fn render(fb: &mut FrameBuffer) {
    if !is_active() {
        return;
    }
    if crate::gui::gpui_platform::is_ready() {
        return;
    }

    let sw = fb.width;
    let sh = fb.height;

    // Dark overlay background
    for y in 0..sh {
        for x in 0..sw {
            let base = fb.get_pixel(x, y);
            fb.set_pixel(x, y, Pixel::new(base.r / 3, base.g / 3, base.b / 3, 255));
        }
    }

    // Main card
    let cx = sw.saturating_sub(CARD_WIDTH) / 2;
    let cy = sh.saturating_sub(CARD_HEIGHT) / 2;
    let card_bg = Pixel::new(22, 22, 35, 230);
    for y in cy..cy + CARD_HEIGHT {
        for x in cx..cx + CARD_WIDTH {
            fb.set_pixel(x, y, card_bg);
        }
    }

    // Holographic top border
    for x in cx..cx + CARD_WIDTH {
        fb.set_pixel(x, cy, ACCENT_CYAN);
        fb.set_pixel(x, cy + 1, ACCENT_CYAN);
    }

    // Sidebar
    let sidebar_bg = Pixel::new(16, 16, 28, 255);
    for y in (cy + 2)..cy + CARD_HEIGHT {
        for x in cx..cx + SIDEBAR_WIDTH {
            fb.set_pixel(x, y, sidebar_bg);
        }
    }

    let cur_step = CURRENT_STEP.load(Ordering::Relaxed) as usize;
    let green = Pixel::new(100, 220, 140, 255);
    let dim = Pixel::new(120, 120, 150, 255);

    draw_text(fb, cx + 16, cy + 16, "KnoxOS Install", TEXT_WHITE);

    for (i, &name) in STEP_NAMES.iter().enumerate() {
        let sy = cy + 50 + i * 32;
        let col = if i == cur_step {
            ACCENT_CYAN
        } else if i < cur_step {
            green
        } else {
            dim
        };
        let num_char = (b'1' + i as u8) as char;
        let mut num_buf = [0u8; 4];
        let num_str = char_to_str(num_char, &mut num_buf);
        draw_text(fb, cx + 16, sy, num_str, col);
        draw_text(fb, cx + 32, sy, name, col);
        if i == cur_step {
            for yy in sy..sy + 20 {
                fb.set_pixel(cx, yy, ACCENT_CYAN);
                fb.set_pixel(cx + 1, yy, ACCENT_CYAN);
            }
        }
    }

    // Content area
    let content_x = cx + SIDEBAR_WIDTH + 24;
    let content_y = cy + 24;
    let content_w = CARD_WIDTH - SIDEBAR_WIDTH - 48;

    match cur_step {
        0 => render_welcome(fb, content_x, content_y),
        1 => render_disk_setup(fb, content_x, content_y),
        2 => render_filesystem(fb, content_x, content_y),
        3 => render_bootloader(fb, content_x, content_y),
        4 => render_user_account(fb, content_x, content_y),
        5 => render_timezone(fb, content_x, content_y),
        6 => render_install(fb, content_x, content_y, content_w),
        _ => {}
    }

    // Navigation buttons
    let btn_y = cy + CARD_HEIGHT - 52;
    if cur_step > 0 && !INSTALL_COMPLETE.load(Ordering::Relaxed) {
        draw_button(fb, cx + SIDEBAR_WIDTH + 24, btn_y, "< Back", false);
    }
    let next_x = cx + CARD_WIDTH - BUTTON_WIDTH - 24;
    if INSTALL_COMPLETE.load(Ordering::Relaxed) {
        draw_button(fb, next_x, btn_y, "Reboot", true);
    } else if cur_step == STEP_COUNT - 1 {
        if INSTALL_PROGRESS.load(Ordering::Relaxed) == 0 {
            draw_button(fb, next_x, btn_y, "Install", true);
        }
    } else {
        draw_button(fb, next_x, btn_y, "Next >", true);
    }

    // Error message
    let st = INSTALLER_STATE.lock();
    if st.error_len > 0 {
        let err_str = core::str::from_utf8(&st.error_msg[..st.error_len]).unwrap_or("Error");
        draw_text(
            fb,
            content_x,
            btn_y - 24,
            err_str,
            Pixel::new(255, 80, 80, 255),
        );
    }
}
