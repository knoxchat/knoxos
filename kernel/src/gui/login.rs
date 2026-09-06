/// Login Screen / Display Manager
/// Provides a visually appealing login screen with user authentication
/// before granting access to the desktop environment.
///
/// Design: Nebula Depth glassmorphism theme with centered login card,
/// animated background, and typed password field.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

/// Login screen state
static LOGIN_STATE: Mutex<LoginState> = Mutex::new(LoginState::new_const());

/// Whether the login screen is currently active (blocks desktop)
static LOGGED_IN: AtomicBool = AtomicBool::new(false);

/// Timestamp when login screen was last redrawn (for animation)
static LOGIN_ANIM_TSC: AtomicU64 = AtomicU64::new(0);

/// Login attempt failure animation timer
static SHAKE_TSC: AtomicU64 = AtomicU64::new(0);

/// Login screen state machine
struct LoginState {
    username: [u8; 64],
    username_len: usize,
    password: [u8; 64],
    password_len: usize,
    /// Which field is focused: 0 = username, 1 = password
    focused_field: u8,
    /// Error message to display (e.g., "Invalid password")
    error_msg: [u8; 64],
    error_len: usize,
    /// Whether the login is being processed (brief delay for UX)
    authenticating: bool,
}

impl LoginState {
    const fn new_const() -> Self {
        Self {
            username: [0u8; 64],
            username_len: 0,
            password: [0u8; 64],
            password_len: 0,
            focused_field: 0,
            error_msg: [0u8; 64],
            error_len: 0,
            authenticating: false,
        }
    }

    fn username_str(&self) -> &str {
        core::str::from_utf8(&self.username[..self.username_len]).unwrap_or("")
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

/// Check if the user is logged in (desktop should be shown)
pub fn is_logged_in() -> bool {
    LOGGED_IN.load(Ordering::Relaxed)
}

/// Force login state (for initial boot — auto-login disabled by default)
pub fn set_logged_in(val: bool) {
    LOGGED_IN.store(val, Ordering::Relaxed);
}

/// Handle a character input on the login screen
pub fn handle_char(ch: char) {
    let mut state = LOGIN_STATE.lock();
    if state.authenticating {
        return;
    }
    state.clear_error();

    if state.focused_field == 0 {
        // Username field
        if state.username_len < 63 {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            for &b in s.as_bytes() {
                let len = state.username_len;
                if len < 63 {
                    state.username[len] = b;
                    state.username_len = len + 1;
                }
            }
        }
    } else {
        // Password field
        if state.password_len < 63 {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            for &b in s.as_bytes() {
                let len = state.password_len;
                if len < 63 {
                    state.password[len] = b;
                    state.password_len = len + 1;
                }
            }
        }
    }
    super::request_redraw();
}

/// Handle backspace on login screen
pub fn handle_backspace() {
    let mut state = LOGIN_STATE.lock();
    if state.authenticating {
        return;
    }
    state.clear_error();

    if state.focused_field == 0 {
        if state.username_len > 0 {
            state.username_len -= 1;
        }
    } else {
        if state.password_len > 0 {
            state.password_len -= 1;
        }
    }
    super::request_redraw();
}

/// Handle Tab key — switch between username and password fields
pub fn handle_tab() {
    let mut state = LOGIN_STATE.lock();
    if state.authenticating {
        return;
    }
    state.focused_field = if state.focused_field == 0 { 1 } else { 0 };
    super::request_redraw();
}

/// Handle Enter key — attempt login
pub fn handle_enter() {
    let mut state = LOGIN_STATE.lock();
    if state.authenticating {
        return;
    }

    // If on username field, move to password
    if state.focused_field == 0 {
        state.focused_field = 1;
        drop(state);
        super::request_redraw();
        return;
    }

    // Attempt authentication
    let username = {
        let mut s = String::new();
        for &b in &state.username[..state.username_len] {
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
        Ok(uid) => {
            // Success!
            crate::serial_println!("[KnoxOS] Login successful: user={}, uid={}", username, uid);
            state.authenticating = true;
            state.clear_error();
            drop(state);

            // Set the logged-in user context
            LOGGED_IN.store(true, Ordering::Relaxed);
            super::sounds::login();
            super::request_redraw();
        }
        Err(msg) => {
            state.set_error(msg);
            // Clear password on failure
            state.password_len = 0;
            // Trigger shake animation
            SHAKE_TSC.store(crate::gui::read_tsc(), Ordering::Relaxed);
            drop(state);
            super::request_redraw();
        }
    }
}

/// Handle Escape key on login screen — clear fields
pub fn handle_escape() {
    let mut state = LOGIN_STATE.lock();
    state.username_len = 0;
    state.password_len = 0;
    state.focused_field = 0;
    state.clear_error();
    state.authenticating = false;
    drop(state);
    super::request_redraw();
}

/// Draw the login screen
pub fn draw_login_screen(fb: &mut FrameBuffer) {
    let sw = fb.width as i32;
    let sh = fb.height as i32;
    let state = LOGIN_STATE.lock();
    let now = crate::gui::read_tsc();
    let shake_start = SHAKE_TSC.load(Ordering::Relaxed);

    // ══════════════════════════════════════════════════════════
    // BACKGROUND — Deep space gradient with subtle animated stars
    // ══════════════════════════════════════════════════════════
    draw_login_background(fb, sw, sh, now);

    // ══════════════════════════════════════════════════════════
    // LOGIN CARD — Centered glassmorphism panel
    // ══════════════════════════════════════════════════════════
    let card_w = 420i32;
    let card_h = 380i32;
    let mut card_x = (sw - card_w) / 2;
    let card_y = (sh - card_h) / 2;

    // Shake animation on failed login
    if shake_start > 0 {
        let elapsed = now.wrapping_sub(shake_start);
        let tsc_freq = crate::gui::min_frame_ticks() * 60;
        let ms = (elapsed * 1000).checked_div(tsc_freq).unwrap_or(0) as i32;
        if ms < 400 {
            // Damped sine wave shake
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

    // Card background (glassmorphism)
    fb.fill_rounded_rect_aa(card_rect, Pixel::new(16, 20, 32, 210), 16);

    // Card border (holographic)
    fb.draw_rounded_rect(card_rect, Pixel::new(80, 160, 255, 60), 16, 1);
    let inner = Rect::new(card_x + 1, card_y + 1, card_w as u32 - 2, card_h as u32 - 2);
    fb.draw_rounded_rect(inner, Pixel::new(120, 200, 255, 25), 15, 1);

    // Top highlight
    let hl_x = card_x + 20;
    let hl_w = (card_w - 40) as u32;
    fb.draw_hline(hl_x, card_y + 1, hl_w, Pixel::new(180, 220, 255, 30));

    // ══════════════════════════════════════════════════════════
    // CONTENT
    // ══════════════════════════════════════════════════════════
    let content_x = card_x + 40;
    let mut cy = card_y + 30;

    // ── OS Logo / Title ──
    // Draw "KnoxOS" title centered
    let title = "KnoxOS";
    let title_w = fonts::measure_string_width(title, 1) as i32;
    let title_x = card_x + (card_w - title_w * 2) / 2; // approximate 2x scale
    // Draw large title using standard font (double-size effect)
    fonts::draw_string_bold(fb, title_x, cy, title, Pixel::new(0, 200, 255, 255), 2);
    cy += 50;

    // Subtitle
    let subtitle = "Welcome back";
    let sub_w = fonts::measure_string_width_compact(subtitle, 1) as i32;
    fonts::draw_string_compact(
        fb,
        card_x + (card_w - sub_w) / 2,
        cy,
        subtitle,
        Pixel::new(160, 200, 240, 180),
        1,
    );
    cy += 30;

    // ── Username field ──
    cy += 10;
    draw_input_field(
        fb,
        content_x,
        cy,
        card_w - 80,
        "Username",
        state.username_str(),
        false,
        state.focused_field == 0,
    );
    cy += 60;

    // ── Password field ──
    draw_input_field(
        fb,
        content_x,
        cy,
        card_w - 80,
        "Password",
        state.password_str(),
        true, // masked
        state.focused_field == 1,
    );
    cy += 60;

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
    cy += 20;

    // ── Login button ──
    let btn_w = 160i32;
    let btn_h = 36i32;
    let btn_x = card_x + (card_w - btn_w) / 2;
    let btn_rect = Rect::new(btn_x, cy, btn_w as u32, btn_h as u32);

    if state.authenticating {
        // Pulsing button during auth
        fb.fill_rounded_rect_aa(btn_rect, Pixel::new(0, 180, 220, 200), 8);
    } else {
        fb.fill_rounded_rect_aa(btn_rect, Pixel::new(0, 160, 255, 200), 8);
    }
    fb.draw_rounded_rect(btn_rect, Pixel::new(80, 200, 255, 80), 8, 1);

    let btn_label = if state.authenticating {
        "Signing in..."
    } else {
        "Sign In"
    };
    let lbl_w = fonts::measure_string_width_compact(btn_label, 1) as i32;
    fonts::draw_string_bold_compact(
        fb,
        btn_x + (btn_w - lbl_w) / 2,
        cy + (btn_h - 10) / 2,
        btn_label,
        Pixel::new(255, 255, 255, 255),
        1,
    );

    // ── Footer — version / clock ──
    let footer_y = sh - 30;
    let ver = "KnoxOS v0.2.1";
    let ver_w = fonts::measure_string_width_compact(ver, 1) as i32;
    fonts::draw_string_compact(
        fb,
        (sw - ver_w) / 2,
        footer_y,
        ver,
        Pixel::new(100, 140, 180, 120),
        1,
    );

    // ── Keyboard hint ──
    let hint = "Tab: switch fields  |  Enter: sign in";
    let hint_w = fonts::measure_string_width_compact(hint, 1) as i32;
    fonts::draw_string_compact(
        fb,
        (sw - hint_w) / 2,
        footer_y + 14,
        hint,
        Pixel::new(80, 120, 160, 100),
        1,
    );

    // Draw cursor
    draw_login_cursor(fb, sw, sh);
}

/// Draw an input field with label
fn draw_input_field(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    width: i32,
    label: &str,
    value: &str,
    masked: bool,
    focused: bool,
) {
    // Label
    fonts::draw_string_compact(fb, x, y, label, Pixel::new(140, 180, 220, 180), 1);

    // Field background
    let field_y = y + 16;
    let field_h = 32i32;
    let field_rect = Rect::new(x, field_y, width as u32, field_h as u32);

    let bg_color = if focused {
        Pixel::new(24, 32, 48, 220)
    } else {
        Pixel::new(18, 24, 36, 180)
    };
    fb.fill_rounded_rect_aa(field_rect, bg_color, 8);

    // Border
    let border_color = if focused {
        Pixel::new(0, 180, 255, 160)
    } else {
        Pixel::new(60, 100, 160, 60)
    };
    fb.draw_rounded_rect(field_rect, border_color, 8, 1);

    // Text value
    let text_x = x + 10;
    let text_y = field_y + (field_h - 14) / 2;

    if value.is_empty() {
        // Placeholder
        let placeholder = if masked {
            "Enter password"
        } else {
            "Enter username"
        };
        fonts::draw_string_compact(
            fb,
            text_x,
            text_y,
            placeholder,
            Pixel::new(80, 100, 140, 100),
            1,
        );
    } else if masked {
        // Show dots for password
        let dot_count = value.len();
        let mut dots = String::new();
        for _ in 0..dot_count {
            dots.push('\u{2022}'); // bullet character
        }
        // Draw dots using simple circles instead of font
        for i in 0..dot_count.min(40) {
            fb.fill_circle_aa(
                text_x + 4 + i as i32 * 10,
                field_y + field_h / 2,
                3,
                Pixel::new(220, 240, 255, 220),
            );
        }
    } else {
        fonts::draw_string_compact(fb, text_x, text_y, value, Pixel::new(220, 240, 255, 240), 1);
    }

    // Blinking cursor in focused field
    if focused {
        let tsc = crate::gui::read_tsc();
        let freq = crate::gui::min_frame_ticks() * 60;
        let blink = if freq > 0 {
            ((tsc / (freq / 2)) % 2) == 0
        } else {
            true
        };
        if blink {
            let cursor_x = if masked {
                text_x + 4 + value.len().min(40) as i32 * 10
            } else {
                text_x + fonts::measure_string_width_compact(value, 1) as i32
            };
            fb.fill_rect(
                Rect::new(cursor_x + 2, field_y + 6, 2, field_h as u32 - 12),
                Pixel::new(0, 200, 255, 200),
            );
        }
    }
}

/// Draw the login background (deep space gradient with star particles)
fn draw_login_background(fb: &mut FrameBuffer, sw: i32, sh: i32, _now: u64) {
    // Vertical gradient: deep navy to dark purple
    for y in 0..sh {
        let t = y as f32 / sh as f32;
        let r = (4.0 + t * 12.0) as u8;
        let g = (6.0 + t * 4.0) as u8;
        let b = (16.0 + t * 20.0) as u8;
        fb.draw_hline(0, y, sw as u32, Pixel::new(r, g, b, 255));
    }

    // Fixed star-like dots (deterministic based on position)
    let star_seeds: [(i32, i32, u8); 24] = [
        (120, 80, 180),
        (350, 150, 140),
        (600, 60, 200),
        (900, 120, 160),
        (1100, 200, 190),
        (1400, 80, 150),
        (1650, 160, 170),
        (1800, 50, 200),
        (200, 300, 130),
        (500, 400, 175),
        (780, 350, 145),
        (1050, 280, 185),
        (1300, 380, 155),
        (1550, 300, 195),
        (1700, 420, 140),
        (250, 550, 165),
        (530, 600, 180),
        (830, 520, 150),
        (1100, 580, 190),
        (1380, 500, 160),
        (1600, 550, 175),
        (450, 750, 145),
        (950, 700, 185),
        (1500, 680, 170),
    ];
    for &(sx, sy, brightness) in &star_seeds {
        if sx < sw && sy < sh {
            let b16 = brightness as u16;
            let p = Pixel::new(
                brightness,
                brightness,
                (b16 + 40).min(255) as u8,
                brightness,
            );
            fb.fill_circle_aa(sx, sy, 1u32, p);
        }
    }

    // Subtle nebula glow in center
    let cx = sw / 2;
    let cy = sh / 2 - 40;
    for r in (0..200).step_by(8) {
        let alpha = ((200 - r) / 20).min(8) as u8;
        fb.fill_circle_aa(cx, cy, r as u32, Pixel::new(20, 60, 120, alpha));
    }
}

/// Draw the mouse cursor on the login screen
fn draw_login_cursor(fb: &mut FrameBuffer, _sw: i32, _sh: i32) {
    let mouse = super::input::MOUSE.lock();
    let mx = mouse.x;
    let my = mouse.y;
    drop(mouse);

    // Simple arrow cursor
    let cursor_color = Pixel::new(255, 255, 255, 240);
    let border_color = Pixel::new(0, 0, 0, 200);

    // Draw a simple triangular cursor (12 pixels tall)
    for row in 0..12i32 {
        let w = (row / 2) + 1;
        for col in 0..w {
            let px = (mx + col).max(0) as usize;
            let py = (my + row).max(0) as usize;
            fb.set_pixel(px, py, cursor_color);
        }
        // Border
        fb.set_pixel(
            (mx + w).max(0) as usize,
            (my + row).max(0) as usize,
            border_color,
        );
    }
    // Bottom line
    for col in 0..7i32 {
        fb.set_pixel(
            (mx + col).max(0) as usize,
            (my + 12).max(0) as usize,
            border_color,
        );
    }
}

/// Handle mouse click on login screen
pub fn handle_click(x: i32, y: i32, screen_width: i32, screen_height: i32) {
    let card_w = 420i32;
    let card_h = 380i32;
    let card_x = (screen_width - card_w) / 2;
    let card_y = (screen_height - card_h) / 2;
    let content_x = card_x + 40;
    let field_w = card_w - 80;

    // Username field area
    let uname_field_y = card_y + 30 + 50 + 30 + 10 + 16; // approximate
    let uname_rect = Rect::new(content_x, uname_field_y, field_w as u32, 32);

    // Password field area
    let pass_field_y = uname_field_y + 60 + 16;
    let pass_rect = Rect::new(content_x, pass_field_y, field_w as u32, 32);

    // Login button area
    let btn_w = 160i32;
    let btn_h = 36i32;
    let btn_x = card_x + (card_w - btn_w) / 2;
    let btn_y = pass_field_y + 32 + 20 + 20;
    let btn_rect = Rect::new(btn_x, btn_y, btn_w as u32, btn_h as u32);

    let mut state = LOGIN_STATE.lock();
    if uname_rect.contains(x, y) {
        state.focused_field = 0;
    } else if pass_rect.contains(x, y) {
        state.focused_field = 1;
    } else if btn_rect.contains(x, y) {
        state.focused_field = 1;
        drop(state);
        handle_enter();
        return;
    }
    drop(state);
    super::request_redraw();
}

/// Reset login screen state (e.g., after logout)
pub fn reset() {
    let mut state = LOGIN_STATE.lock();
    state.username_len = 0;
    state.password_len = 0;
    state.focused_field = 0;
    state.error_len = 0;
    state.authenticating = false;
    LOGGED_IN.store(false, Ordering::Relaxed);
}
