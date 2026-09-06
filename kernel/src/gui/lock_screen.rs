/// Screen Lock
/// Provides a lock screen overlay that requires password re-entry.
/// Triggered by Super+L or from the system tray / start menu.
///
/// When locked, the screen shows a dimmed/blurred overlay with a password prompt.
/// All keyboard/mouse input is routed to the lock screen until authentication succeeds.
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

/// Whether the screen is currently locked
static SCREEN_LOCKED: AtomicBool = AtomicBool::new(false);

/// Lock screen password state
static LOCK_STATE: Mutex<LockState> = Mutex::new(LockState::new_const());

/// Shake animation TSC
static SHAKE_TSC: AtomicU64 = AtomicU64::new(0);

/// Username of the locked session
static LOCKED_USER: Mutex<[u8; 64]> = Mutex::new([0u8; 64]);
static LOCKED_USER_LEN: Mutex<usize> = Mutex::new(0);

struct LockState {
    password: [u8; 64],
    password_len: usize,
    error_msg: [u8; 64],
    error_len: usize,
}

impl LockState {
    const fn new_const() -> Self {
        Self {
            password: [0u8; 64],
            password_len: 0,
            error_msg: [0u8; 64],
            error_len: 0,
        }
    }

    fn password_str(&self) -> &str {
        core::str::from_utf8(&self.password[..self.password_len]).unwrap_or("")
    }

    fn error_str(&self) -> &str {
        core::str::from_utf8(&self.error_msg[..self.error_len]).unwrap_or("")
    }

    fn set_error(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        let len = bytes.len().min(64);
        self.error_msg[..len].copy_from_slice(&bytes[..len]);
        self.error_len = len;
    }

    fn clear_error(&mut self) {
        self.error_len = 0;
    }
}

/// Check if screen is locked
pub fn is_locked() -> bool {
    SCREEN_LOCKED.load(Ordering::Relaxed)
}

/// Lock the screen
pub fn lock() {
    if SCREEN_LOCKED.load(Ordering::Relaxed) {
        return; // Already locked
    }

    // Save current username
    let uid = crate::users::get_current_uid();
    if let Some(user) = crate::users::get_user(uid) {
        let bytes = user.username.as_bytes();
        let len = bytes.len().min(64);
        let mut name = LOCKED_USER.lock();
        name[..len].copy_from_slice(&bytes[..len]);
        *LOCKED_USER_LEN.lock() = len;
    } else {
        // Default to "user"
        let default = b"user";
        let mut name = LOCKED_USER.lock();
        name[..4].copy_from_slice(default);
        *LOCKED_USER_LEN.lock() = 4;
    }

    // Clear any previous password state
    let mut state = LOCK_STATE.lock();
    state.password_len = 0;
    state.error_len = 0;
    drop(state);

    SCREEN_LOCKED.store(true, Ordering::Relaxed);
    crate::serial_println!("[KnoxOS] Screen locked");
    super::request_redraw();
}

/// Unlock the screen (after successful auth)
fn unlock() {
    SCREEN_LOCKED.store(false, Ordering::Relaxed);
    crate::serial_println!("[KnoxOS] Screen unlocked");
    super::request_redraw();
}

/// Handle character input on lock screen
pub fn handle_char(ch: char) {
    let mut state = LOCK_STATE.lock();
    state.clear_error();
    let len = state.password_len;
    if len < 63 {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        for &b in s.as_bytes() {
            let l = state.password_len;
            if l < 63 {
                state.password[l] = b;
                state.password_len = l + 1;
            }
        }
    }
    drop(state);
    super::request_redraw();
}

/// Handle backspace
pub fn handle_backspace() {
    let mut state = LOCK_STATE.lock();
    state.clear_error();
    if state.password_len > 0 {
        state.password_len -= 1;
    }
    drop(state);
    super::request_redraw();
}

/// Handle Enter — attempt unlock
pub fn handle_enter() {
    let mut state = LOCK_STATE.lock();

    // Get username
    let username = {
        let name = LOCKED_USER.lock();
        let len = *LOCKED_USER_LEN.lock();
        let mut s = String::new();
        for &b in &name[..len] {
            s.push(b as char);
        }
        s
    };

    let password = {
        let mut s = String::new();
        for &b in &state.password[..state.password_len] {
            s.push(b as char);
        }
        s
    };

    match crate::users::authenticate(&username, &password) {
        Ok(_uid) => {
            crate::serial_println!("[KnoxOS] Screen unlock successful for {}", username);
            state.password_len = 0;
            state.error_len = 0;
            drop(state);
            unlock();
        }
        Err(msg) => {
            state.set_error(msg);
            state.password_len = 0;
            SHAKE_TSC.store(crate::gui::read_tsc(), Ordering::Relaxed);
            drop(state);
            super::request_redraw();
        }
    }
}

/// Handle Escape — clear password
pub fn handle_escape() {
    let mut state = LOCK_STATE.lock();
    state.password_len = 0;
    state.clear_error();
    drop(state);
    super::request_redraw();
}

/// Draw the lock screen overlay
pub fn draw_lock_screen(fb: &mut FrameBuffer) {
    let sw = fb.width as i32;
    let sh = fb.height as i32;
    let state = LOCK_STATE.lock();
    let now = crate::gui::read_tsc();
    let shake_start = SHAKE_TSC.load(Ordering::Relaxed);

    // ══════════════════════════════════════════════════════════
    // DIM OVERLAY — Semi-transparent dark overlay on desktop
    // ══════════════════════════════════════════════════════════
    // Draw a full-screen darkening overlay
    for y in 0..sh {
        let t = y as f32 / sh as f32;
        let r = (8.0 + t * 6.0) as u8;
        let g = (10.0 + t * 4.0) as u8;
        let b = (20.0 + t * 12.0) as u8;
        fb.draw_hline(0, y, sw as u32, Pixel::new(r, g, b, 240));
    }

    // ══════════════════════════════════════════════════════════
    // LOCK CARD — Centered unlock prompt
    // ══════════════════════════════════════════════════════════
    let card_w = 380i32;
    let card_h = 280i32;
    let mut card_x = (sw - card_w) / 2;
    let card_y = (sh - card_h) / 2;

    // Shake animation on failed attempt
    if shake_start > 0 {
        let elapsed = now.wrapping_sub(shake_start);
        let tsc_freq = crate::gui::min_frame_ticks() * 60;
        let ms = (elapsed * 1000).checked_div(tsc_freq).unwrap_or(0) as i32;
        if ms < 400 {
            let t = ms as f32 / 400.0;
            let shake = libm::sinf(t * 20.0) * (1.0 - t) * 8.0;
            card_x += shake as i32;
        } else {
            SHAKE_TSC.store(0, Ordering::Relaxed);
        }
    }

    let card_rect = Rect::new(card_x, card_y, card_w as u32, card_h as u32);

    // Card shadow
    for offset in (0i32..12).step_by(3) {
        let alpha = (10 - offset).clamp(0, 10) as u8;
        let sr = Rect::new(
            card_x - offset / 2,
            card_y + 4 + offset / 2,
            card_w as u32 + offset as u32,
            card_h as u32 + offset as u32 / 2,
        );
        fb.draw_rounded_rect(sr, Pixel::new(0, 0, 0, alpha), 20 + offset as u32 / 3, 1);
    }

    // Card background
    fb.fill_rounded_rect_aa(card_rect, Pixel::new(16, 20, 32, 230), 16);
    fb.draw_rounded_rect(card_rect, Pixel::new(80, 160, 255, 50), 16, 1);

    // ══════════════════════════════════════════════════════════
    // CONTENT
    // ══════════════════════════════════════════════════════════
    let content_x = card_x + 40;
    let mut cy = card_y + 30;

    // Lock icon (simple padlock using shapes)
    let lock_cx = card_x + card_w / 2;
    // Padlock body
    fb.fill_rounded_rect_aa(
        Rect::new(lock_cx - 14, cy + 14, 28, 20),
        Pixel::new(0, 180, 255, 200),
        4,
    );
    // Padlock shackle (arc using circle outline)
    fb.draw_rounded_rect(
        Rect::new(lock_cx - 10, cy, 20, 18),
        Pixel::new(0, 180, 255, 200),
        8,
        2,
    );
    // Keyhole
    fb.fill_circle_aa(lock_cx, cy + 22, 3u32, Pixel::new(16, 20, 32, 255));

    cy += 45;

    // "Locked" title
    let title = "Locked";
    let title_w = fonts::measure_string_width(title, 1) as i32;
    fonts::draw_string_bold(
        fb,
        card_x + (card_w - title_w) / 2,
        cy,
        title,
        Pixel::new(220, 240, 255, 240),
        1,
    );
    cy += 28;

    // Username display
    let username = {
        let name = LOCKED_USER.lock();
        let len = *LOCKED_USER_LEN.lock();
        let mut s = String::new();
        for &b in &name[..len] {
            s.push(b as char);
        }
        s
    };
    let user_w = fonts::measure_string_width_compact(&username, 1) as i32;
    fonts::draw_string_compact(
        fb,
        card_x + (card_w - user_w) / 2,
        cy,
        &username,
        Pixel::new(140, 180, 220, 180),
        1,
    );
    cy += 25;

    // ── Password field ──
    let field_w = card_w - 80;
    let field_h = 32i32;
    let field_x = content_x;
    let field_y = cy;
    let field_rect = Rect::new(field_x, field_y, field_w as u32, field_h as u32);

    fb.fill_rounded_rect_aa(field_rect, Pixel::new(24, 32, 48, 220), 8);
    fb.draw_rounded_rect(field_rect, Pixel::new(0, 180, 255, 160), 8, 1);

    // Password dots or placeholder
    let text_x = field_x + 10;
    let pw_str = state.password_str();
    if pw_str.is_empty() {
        fonts::draw_string_compact(
            fb,
            text_x,
            field_y + (field_h - 10) / 2,
            "Enter password",
            Pixel::new(80, 100, 140, 100),
            1,
        );
    } else {
        let dot_count = pw_str.len();
        for i in 0..dot_count.min(30) {
            fb.fill_circle_aa(
                text_x + 4 + i as i32 * 10,
                field_y + field_h / 2,
                3u32,
                Pixel::new(220, 240, 255, 220),
            );
        }
    }

    // Blinking cursor
    let tsc = crate::gui::read_tsc();
    let freq = crate::gui::min_frame_ticks() * 60;
    let blink = if freq > 0 {
        ((tsc / (freq / 2)) % 2) == 0
    } else {
        true
    };
    if blink {
        let cursor_x = text_x + 4 + pw_str.len().min(30) as i32 * 10;
        fb.fill_rect(
            Rect::new(cursor_x + 2, field_y + 6, 2, field_h as u32 - 12),
            Pixel::new(0, 200, 255, 200),
        );
    }

    cy += field_h + 12;

    // ── Error message ──
    if state.error_len > 0 {
        let err = state.error_str();
        let err_w = fonts::measure_string_width_compact(err, 1) as i32;
        fonts::draw_string_compact(
            fb,
            card_x + (card_w - err_w) / 2,
            cy,
            err,
            Pixel::new(255, 80, 80, 255),
            1,
        );
    }
    cy += 16;

    // ── Unlock button ──
    let btn_w = 140i32;
    let btn_h = 34i32;
    let btn_x = card_x + (card_w - btn_w) / 2;
    let btn_rect = Rect::new(btn_x, cy, btn_w as u32, btn_h as u32);
    fb.fill_rounded_rect_aa(btn_rect, Pixel::new(0, 160, 255, 200), 8);
    fb.draw_rounded_rect(btn_rect, Pixel::new(80, 200, 255, 80), 8, 1);

    let lbl = "Unlock";
    let lbl_w = fonts::measure_string_width_compact(lbl, 1) as i32;
    fonts::draw_string_bold_compact(
        fb,
        btn_x + (btn_w - lbl_w) / 2,
        cy + (btn_h - 10) / 2,
        lbl,
        Pixel::new(255, 255, 255, 255),
        1,
    );

    // ── Keyboard hint ──
    let hint = "Enter password to unlock";
    let hint_w = fonts::measure_string_width_compact(hint, 1) as i32;
    fonts::draw_string_compact(
        fb,
        (sw - hint_w) / 2,
        sh - 30,
        hint,
        Pixel::new(80, 120, 160, 100),
        1,
    );

    // Draw cursor
    let mouse = super::input::MOUSE.lock();
    let mx = mouse.x;
    let my = mouse.y;
    drop(mouse);

    let cursor_color = Pixel::new(255, 255, 255, 240);
    for row in 0..12i32 {
        let w = (row / 2) + 1;
        for col in 0..w {
            fb.set_pixel(
                (mx + col).max(0) as usize,
                (my + row).max(0) as usize,
                cursor_color,
            );
        }
        fb.set_pixel(
            (mx + w).max(0) as usize,
            (my + row).max(0) as usize,
            Pixel::new(0, 0, 0, 200),
        );
    }
}

/// Handle mouse click on lock screen
pub fn handle_click(x: i32, y: i32, screen_width: i32, screen_height: i32) {
    let card_w = 380i32;
    let card_h = 280i32;
    let card_x = (screen_width - card_w) / 2;
    let card_y = (screen_height - card_h) / 2;

    // Unlock button area
    let btn_w = 140i32;
    let btn_h = 34i32;
    let btn_x = card_x + (card_w - btn_w) / 2;
    // approximate button y position
    let btn_y = card_y + 30 + 45 + 28 + 25 + 32 + 12 + 16;
    let btn_rect = Rect::new(btn_x, btn_y, btn_w as u32, btn_h as u32);

    if btn_rect.contains(x, y) {
        handle_enter();
    }
    super::request_redraw();
}
