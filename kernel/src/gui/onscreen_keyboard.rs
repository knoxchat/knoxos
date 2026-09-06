/// On-screen virtual keyboard for tablet mode / accessibility
///
/// Provides a full QWERTY layout rendered on the lower portion of the screen.
/// Supports shift, symbols layer, and direct input injection.
use spin::Mutex;

use super::framebuffer::{FrameBuffer, Pixel, Rect};

/// On-screen keyboard state
pub struct OnScreenKeyboard {
    /// Whether the keyboard is visible
    pub visible: bool,
    /// Shift key is active (uppercase)
    pub shift: bool,
    /// Symbols layer is active
    pub symbols: bool,
    /// Caps lock engaged
    pub caps_lock: bool,
    /// Keyboard height in pixels
    pub height: u32,
}

impl OnScreenKeyboard {
    pub fn new() -> Self {
        Self {
            visible: false,
            shift: false,
            symbols: false,
            caps_lock: false,
            height: 240,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref OSK: Mutex<OnScreenKeyboard> = Mutex::new(OnScreenKeyboard::new());
}

/// Standard QWERTY row layouts
const ROW_1: &[&str] = &["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"];
const ROW_2: &[&str] = &["a", "s", "d", "f", "g", "h", "j", "k", "l"];
const ROW_3: &[&str] = &["z", "x", "c", "v", "b", "n", "m"];
const ROW_1_SHIFT: &[&str] = &["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"];
const ROW_2_SHIFT: &[&str] = &["A", "S", "D", "F", "G", "H", "J", "K", "L"];
const ROW_3_SHIFT: &[&str] = &["Z", "X", "C", "V", "B", "N", "M"];
const ROW_1_SYM: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
const ROW_2_SYM: &[&str] = &["@", "#", "$", "%", "&", "*", "-", "+", "="];
const ROW_3_SYM: &[&str] = &["!", "?", "/", "(", ")", "'", "\""];

/// Show / hide the on-screen keyboard
pub fn toggle() {
    let mut osk = OSK.lock();
    osk.visible = !osk.visible;
    osk.shift = false;
    osk.symbols = false;
    crate::serial_println!("[OSK] visible={}", osk.visible);
}

pub fn show() {
    OSK.lock().visible = true;
}

pub fn hide() {
    OSK.lock().visible = false;
}

pub fn is_visible() -> bool {
    OSK.lock().visible
}

/// Get the keyboard height (so other UI can adjust)
pub fn keyboard_height() -> u32 {
    let osk = OSK.lock();
    if osk.visible { osk.height } else { 0 }
}

/// Draw the on-screen keyboard onto the framebuffer
pub fn draw(fb: &mut FrameBuffer) {
    let osk = OSK.lock();
    if !osk.visible {
        return;
    }

    let screen_w = fb.width as u32;
    let screen_h = fb.height as u32;
    let kb_h = osk.height;
    let kb_y = screen_h.saturating_sub(kb_h);
    let use_shift = osk.shift || osk.caps_lock;
    let use_sym = osk.symbols;

    let rows: [&[&str]; 3] = if use_sym {
        [ROW_1_SYM, ROW_2_SYM, ROW_3_SYM]
    } else if use_shift {
        [ROW_1_SHIFT, ROW_2_SHIFT, ROW_3_SHIFT]
    } else {
        [ROW_1, ROW_2, ROW_3]
    };

    let bg = Pixel::new(45, 45, 50, 255);
    let key_bg = Pixel::new(70, 70, 78, 255);
    let key_text = Pixel::new(240, 240, 240, 255);
    let special_bg = Pixel::new(55, 55, 62, 255);

    // Fill background
    fb.fill_rect(Rect::new(0, kb_y as i32, screen_w, kb_h), bg);

    let row_h = kb_h / 5;
    let gap = 3u32;

    // Draw letter rows
    for (ri, row) in rows.iter().enumerate() {
        let row_y = kb_y + (ri as u32 + 1) * row_h;
        let key_count = row.len() as u32;
        let key_w = (screen_w - gap * (key_count + 1)) / key_count;
        let offset_x = (screen_w - (key_w * key_count + gap * (key_count - 1))) / 2;

        for (ki, label) in row.iter().enumerate() {
            let kx = offset_x + ki as u32 * (key_w + gap);
            draw_key(fb, kx, row_y, key_w, row_h - gap, label, key_bg, key_text);
        }
    }

    // Bottom row: Shift, ?123, Space, Backspace, Enter
    let bottom_y = kb_y + 4 * row_h;
    let special_w = screen_w / 6;
    let space_w = screen_w - special_w * 4 - gap * 5;

    draw_key(
        fb,
        gap,
        bottom_y,
        special_w,
        row_h - gap,
        if use_shift { "SHIFT" } else { "shift" },
        special_bg,
        key_text,
    );
    draw_key(
        fb,
        gap * 2 + special_w,
        bottom_y,
        special_w,
        row_h - gap,
        "?123",
        special_bg,
        key_text,
    );
    draw_key(
        fb,
        gap * 3 + special_w * 2,
        bottom_y,
        space_w,
        row_h - gap,
        "space",
        key_bg,
        key_text,
    );
    draw_key(
        fb,
        gap * 4 + special_w * 2 + space_w,
        bottom_y,
        special_w,
        row_h - gap,
        "bksp",
        special_bg,
        key_text,
    );
    draw_key(
        fb,
        gap * 5 + special_w * 3 + space_w,
        bottom_y,
        special_w,
        row_h - gap,
        "enter",
        special_bg,
        key_text,
    );

    // Top number row
    let top_y = kb_y;
    let nums = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
    let num_w = (screen_w - gap * 11) / 10;
    let num_off = (screen_w - (num_w * 10 + gap * 9)) / 2;
    for (i, n) in nums.iter().enumerate() {
        let nx = num_off + i as u32 * (num_w + gap);
        draw_key(fb, nx, top_y, num_w, row_h - gap, n, special_bg, key_text);
    }
}

/// Draw a single key rectangle with centered label
fn draw_key(
    fb: &mut FrameBuffer,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    label: &str,
    bg: Pixel,
    text_color: Pixel,
) {
    // Draw key background (simple rectangle — rounded corners are cosmetic)
    let key_rect = Rect::new(x as i32, y as i32, w, h);
    fb.fill_rect(key_rect, bg);

    // Center the label text
    let char_w = 8u32;
    let text_w = label.len() as u32 * char_w;
    let tx = (x + w.saturating_sub(text_w) / 2) as i32;
    let ty = (y + h.saturating_sub(14) / 2) as i32;
    for (i, ch) in label.chars().enumerate() {
        super::fonts::draw_char(fb, tx + (i as i32) * char_w as i32, ty, ch, text_color, 1);
    }
}

/// Handle a tap/click on the on-screen keyboard
/// Returns the character to inject, or None if it was a modifier key
pub fn handle_click(x: i32, y: i32, screen_w: u32, screen_h: u32) -> Option<char> {
    let mut osk = OSK.lock();
    if !osk.visible {
        return None;
    }

    let kb_h = osk.height;
    let kb_y = (screen_h.saturating_sub(kb_h)) as i32;
    if y < kb_y {
        return None;
    }

    let row_h = kb_h / 5;
    let gap = 3u32;
    let rel_y = (y - kb_y) as u32;
    let row_idx = rel_y / row_h;

    let use_shift = osk.shift || osk.caps_lock;
    let use_sym = osk.symbols;

    let rows: [&[&str]; 3] = if use_sym {
        [ROW_1_SYM, ROW_2_SYM, ROW_3_SYM]
    } else if use_shift {
        [ROW_1_SHIFT, ROW_2_SHIFT, ROW_3_SHIFT]
    } else {
        [ROW_1, ROW_2, ROW_3]
    };

    // Top number row
    if row_idx == 0 {
        let nums = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
        let num_w = (screen_w - gap * 11) / 10;
        let num_off = (screen_w - (num_w * 10 + gap * 9)) / 2;
        for (i, ch) in nums.iter().enumerate() {
            let nx = num_off + i as u32 * (num_w + gap);
            if x as u32 >= nx && (x as u32) < nx + num_w {
                if osk.shift && !osk.caps_lock {
                    osk.shift = false;
                }
                return Some(*ch);
            }
        }
        return None;
    }

    // Letter rows (1-3)
    if (1..=3).contains(&row_idx) {
        let ri = (row_idx - 1) as usize;
        if ri < rows.len() {
            let row = rows[ri];
            let key_count = row.len() as u32;
            let key_w = (screen_w - gap * (key_count + 1)) / key_count;
            let offset_x = (screen_w - (key_w * key_count + gap * (key_count - 1))) / 2;
            for (ki, label) in row.iter().enumerate() {
                let kx = offset_x + ki as u32 * (key_w + gap);
                if x as u32 >= kx && (x as u32) < kx + key_w {
                    let ch = label.chars().next().unwrap_or(' ');
                    if osk.shift && !osk.caps_lock {
                        osk.shift = false;
                    }
                    return Some(ch);
                }
            }
        }
        return None;
    }

    // Bottom row: Shift, ?123, Space, Backspace, Enter
    if row_idx == 4 {
        let special_w = screen_w / 6;
        let space_w = screen_w - special_w * 4 - gap * 5;

        let shift_end = gap + special_w;
        let sym_end = gap * 2 + special_w * 2;
        let space_end = gap * 3 + special_w * 2 + space_w;
        let bksp_end = gap * 4 + special_w * 3 + space_w;

        let ux = x as u32;
        if ux < shift_end {
            osk.shift = !osk.shift;
            return None;
        } else if ux < sym_end {
            osk.symbols = !osk.symbols;
            return None;
        } else if ux < space_end {
            return Some(' ');
        } else if ux < bksp_end {
            return Some('\x08'); // backspace
        } else {
            return Some('\n'); // enter
        }
    }

    None
}
