use alloc::format;
/// First-Boot Setup Wizard (OOBE — Out of Box Experience)
///
/// Guided initial configuration on first boot:
///   Step 1: Welcome
///   Step 2: Language & region
///   Step 3: Keyboard layout
///   Step 4: Timezone
///   Step 5: Network / WiFi
///   Step 6: User account creation
///   Step 7: Privacy settings
///   Step 8: Theme / appearance
///   Step 9: Complete
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use super::window::{self, WindowContentType, WindowId};

// ── Steps ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStep {
    Welcome,
    Language,
    Keyboard,
    Timezone,
    Network,
    UserAccount,
    Privacy,
    Theme,
    Complete,
}

impl SetupStep {
    fn index(self) -> u8 {
        match self {
            Self::Welcome => 0,
            Self::Language => 1,
            Self::Keyboard => 2,
            Self::Timezone => 3,
            Self::Network => 4,
            Self::UserAccount => 5,
            Self::Privacy => 6,
            Self::Theme => 7,
            Self::Complete => 8,
        }
    }

    fn from_index(i: u8) -> Self {
        match i {
            0 => Self::Welcome,
            1 => Self::Language,
            2 => Self::Keyboard,
            3 => Self::Timezone,
            4 => Self::Network,
            5 => Self::UserAccount,
            6 => Self::Privacy,
            7 => Self::Theme,
            _ => Self::Complete,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome to KnoxOS",
            Self::Language => "Language & Region",
            Self::Keyboard => "Keyboard Layout",
            Self::Timezone => "Timezone",
            Self::Network => "Network",
            Self::UserAccount => "Create Your Account",
            Self::Privacy => "Privacy",
            Self::Theme => "Appearance",
            Self::Complete => "All Set!",
        }
    }

    const TOTAL: u8 = 9;
}

// ── State ────────────────────────────────────────────────────────────

static SETUP_COMPLETE: AtomicBool = AtomicBool::new(false);
static FIRST_BOOT: AtomicBool = AtomicBool::new(true);

struct WizardState {
    window_id: WindowId,
    step: SetupStep,
    language_idx: usize,
    keyboard_idx: usize,
    timezone_idx: usize,
    username: String,
    full_name: String,
    password: String,
    hostname: String,
    crash_reports: bool,
    usage_stats: bool,
    dark_theme: bool,
    accent_idx: u8,
}

impl WizardState {
    fn new(wid: WindowId) -> Self {
        Self {
            window_id: wid,
            step: SetupStep::Welcome,
            language_idx: 0,
            keyboard_idx: 0,
            timezone_idx: 0,
            username: String::new(),
            full_name: String::new(),
            password: String::new(),
            hostname: String::from("knoxos"),
            crash_reports: true,
            usage_stats: false,
            dark_theme: true,
            accent_idx: 0,
        }
    }
}

lazy_static! {
    static ref STATES: Mutex<Vec<WizardState>> = Mutex::new(Vec::new());
}

// ── Reference data ───────────────────────────────────────────────────

const LANGUAGES: &[(&str, &str)] = &[
    ("English", "US"),
    ("English", "GB"),
    ("Deutsch", "DE"),
    ("Français", "FR"),
    ("Español", "ES"),
    ("Italiano", "IT"),
    ("Português", "BR"),
    ("日本語", "JP"),
    ("한국어", "KR"),
    ("中文", "CN"),
    ("Русский", "RU"),
    ("العربية", "SA"),
];

const KEYBOARDS: &[&str] = &[
    "US",
    "US International",
    "UK",
    "German",
    "French (AZERTY)",
    "Spanish",
    "Italian",
    "Japanese",
    "Korean",
    "Russian",
];

const TIMEZONES: &[&str] = &[
    "UTC",
    "US/Eastern",
    "US/Central",
    "US/Mountain",
    "US/Pacific",
    "Europe/London",
    "Europe/Berlin",
    "Europe/Paris",
    "Europe/Moscow",
    "Asia/Tokyo",
    "Asia/Shanghai",
    "Asia/Seoul",
    "Asia/Kolkata",
    "Australia/Sydney",
    "Pacific/Auckland",
];

const ACCENT_COLORS: &[Pixel] = &[
    Pixel::from_hex(0x4A90D9),
    Pixel::from_hex(0x2ECC71),
    Pixel::from_hex(0xE74C3C),
    Pixel::from_hex(0xF39C12),
    Pixel::from_hex(0x9B59B6),
    Pixel::from_hex(0x1ABC9C),
    Pixel::from_hex(0xE67E22),
    Pixel::from_hex(0xE91E63),
];

// ── Public API ───────────────────────────────────────────────────────

pub fn open() {
    let mut win = window::Window::new("Setup Wizard", 140, 80, 520, 440);
    win.content_type = WindowContentType::SetupWizard;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    STATES.lock().push(WizardState::new(wid));
    super::request_redraw();
}

pub fn is_first_boot() -> bool {
    FIRST_BOOT.load(Ordering::SeqCst)
}
pub fn is_complete() -> bool {
    SETUP_COMPLETE.load(Ordering::SeqCst)
}

pub fn init() {
    crate::serial_println!(
        "[setup_wizard] initialized (first_boot={})",
        FIRST_BOOT.load(Ordering::Relaxed)
    );
}

// ── Drawing ──────────────────────────────────────────────────────────

pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, area: Rect, _scroll_y: i32) {
    let tc = theme::colors();
    let accent = colors::accent();
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let x0 = area.x;
    let y0 = area.y;
    let step = state.step;

    // ── Progress dots ──
    let dot_y = y0 + 12;
    let dot_spacing = 18;
    let dots_start = x0 + (area.width as i32 - (SetupStep::TOTAL as i32 * dot_spacing)) / 2;
    for i in 0..SetupStep::TOTAL {
        let dx = dots_start + (i as i32) * dot_spacing;
        let (r, col) = if i <= step.index() {
            (4, accent)
        } else {
            (3, tc.bg_tertiary)
        };
        fb.fill_circle_aa(dx, dot_y, r, col);
    }

    // ── Title ──
    let title = step.title();
    let tw = fonts::measure_string_width_compact(title, 2) as i32;
    fonts::draw_string_compact(
        fb,
        x0 + (area.width as i32 - tw) / 2,
        y0 + 28,
        title,
        tc.text_primary,
        2,
    );

    // ── Content ──
    let cy = y0 + 58;
    let cw = area.width;
    let ch = area.height.saturating_sub(100);

    match step {
        SetupStep::Welcome => draw_welcome(fb, x0, cy, cw, &tc),
        SetupStep::Language => draw_language(fb, x0, cy, cw, ch, state, &tc),
        SetupStep::Keyboard => draw_keyboard(fb, x0, cy, cw, ch, state, &tc),
        SetupStep::Timezone => draw_timezone(fb, x0, cy, cw, ch, state, &tc),
        SetupStep::Network => draw_network(fb, x0, cy, cw, ch, &tc),
        SetupStep::UserAccount => draw_user_account(fb, x0, cy, cw, state, &tc),
        SetupStep::Privacy => draw_privacy(fb, x0, cy, cw, state, &tc),
        SetupStep::Theme => draw_theme(fb, x0, cy, cw, state, &tc, accent),
        SetupStep::Complete => draw_complete(fb, x0, cy, cw, state, &tc),
    }

    // ── Navigation buttons ──
    let btn_y = y0 + area.height as i32 - 38;

    if step.index() > 0 && step != SetupStep::Complete {
        fb.fill_rounded_rect_aa(Rect::new(x0 + 16, btn_y, 72, 26), tc.bg_tertiary, 5);
        fonts::draw_string_compact(fb, x0 + 32, btn_y + 7, "Back", tc.text_primary, 1);
    }

    if step != SetupStep::Complete {
        let label = if step == SetupStep::Network {
            "Skip"
        } else {
            "Next"
        };
        let bx = x0 + area.width as i32 - 88;
        fb.fill_rounded_rect_aa(Rect::new(bx, btn_y, 72, 26), accent, 5);
        fonts::draw_string_compact(fb, bx + 16, btn_y + 7, label, Pixel::rgb(255, 255, 255), 1);
    } else {
        let bw: u32 = 170;
        let bx = x0 + (area.width as i32 - bw as i32) / 2;
        fb.fill_rounded_rect_aa(Rect::new(bx, btn_y, bw, 30), accent, 8);
        fonts::draw_string_compact(
            fb,
            bx + 14,
            btn_y + 8,
            "Start Using KnoxOS",
            Pixel::rgb(255, 255, 255),
            1,
        );
    }
}

fn draw_welcome(fb: &mut FrameBuffer, x0: i32, y0: i32, w: u32, tc: &theme::ThemeColors) {
    let cx = x0 + w as i32 / 2;
    fb.fill_circle_aa(cx, y0 + 40, 34, colors::accent());
    fonts::draw_string_compact(fb, cx - 10, y0 + 26, "K", Pixel::rgb(255, 255, 255), 3);

    let msg = "Let's set up your new system";
    let mw = fonts::measure_string_width_compact(msg, 1) as i32;
    fonts::draw_string_compact(fb, cx - mw / 2, y0 + 88, msg, tc.text_secondary, 1);

    let msg2 = "This will only take a minute";
    let m2w = fonts::measure_string_width_compact(msg2, 1) as i32;
    fonts::draw_string_compact(fb, cx - m2w / 2, y0 + 108, msg2, tc.text_muted, 1);
}

fn draw_language(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    w: u32,
    h: u32,
    state: &WizardState,
    tc: &theme::ThemeColors,
) {
    let row_h: i32 = 28;
    for (i, (lang, region)) in LANGUAGES.iter().enumerate() {
        let ry = y0 + (i as i32) * row_h;
        if ry + row_h > y0 + h as i32 {
            break;
        }
        let sel = i == state.language_idx;
        if sel {
            fb.fill_rounded_rect_aa(
                Rect::new(x0 + 12, ry, w - 24, row_h as u32 - 2),
                colors::accent(),
                4,
            );
        }
        let label = format!("{} ({})", lang, region);
        let col = if sel {
            Pixel::rgb(255, 255, 255)
        } else {
            tc.text_primary
        };
        fonts::draw_string_compact(fb, x0 + 24, ry + 6, &label, col, 1);
    }
}

fn draw_keyboard(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    w: u32,
    h: u32,
    state: &WizardState,
    tc: &theme::ThemeColors,
) {
    let row_h: i32 = 28;
    for (i, layout) in KEYBOARDS.iter().enumerate() {
        let ry = y0 + (i as i32) * row_h;
        if ry + row_h > y0 + h as i32 {
            break;
        }
        let sel = i == state.keyboard_idx;
        if sel {
            fb.fill_rounded_rect_aa(
                Rect::new(x0 + 12, ry, w - 24, row_h as u32 - 2),
                colors::accent(),
                4,
            );
        }
        let col = if sel {
            Pixel::rgb(255, 255, 255)
        } else {
            tc.text_primary
        };
        fonts::draw_string_compact(fb, x0 + 24, ry + 6, layout, col, 1);
    }
}

fn draw_timezone(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    w: u32,
    h: u32,
    state: &WizardState,
    tc: &theme::ThemeColors,
) {
    let row_h: i32 = 26;
    for (i, tz) in TIMEZONES.iter().enumerate() {
        let ry = y0 + (i as i32) * row_h;
        if ry + row_h > y0 + h as i32 {
            break;
        }
        let sel = i == state.timezone_idx;
        if sel {
            fb.fill_rounded_rect_aa(
                Rect::new(x0 + 12, ry, w - 24, row_h as u32 - 2),
                colors::accent(),
                4,
            );
        }
        let col = if sel {
            Pixel::rgb(255, 255, 255)
        } else {
            tc.text_primary
        };
        fonts::draw_string_compact(fb, x0 + 24, ry + 5, tz, col, 1);
    }
}

fn draw_network(fb: &mut FrameBuffer, x0: i32, y0: i32, w: u32, _h: u32, tc: &theme::ThemeColors) {
    fonts::draw_string_compact(fb, x0 + 16, y0, "Available Networks:", tc.text_primary, 1);

    let networks = ["KnoxOS-Setup", "Home WiFi", "Office-5G", "Guest"];
    for (i, net) in networks.iter().enumerate() {
        let ry = y0 + 24 + (i as i32) * 32;
        fb.fill_rounded_rect_aa(Rect::new(x0 + 12, ry, w - 24, 28), tc.bg_surface, 4);

        // Signal bars
        for b in 0..4u32 {
            let bar_h = 4 + b * 3;
            let bx = x0 + 24 + (b as i32 * 5);
            let by = ry + 20 - bar_h as i32;
            let col = if b < 3 {
                colors::accent()
            } else {
                tc.bg_tertiary
            };
            fb.fill_rect(Rect::new(bx, by, 3, bar_h), col);
        }

        fonts::draw_string_compact(fb, x0 + 52, ry + 6, net, tc.text_primary, 1);
    }
}

fn draw_user_account(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    w: u32,
    state: &WizardState,
    tc: &theme::ThemeColors,
) {
    let fields: &[(&str, &str, &str)] = &[
        (
            "Full Name",
            if state.full_name.is_empty() {
                "Enter your name"
            } else {
                &state.full_name
            },
            "name",
        ),
        (
            "Username",
            if state.username.is_empty() {
                "username"
            } else {
                &state.username
            },
            "user",
        ),
        (
            "Password",
            if state.password.is_empty() {
                "Enter password"
            } else {
                "••••••••"
            },
            "pw",
        ),
        ("Computer Name", &state.hostname, "host"),
    ];

    for (i, &(label, value, hint)) in fields.iter().enumerate() {
        let fy = y0 + (i as i32) * 52;
        fonts::draw_string_compact(fb, x0 + 16, fy, label, tc.text_secondary, 1);
        fb.fill_rounded_rect_aa(Rect::new(x0 + 16, fy + 16, w - 32, 26), tc.bg_surface, 4);
        let is_placeholder = match hint {
            "name" => state.full_name.is_empty(),
            "user" => state.username.is_empty(),
            "pw" => state.password.is_empty(),
            _ => false,
        };
        let col = if is_placeholder {
            tc.text_muted
        } else {
            tc.text_primary
        };
        fonts::draw_string_compact(fb, x0 + 24, fy + 22, value, col, 1);
    }
}

fn draw_privacy(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    _w: u32,
    state: &WizardState,
    tc: &theme::ThemeColors,
) {
    fonts::draw_string_compact(fb, x0 + 16, y0, "Help improve KnoxOS", tc.text_primary, 1);

    // Crash reports toggle
    let ty = y0 + 28;
    draw_toggle(fb, x0 + 16, ty, state.crash_reports);
    fonts::draw_string_compact(
        fb,
        x0 + 56,
        ty + 1,
        "Send crash reports",
        tc.text_primary,
        1,
    );
    fonts::draw_string_compact(
        fb,
        x0 + 56,
        ty + 16,
        "Anonymous crash data to fix bugs",
        tc.text_secondary,
        1,
    );

    // Usage stats toggle
    let ty2 = y0 + 72;
    draw_toggle(fb, x0 + 16, ty2, state.usage_stats);
    fonts::draw_string_compact(
        fb,
        x0 + 56,
        ty2 + 1,
        "Share usage statistics",
        tc.text_primary,
        1,
    );
    fonts::draw_string_compact(
        fb,
        x0 + 56,
        ty2 + 16,
        "Anonymous feature usage data",
        tc.text_secondary,
        1,
    );

    fonts::draw_string_compact(
        fb,
        x0 + 16,
        y0 + 124,
        "Your data stays on device unless you opt in.",
        tc.text_muted,
        1,
    );
}

fn draw_theme(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    w: u32,
    state: &WizardState,
    tc: &theme::ThemeColors,
    accent: Pixel,
) {
    let card_w = (w - 40) / 2;

    // Light card
    let lx = x0 + 12;
    let lbg = if !state.dark_theme {
        accent
    } else {
        tc.bg_surface
    };
    fb.fill_rounded_rect_aa(Rect::new(lx, y0, card_w, 62), lbg, 6);
    fb.fill_rect(
        Rect::new(lx + 8, y0 + 8, card_w - 16, 28),
        Pixel::from_hex(0xF5F5F5),
    );
    let lc = if !state.dark_theme {
        Pixel::rgb(255, 255, 255)
    } else {
        tc.text_primary
    };
    fonts::draw_string_compact(fb, lx + 12, y0 + 44, "Light", lc, 1);

    // Dark card
    let dx = x0 + 28 + card_w as i32;
    let dbg = if state.dark_theme {
        accent
    } else {
        tc.bg_surface
    };
    fb.fill_rounded_rect_aa(Rect::new(dx, y0, card_w, 62), dbg, 6);
    fb.fill_rect(
        Rect::new(dx + 8, y0 + 8, card_w - 16, 28),
        Pixel::from_hex(0x1E1E2E),
    );
    let dc = if state.dark_theme {
        Pixel::rgb(255, 255, 255)
    } else {
        tc.text_primary
    };
    fonts::draw_string_compact(fb, dx + 12, y0 + 44, "Dark", dc, 1);

    // Accent color palette
    fonts::draw_string_compact(fb, x0 + 12, y0 + 76, "Accent Color", tc.text_primary, 1);
    for (i, &col) in ACCENT_COLORS.iter().enumerate() {
        let cx = x0 + 20 + (i as i32) * 34;
        let cy = y0 + 96;
        fb.fill_circle_aa(cx, cy, 12, col);
        if i as u8 == state.accent_idx {
            fb.fill_circle_aa(cx, cy, 5, Pixel::rgb(255, 255, 255));
        }
    }
}

fn draw_complete(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    w: u32,
    state: &WizardState,
    tc: &theme::ThemeColors,
) {
    let cx = x0 + w as i32 / 2;
    fb.fill_circle_aa(cx, y0 + 36, 28, Pixel::from_hex(0x2ECC71));
    // Checkmark approximated as "OK" label
    fonts::draw_string_compact(fb, cx - 10, y0 + 24, "OK", Pixel::rgb(255, 255, 255), 2);

    let msg = "Your KnoxOS is ready!";
    let mw = fonts::measure_string_width_compact(msg, 2) as i32;
    fonts::draw_string_compact(fb, cx - mw / 2, y0 + 76, msg, tc.text_primary, 2);

    let who = if state.full_name.is_empty() {
        &state.username
    } else {
        &state.full_name
    };
    if !who.is_empty() {
        let greeting = format!("Welcome, {}!", who);
        let gw = fonts::measure_string_width_compact(&greeting, 1) as i32;
        fonts::draw_string_compact(fb, cx - gw / 2, y0 + 100, &greeting, tc.text_secondary, 1);
    }
}

fn draw_toggle(fb: &mut FrameBuffer, x: i32, y: i32, on: bool) {
    let bg = if on {
        colors::accent()
    } else {
        Pixel::from_hex(0x555555)
    };
    fb.fill_rounded_rect_aa(Rect::new(x, y, 32, 16), bg, 8);
    let knob_x = if on { x + 18 } else { x + 4 };
    fb.fill_circle_aa(knob_x + 5, y + 8, 5, Pixel::rgb(255, 255, 255));
}

// ── Click handling ───────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, mx: i32, my: i32) {
    let area = {
        let wm = window::WINDOW_MANAGER.lock();
        match wm.windows.iter().find(|w| w.id == wid) {
            Some(w) => w.content_rect(),
            None => return,
        }
    };
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let step = state.step;
    let btn_y = area.y + area.height as i32 - 38;

    // Back button
    if step.index() > 0
        && step != SetupStep::Complete
        && mx >= area.x + 16
        && mx <= area.x + 88
        && my >= btn_y
        && my <= btn_y + 26
    {
        state.step = SetupStep::from_index(step.index() - 1);
        super::request_redraw();
        return;
    }

    // Next/Skip button
    let next_x = area.x + area.width as i32 - 88;
    if step != SetupStep::Complete
        && mx >= next_x
        && mx <= next_x + 72
        && my >= btn_y
        && my <= btn_y + 26
    {
        let next = step.index() + 1;
        if next < SetupStep::TOTAL {
            state.step = SetupStep::from_index(next);
        }
        super::request_redraw();
        return;
    }

    // "Start Using KnoxOS"
    if step == SetupStep::Complete {
        let bw = 170i32;
        let bx = area.x + (area.width as i32 - bw) / 2;
        if mx >= bx && mx <= bx + bw && my >= btn_y && my <= btn_y + 30 {
            apply_settings(state);
            SETUP_COMPLETE.store(true, Ordering::SeqCst);
            FIRST_BOOT.store(false, Ordering::SeqCst);
            crate::serial_println!("[setup_wizard] setup complete!");
            super::request_redraw();
            return;
        }
    }

    // Step-specific clicks
    let cy = area.y + 58;
    match step {
        SetupStep::Language => {
            let row_h = 28;
            let rel = my - cy;
            if rel >= 0 {
                let i = (rel / row_h) as usize;
                if i < LANGUAGES.len() {
                    state.language_idx = i;
                    super::request_redraw();
                }
            }
        }
        SetupStep::Keyboard => {
            let row_h = 28;
            let rel = my - cy;
            if rel >= 0 {
                let i = (rel / row_h) as usize;
                if i < KEYBOARDS.len() {
                    state.keyboard_idx = i;
                    super::request_redraw();
                }
            }
        }
        SetupStep::Timezone => {
            let row_h = 26;
            let rel = my - cy;
            if rel >= 0 {
                let i = (rel / row_h) as usize;
                if i < TIMEZONES.len() {
                    state.timezone_idx = i;
                    super::request_redraw();
                }
            }
        }
        SetupStep::Privacy => {
            if my >= cy + 28 && my <= cy + 44 && mx >= area.x + 16 && mx <= area.x + 48 {
                state.crash_reports = !state.crash_reports;
                super::request_redraw();
            }
            if my >= cy + 72 && my <= cy + 88 && mx >= area.x + 16 && mx <= area.x + 48 {
                state.usage_stats = !state.usage_stats;
                super::request_redraw();
            }
        }
        SetupStep::Theme => {
            let card_w = (area.width - 40) / 2;
            // Light/Dark
            if my >= cy && my <= cy + 62 {
                if mx >= area.x + 12 && mx <= area.x + 12 + card_w as i32 {
                    state.dark_theme = false;
                    super::request_redraw();
                } else if mx >= area.x + 28 + card_w as i32 {
                    state.dark_theme = true;
                    super::request_redraw();
                }
            }
            // Accent colors
            if my >= cy + 84 && my <= cy + 108 {
                for i in 0..ACCENT_COLORS.len() {
                    let cx = area.x + 20 + (i as i32) * 34;
                    if mx >= cx - 12 && mx <= cx + 12 {
                        state.accent_idx = i as u8;
                        super::request_redraw();
                        break;
                    }
                }
            }
        }
        _ => {}
    }
}

fn apply_settings(state: &WizardState) {
    let (lang, region) = LANGUAGES[state.language_idx];
    crate::serial_println!("[setup] language: {} ({})", lang, region);
    crate::serial_println!("[setup] keyboard: {}", KEYBOARDS[state.keyboard_idx]);
    crate::serial_println!("[setup] timezone: {}", TIMEZONES[state.timezone_idx]);

    if !state.username.is_empty() {
        crate::pam::add_user(&state.username, 1000, 1000, "/home/knox", "/bin/sh");
        if !state.password.is_empty() {
            crate::pam::set_password(&state.username, &state.password);
        }
        crate::serial_println!("[setup] user: {}", state.username);
    }

    crate::serial_println!("[setup] hostname: {}", state.hostname);
    crate::crash_reporter::set_enabled(state.crash_reports);
    crate::serial_println!(
        "[setup] theme: {} accent={}",
        if state.dark_theme { "dark" } else { "light" },
        state.accent_idx
    );
}
