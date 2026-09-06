/// Settings Panel — Rendered inside a Window with WindowContentType::Settings
/// Provides system configuration tabs: Display, Sound, Network, System, About.
use alloc::string::String;
use alloc::vec::Vec;

use super::colors;
use super::font_engine;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};

/// Active tab in settings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Display,
    Sound,
    Network,
    Personalization,
    Users,
    System,
    DateTime,
    Privacy,
    Startup,
    About,
}

/// Settings panel state (per window instance)
pub struct SettingsState {
    pub active_tab: SettingsTab,
    pub scroll_y: i32,
    /// Keyboard focus index: -1 = sidebar tabs area, 0..N = content items
    /// When focus_index < 0 the sidebar is focused; Tab_index in sidebar = -(focus_index+1)
    pub focus_index: i32,
    /// Whether keyboard navigation is active (show focus rings)
    pub keyboard_nav: bool,
}

impl SettingsState {
    pub fn new() -> Self {
        SettingsState {
            active_tab: SettingsTab::Display,
            scroll_y: 0,
            focus_index: -1,
            keyboard_nav: false,
        }
    }

    /// Number of focusable items in the sidebar (always 7 tabs)
    pub fn sidebar_count(&self) -> i32 {
        TABS.len() as i32
    }

    /// Number of focusable content items in the current tab
    pub fn content_item_count(&self) -> i32 {
        match self.active_tab {
            SettingsTab::Display => {
                // Resolution options + brightness slider
                super::RESOLUTIONS.len() as i32 + 1
            }
            SettingsTab::Sound => {
                // Master volume + system sounds sliders + 3 output devices
                5
            }
            SettingsTab::Network => {
                // Wi-Fi toggle + 3 networks
                4
            }
            SettingsTab::Personalization => {
                // 4 themes + 6 accents + 2 toggles
                12
            }
            SettingsTab::Users => {
                // Lock Screen + Log Out buttons
                2
            }
            SettingsTab::System => {
                // Power saving toggle + 6 keyboard layouts
                7
            }
            SettingsTab::DateTime => {
                // NTP toggle + 24h toggle + seconds toggle + 12 timezone entries
                15
            }
            SettingsTab::Privacy => {
                // 6 privacy toggles (location, analytics, camera, mic, firewall, autoupdate)
                6
            }
            SettingsTab::Startup => {
                // startup apps (toggle each)
                let count = super::settings_ext::STARTUP.lock().apps.len();
                count as i32
            }
            SettingsTab::About => {
                // No interactive items
                0
            }
        }
    }
}

lazy_static::lazy_static! {
    /// Global settings state — shared between draw and click handlers
    pub static ref SETTINGS_STATE: spin::Mutex<SettingsState> =
        spin::Mutex::new(SettingsState::new());
}

/// Tabs definition
const TABS: &[(SettingsTab, &str, &str)] = &[
    (SettingsTab::Display, "Display", "D"),
    (SettingsTab::Sound, "Sound", "S"),
    (SettingsTab::Network, "Network", "N"),
    (SettingsTab::Personalization, "Personalization", "P"),
    (SettingsTab::Users, "Users", "U"),
    (SettingsTab::System, "System", "G"),
    (SettingsTab::DateTime, "Date & Time", "T"),
    (SettingsTab::Privacy, "Privacy", "🔒"),
    (SettingsTab::Startup, "Startup Apps", "⚡"),
    (SettingsTab::About, "About", "i"),
];

const SIDEBAR_WIDTH: i32 = 160;
const TAB_HEIGHT: i32 = 36;
/// Width of the scrollbar track in the content area
const SCROLLBAR_WIDTH: i32 = 8;

/// Draw settings content inside a window's content area.
/// `scroll_y` is the window's scroll offset for the content pane.
/// Returns the total virtual content height (used by the caller to set max_scroll_y).
pub fn draw_settings(
    fb: &mut FrameBuffer,
    content_rect: Rect,
    state: &SettingsState,
    scroll_y: i32,
) -> i32 {
    let cx = content_rect.x;
    let cy = content_rect.y;
    let cw = content_rect.width;
    let ch = content_rect.height;

    // Background
    fb.fill_rect(content_rect, Pixel::rgb(24, 24, 28));

    // ── Sidebar ──────────────────────────
    fb.fill_rect(
        Rect::new(cx, cy, SIDEBAR_WIDTH as u32, ch),
        Pixel::rgb(30, 30, 34),
    );

    // Title
    font_engine::draw_ui_bold(fb, cx + 16, cy + 12, "Settings", 16, colors::WHITE);
    fb.draw_hline(
        cx + 12,
        cy + 34,
        (SIDEBAR_WIDTH - 24) as u32,
        Pixel::rgb(50, 50, 55),
    );

    // Tab items
    for (i, (tab, label, icon)) in TABS.iter().enumerate() {
        let ty = cy + 44 + i as i32 * TAB_HEIGHT;
        let is_active = state.active_tab == *tab;

        if is_active {
            fb.fill_rounded_rect_aa(
                Rect::new(
                    cx + 6,
                    ty,
                    (SIDEBAR_WIDTH - 12) as u32,
                    TAB_HEIGHT as u32 - 4,
                ),
                Pixel::rgb(55, 55, 60),
                4,
            );
            // Active indicator bar
            fb.fill_rounded_rect_aa(
                Rect::new(cx + 4, ty + 6, 3, (TAB_HEIGHT - 16) as u32),
                Pixel::rgb(82, 139, 255),
                1,
            );
        }

        // Icon circle
        let icon_bg = if is_active {
            Pixel::rgb(82, 139, 255)
        } else {
            Pixel::rgb(50, 50, 55)
        };
        fb.fill_circle_aa(cx + 24, ty + TAB_HEIGHT / 2 - 2, 10, icon_bg);
        fonts::draw_char_bold_compact(
            fb,
            cx + 20,
            ty + TAB_HEIGHT / 2 - 8,
            icon.chars().next().unwrap_or('?'),
            colors::WHITE,
            1,
        );

        let text_color = if is_active {
            colors::WHITE
        } else {
            Pixel::rgb(160, 160, 160)
        };
        if is_active {
            font_engine::draw_ui_bold(
                fb,
                cx + 42,
                ty + (TAB_HEIGHT - 13) / 2,
                label,
                13,
                text_color,
            );
        } else {
            font_engine::draw_ui_text(
                fb,
                cx + 42,
                ty + (TAB_HEIGHT - 13) / 2,
                label,
                13,
                text_color,
            );
        }

        // Draw focus ring on sidebar tab if keyboard nav is active
        if state.keyboard_nav {
            if let FocusRegion::Sidebar(fi) = state.focus_region() {
                if fi == i as i32 {
                    let tab_rect = Rect::new(
                        cx + 6,
                        ty,
                        (SIDEBAR_WIDTH - 12) as u32,
                        TAB_HEIGHT as u32 - 4,
                    );
                    draw_focus_indicator(fb, tab_rect, true);
                }
            }
        }
    }

    // ── Content area (scrollable) ─────────────────────
    let content_x = cx + SIDEBAR_WIDTH;
    let content_y = cy;
    let content_w = cw as i32 - SIDEBAR_WIDTH;
    let content_h = ch;

    // Vertical separator
    fb.draw_vline(content_x, cy, ch, Pixel::rgb(45, 45, 50));

    // Clip content to the pane right of the sidebar
    let content_clip = Rect::new(content_x, content_y, content_w as u32, content_h);
    fb.push_clip(content_clip);

    // Determine which content item is focused (-1 if sidebar or nav inactive)
    let focused_content = if state.keyboard_nav {
        match state.focus_region() {
            FocusRegion::Content(idx) => idx,
            _ => -1,
        }
    } else {
        -1
    };

    // Draw current tab with scroll offset applied
    let total_h = match state.active_tab {
        SettingsTab::Display => draw_display_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Sound => draw_sound_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Network => draw_network_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Personalization => draw_personalization_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Users => draw_users_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::System => draw_system_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::DateTime => draw_datetime_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Privacy => draw_privacy_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::Startup => draw_startup_tab(
            fb,
            content_x,
            content_y - scroll_y,
            content_w,
            focused_content,
        ),
        SettingsTab::About => draw_about_tab(fb, content_x, content_y - scroll_y, content_w),
    };

    fb.pop_clip();

    // ── Scrollbar (drawn on top, inside content pane) ──────────────
    let visible_h = content_h as i32;
    if total_h > visible_h {
        let sb_x = cx + cw as i32 - SCROLLBAR_WIDTH - 2;
        let sb_track = Rect::new(
            sb_x,
            content_y + 2,
            SCROLLBAR_WIDTH as u32,
            content_h.saturating_sub(4),
        );
        fb.fill_rounded_rect_aa(
            sb_track,
            colors::SCROLLBAR_TRACK,
            (SCROLLBAR_WIDTH / 2) as u32,
        );

        let visible_ratio = visible_h as f32 / total_h as f32;
        let thumb_h = ((visible_ratio * visible_h as f32) as u32)
            .max(20)
            .min(content_h.saturating_sub(4));
        let max_scroll = (total_h - visible_h).max(1);
        let scroll_ratio = scroll_y as f32 / max_scroll as f32;
        let track_space = content_h.saturating_sub(4 + thumb_h) as f32;
        let thumb_y = content_y + 2 + (scroll_ratio.clamp(0.0, 1.0) * track_space) as i32;

        fb.fill_rounded_rect_aa(
            Rect::new(sb_x + 1, thumb_y, (SCROLLBAR_WIDTH - 2) as u32, thumb_h),
            colors::SCROLLBAR_THUMB,
            ((SCROLLBAR_WIDTH - 2) / 2) as u32,
        );
    }

    total_h
}

// ─── TrueType font helpers ──────────────────────────────
/// Regular proportional text at 13px (replaces draw_string_compact at scale 1)
#[inline]
fn ttf(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    font_engine::draw_ui_text(fb, x, y, text, 13, color);
}

/// Bold proportional text at 13px (replaces draw_string_bold_compact at scale 1)
#[inline]
fn ttf_b(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    font_engine::draw_ui_bold(fb, x, y, text, 13, color);
}

/// Bold header text at 15px (replaces draw_string_bold at scale 1)
#[inline]
fn ttf_h(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    font_engine::draw_ui_bold(fb, x, y, text, 15, color);
}

/// Draw text centered horizontally in a region
fn ttf_centered(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    region_w: u32,
    text: &str,
    size: u16,
    color: Pixel,
) {
    let tw = font_engine::measure_ui_text(text, size) as i32;
    let cx = x + (region_w as i32 - tw) / 2;
    font_engine::draw_ui_text(fb, cx, y, text, size, color);
}

/// Draw bold text centered horizontally in a region
fn ttf_centered_b(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    region_w: u32,
    text: &str,
    size: u16,
    color: Pixel,
) {
    let tw = font_engine::measure_ui_text(text, size) as i32;
    let cx = x + (region_w as i32 - tw) / 2;
    font_engine::draw_ui_bold(fb, cx, y, text, size, color);
}

// ─── Helper: Section Header ──────────────────────────────
fn draw_section_header(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, title: &str) {
    font_engine::draw_ui_bold(fb, x + 20, y, title, 15, colors::WHITE);
    fb.draw_hline(x + 16, y + 18, (w - 32) as u32, Pixel::rgb(50, 50, 55));
}

// ─── Helper: Toggle Row ─────────────────────────────────
fn draw_toggle_row(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    label: &str,
    sublabel: &str,
    enabled: bool,
) {
    font_engine::draw_ui_text(fb, x + 24, y, label, 13, colors::WHITE);
    if !sublabel.is_empty() {
        font_engine::draw_ui_text(fb, x + 24, y + 16, sublabel, 11, Pixel::rgb(120, 120, 120));
    }

    // Toggle switch
    let toggle_x = x + w - 60;
    let bg = if enabled {
        Pixel::rgb(82, 139, 255)
    } else {
        Pixel::rgb(60, 60, 65)
    };
    fb.fill_rounded_rect_aa(Rect::new(toggle_x, y + 2, 36, 18), bg, 9);
    let knob_x = if enabled { toggle_x + 20 } else { toggle_x + 4 };
    fb.fill_circle_aa(knob_x + 6, y + 11, 7, colors::WHITE);
}

// ─── Helper: Slider Row ─────────────────────────────────
fn draw_slider_row(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    label: &str,
    value: u8,
    color: Pixel,
) {
    font_engine::draw_ui_text(fb, x + 24, y, label, 13, colors::WHITE);
    let val_str = alloc::format!("{}%", value);
    let val_x = x + w - 60;
    font_engine::draw_ui_text(fb, val_x, y, &val_str, 13, Pixel::rgb(140, 140, 140));

    // Slider bar
    let bar_y = y + 18;
    let bar_w = (w - 52) as u32;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 24, bar_y, bar_w, 6),
        Pixel::rgb(50, 50, 55),
        3,
    );
    let fill_w = (bar_w * value as u32) / 100;
    if fill_w > 0 {
        fb.fill_rounded_rect_aa(Rect::new(x + 24, bar_y, fill_w, 6), color, 3);
    }
    fb.fill_circle_aa(x + 24 + fill_w as i32, bar_y + 3, 7, color);
    fb.fill_circle_aa(x + 24 + fill_w as i32, bar_y + 3, 4, colors::WHITE);
}

// ─── Helper: Info Row ───────────────────────────────────
fn draw_info_row(fb: &mut FrameBuffer, x: i32, y: i32, label: &str, value: &str) {
    font_engine::draw_ui_text(fb, x + 24, y, label, 13, Pixel::rgb(140, 140, 140));
    font_engine::draw_ui_text(fb, x + 160, y, value, 13, colors::WHITE);
}

// ═══════════════════════════════════════════════════════════════════════════
// DISPLAY TAB
// ═══════════════════════════════════════════════════════════════════════════

/// Height of each resolution option row
const RES_ROW_HEIGHT: i32 = 28;
/// X offset for the resolution list within the display tab
const RES_LIST_X_PAD: i32 = 24;
/// Starting Y offset for the resolution list (relative to content_y)
pub const RES_LIST_Y_START: i32 = 112;

fn draw_display_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Display");

    // Current resolution info
    let res_str = alloc::format!("{}x{}", fb.width, fb.height);
    draw_info_row(fb, x, y + 42, "Resolution", &res_str);
    draw_info_row(fb, x, y + 60, "Color Depth", "32-bit (BGRA)");
    draw_info_row(fb, x, y + 78, "Refresh", "Software Rendered");

    // ── Resolution picker ────────────────
    draw_section_header(fb, x, y + 92, w, "Change Resolution");

    let current_w = fb.width;
    let current_h = fb.height;

    for (i, &(rw, rh, label)) in super::RESOLUTIONS.iter().enumerate() {
        let row_y = y + RES_LIST_Y_START + i as i32 * RES_ROW_HEIGHT;
        let is_active = rw == current_w && rh == current_h;

        // Background highlight for active / hover-zone
        let btn_rect = Rect::new(
            x + RES_LIST_X_PAD,
            row_y,
            (w - RES_LIST_X_PAD * 2) as u32,
            RES_ROW_HEIGHT as u32 - 4,
        );
        if is_active {
            fb.fill_rounded_rect_aa(btn_rect, Pixel::rgb(40, 60, 100), 4);
        } else {
            fb.fill_rounded_rect_aa(btn_rect, Pixel::rgb(36, 36, 40), 4);
        }

        // Radio button circle
        let radio_x = x + RES_LIST_X_PAD + 14;
        let radio_y = row_y + RES_ROW_HEIGHT / 2 - 2;
        fb.fill_circle_aa(radio_x, radio_y, 7, Pixel::rgb(70, 70, 75));
        if is_active {
            fb.fill_circle_aa(radio_x, radio_y, 7, Pixel::rgb(82, 139, 255));
            fb.fill_circle_aa(radio_x, radio_y, 4, colors::WHITE);
        }

        // Label text
        let text_color = if is_active {
            colors::WHITE
        } else {
            Pixel::rgb(180, 180, 180)
        };
        font_engine::draw_ui_text(
            fb,
            x + RES_LIST_X_PAD + 30,
            row_y + 5,
            label,
            13,
            text_color,
        );

        // Focus ring for this resolution option
        draw_focus_indicator(fb, btn_rect, focus_idx == i as i32);
    }

    // Brightness slider (below the list)
    let res_count = super::RESOLUTIONS.len() as i32;
    let after_list_y = y + RES_LIST_Y_START + res_count * RES_ROW_HEIGHT + 10;
    fb.draw_hline(
        x + 16,
        after_list_y,
        (w - 32) as u32,
        Pixel::rgb(45, 45, 50),
    );
    draw_slider_row(
        fb,
        x,
        after_list_y + 12,
        w,
        "Brightness",
        80,
        Pixel::rgb(224, 175, 104),
    );

    // Focus ring for brightness slider
    let slider_rect = Rect::new(x + 24, after_list_y + 12, (w - 52) as u32, 28);
    draw_focus_indicator(fb, slider_rect, focus_idx == res_count);

    // Scale info
    draw_section_header(fb, x, after_list_y + 52, w, "Scale & Layout");
    draw_info_row(fb, x, after_list_y + 82, "Scale", "100% (Recommended)");
    draw_info_row(fb, x, after_list_y + 100, "Orientation", "Landscape");

    // Return total content height
    after_list_y - y + 130
}

// ═══════════════════════════════════════════════════════════════════════════
// SOUND TAB
// ═══════════════════════════════════════════════════════════════════════════
fn draw_sound_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Sound");

    draw_slider_row(
        fb,
        x,
        y + 42,
        w,
        "Master Volume",
        75,
        Pixel::rgb(82, 139, 255),
    );
    // Focus: item 0 = master volume
    draw_focus_indicator(
        fb,
        Rect::new(x + 24, y + 42, (w - 52) as u32, 28),
        focus_idx == 0,
    );

    draw_slider_row(
        fb,
        x,
        y + 82,
        w,
        "System Sounds",
        50,
        Pixel::rgb(82, 139, 255),
    );
    // Focus: item 1 = system sounds
    draw_focus_indicator(
        fb,
        Rect::new(x + 24, y + 82, (w - 52) as u32, 28),
        focus_idx == 1,
    );

    fb.draw_hline(x + 16, y + 118, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 128, w, "Output Device");

    // Device list
    let devices = ["HD Audio Output (Default)", "HDMI Audio", "USB Headset"];
    for (i, device) in devices.iter().enumerate() {
        let dy = y + 158 + i as i32 * 30;
        let is_selected = i == 0;
        if is_selected {
            fb.fill_rounded_rect_aa(
                Rect::new(x + 20, dy - 4, (w - 40) as u32, 26),
                Pixel::rgb(45, 45, 50),
                4,
            );
        }
        let radio_color = if is_selected {
            Pixel::rgb(82, 139, 255)
        } else {
            Pixel::rgb(70, 70, 75)
        };
        fb.fill_circle_aa(x + 36, dy + 6, 6, radio_color);
        if is_selected {
            fb.fill_circle_aa(x + 36, dy + 6, 3, Pixel::rgb(82, 139, 255));
        }
        ttf(fb, x + 52, dy, device, colors::WHITE);
        // Focus: items 2..4 = output devices
        let dev_rect = Rect::new(x + 20, dy - 4, (w - 40) as u32, 26);
        draw_focus_indicator(fb, dev_rect, focus_idx == (i as i32 + 2));
    }

    280
}

// ═══════════════════════════════════════════════════════════════════════════
// NETWORK TAB
// ═══════════════════════════════════════════════════════════════════════════
fn draw_network_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Network");

    draw_toggle_row(fb, x, y + 42, w, "Wi-Fi", "Connected to KnoxOS-Net", true);
    // Focus: item 0 = Wi-Fi toggle
    draw_focus_indicator(fb, Rect::new(x + w - 60, y + 42, 36, 18), focus_idx == 0);

    fb.draw_hline(x + 16, y + 82, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 92, w, "Available Networks");

    let networks = [
        ("KnoxOS-Net", "Connected", true),
        ("Office-5G", "Secured", false),
        ("Guest", "Open", false),
    ];

    for (i, (name, status, connected)) in networks.iter().enumerate() {
        let ny = y + 120 + i as i32 * 40;
        if *connected {
            fb.fill_rounded_rect_aa(
                Rect::new(x + 20, ny - 2, (w - 40) as u32, 34),
                Pixel::rgb(45, 45, 50),
                4,
            );
        }

        // WiFi signal bars
        let icon_x = x + 30;
        for bar in 0..4u32 {
            let bh = 4 + bar * 3;
            let color = if *connected || bar < 2 {
                Pixel::rgb(158, 206, 106)
            } else {
                Pixel::rgb(60, 60, 65)
            };
            fb.fill_rounded_rect_aa(
                Rect::new(icon_x + bar as i32 * 5, ny + 16 - bh as i32, 3, bh),
                color,
                1,
            );
        }

        ttf(fb, x + 56, ny + 2, name, colors::WHITE);
        ttf(fb, x + 56, ny + 16, status, Pixel::rgb(120, 120, 120));
        // Focus: items 1..3 = network entries
        let net_rect = Rect::new(x + 20, ny - 2, (w - 40) as u32, 34);
        draw_focus_indicator(fb, net_rect, focus_idx == (i as i32 + 1));
    }

    fb.draw_hline(x + 16, y + 248, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_info_row(fb, x, y + 260, "IP Address", "10.0.2.15");
    draw_info_row(fb, x, y + 278, "MAC Address", "52:54:00:12:34:56");

    310
}

// ═══════════════════════════════════════════════════════════════════════════
// PERSONALIZATION TAB
// ═══════════════════════════════════════════════════════════════════════════
fn draw_personalization_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Personalization");

    ttf_b(fb, x + 24, y + 42, "Theme", Pixel::rgb(140, 140, 140));

    // Theme swatches — use actual theme system
    let active_theme = super::theme::active_theme();
    let themes: [(super::theme::ThemeId, &str, Pixel); 3] = [
        (
            super::theme::ThemeId::NebulaDark,
            "Nebula Dark",
            Pixel::rgb(8, 12, 24),
        ),
        (
            super::theme::ThemeId::ArcticLight,
            "Arctic Light",
            Pixel::rgb(245, 248, 252),
        ),
        (
            super::theme::ThemeId::SunsetWarm,
            "Sunset Warm",
            Pixel::rgb(28, 20, 16),
        ),
    ];

    for (i, (theme_id, name, color)) in themes.iter().enumerate() {
        let sx = x + 24 + i as i32 * 82;
        let sy = y + 60;
        fb.fill_rounded_rect_aa(Rect::new(sx, sy, 72, 36), *color, 4);
        if active_theme == *theme_id {
            let accent = super::theme::accent_color();
            fb.draw_rounded_rect(Rect::new(sx - 1, sy - 1, 74, 38), accent, 4, 2);
        }
        let label_color = if active_theme == *theme_id {
            super::theme::accent_color()
        } else {
            Pixel::rgb(120, 120, 120)
        };
        ttf(fb, sx + 2, sy + 40, name, label_color);
    }

    // Accent color
    fb.draw_hline(x + 16, y + 115, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    ttf_b(
        fb,
        x + 24,
        y + 125,
        "Accent Color",
        Pixel::rgb(140, 140, 140),
    );

    let current_accent = super::theme::accent_index();
    for (i, color) in super::theme::ACCENT_COLORS.iter().enumerate() {
        let ax = x + 24 + i as i32 * 32;
        let ay = y + 145;
        if i as u8 == current_accent {
            fb.fill_circle_aa(ax + 10, ay + 10, 12, colors::WHITE);
        }
        fb.fill_circle_aa(ax + 10, ay + 10, 10, *color);
    }

    fb.draw_hline(x + 16, y + 178, (w - 32) as u32, Pixel::rgb(45, 45, 50));

    // Cursor Theme
    ttf_b(
        fb,
        x + 24,
        y + 188,
        "Cursor Style",
        Pixel::rgb(140, 140, 140),
    );

    let current_cursor = super::desktop::cursor_theme();
    let cursor_themes: [(&str, super::desktop::CursorTheme); 4] = [
        ("White", super::desktop::CursorTheme::Default),
        ("Dark", super::desktop::CursorTheme::Dark),
        ("Accent", super::desktop::CursorTheme::Accent),
        ("Large", super::desktop::CursorTheme::Large),
    ];

    for (i, (name, theme)) in cursor_themes.iter().enumerate() {
        let bx = x + 24 + i as i32 * 55;
        let by = y + 206;
        let is_active = current_cursor == *theme;
        let bg = if is_active {
            Pixel::new(60, 80, 120, 180)
        } else {
            Pixel::rgb(40, 42, 48)
        };
        fb.fill_rounded_rect_aa(Rect::new(bx, by, 48, 22), bg, 4);
        if is_active {
            fb.draw_rounded_rect(
                Rect::new(bx - 1, by - 1, 50, 24),
                super::theme::accent_color(),
                4,
                1,
            );
        }
        let tc = if is_active {
            Pixel::rgb(220, 235, 255)
        } else {
            Pixel::rgb(120, 120, 120)
        };
        ttf(fb, bx + 4, by + 5, name, tc);
    }

    fb.draw_hline(x + 16, y + 240, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_toggle_row(
        fb,
        x,
        y + 252,
        w,
        "Transparent Taskbar",
        "Enable taskbar transparency",
        true,
    );
    draw_toggle_row(
        fb,
        x,
        y + 292,
        w,
        "Window Animations",
        "Enable window effects",
        true,
    );

    340
}

// ═══════════════════════════════════════════════════════════════════════════
// SYSTEM TAB
// ═══════════════════════════════════════════════════════════════════════════
fn draw_system_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    draw_section_header(fb, x, y + 12, w, "System");

    draw_info_row(fb, x, y + 42, "OS", "KnoxOS v0.1.0");
    draw_info_row(fb, x, y + 60, "Architecture", ARCH_NAME);
    draw_info_row(fb, x, y + 78, "Kernel", "Microkernel (Rust)");

    fb.draw_hline(x + 16, y + 98, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 108, w, "Performance");

    // CPU bar
    ttf_b(fb, x + 24, y + 136, "CPU", Pixel::rgb(140, 140, 140));
    let bar_w = (w - 100) as u32;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 138, bar_w, 8),
        Pixel::rgb(50, 50, 55),
        4,
    );
    let cpu_fill = bar_w * 23 / 100;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 138, cpu_fill, 8),
        Pixel::rgb(82, 139, 255),
        4,
    );
    ttf(fb, x + w - 48, y + 136, "23%", Pixel::rgb(82, 139, 255));

    // Memory bar
    ttf_b(fb, x + 24, y + 160, "RAM", Pixel::rgb(140, 140, 140));
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 162, bar_w, 8),
        Pixel::rgb(50, 50, 55),
        4,
    );
    let mem_fill = bar_w * 42 / 100;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 162, mem_fill, 8),
        Pixel::rgb(158, 206, 106),
        4,
    );
    ttf(fb, x + w - 48, y + 160, "42%", Pixel::rgb(158, 206, 106));

    fb.draw_hline(x + 16, y + 188, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 198, w, "Power");
    draw_toggle_row(
        fb,
        x,
        y + 228,
        w,
        "Power Saving",
        "Reduce performance to save battery",
        false,
    );
    // Focus: item 0 = power saving toggle
    draw_focus_indicator(fb, Rect::new(x + w - 60, y + 228, 36, 18), focus_idx == 0);
    draw_info_row(fb, x, y + 268, "Uptime", "0d 0h 12m");

    // ── Keyboard Layout section ──
    fb.draw_hline(x + 16, y + 290, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 300, w, "Keyboard Layout");

    let current_layout = crate::task::keyboard::get_layout();
    let layouts: &[(&str, &str, crate::task::keyboard::KeyboardLayout)] = &[
        (
            "US QWERTY",
            "English (US)",
            crate::task::keyboard::KeyboardLayout::Us,
        ),
        (
            "UK QWERTY",
            "English (UK)",
            crate::task::keyboard::KeyboardLayout::Uk,
        ),
        (
            "QWERTZ",
            "German",
            crate::task::keyboard::KeyboardLayout::De,
        ),
        (
            "AZERTY",
            "French",
            crate::task::keyboard::KeyboardLayout::Fr,
        ),
        (
            "QWERTY",
            "Spanish",
            crate::task::keyboard::KeyboardLayout::Es,
        ),
        (
            "Dvorak",
            "US Dvorak",
            crate::task::keyboard::KeyboardLayout::Dvorak,
        ),
    ];

    let mut layout_y = y + 332;
    for (i, (label, desc, layout)) in layouts.iter().enumerate() {
        let is_active = current_layout == *layout;
        let row_rect = Rect::new(x + 20, layout_y, (w - 40) as u32, 30);

        if is_active {
            fb.fill_rounded_rect_aa(row_rect, Pixel::new(0, 100, 180, 60), 6);
        }

        // Radio dot
        let dot_x = x + 32;
        let dot_y = layout_y + 15;
        fb.draw_rounded_rect(
            Rect::new(dot_x - 7, dot_y - 7, 14, 14),
            Pixel::new(100, 160, 220, 160),
            7,
            1,
        );
        if is_active {
            fb.fill_circle_aa(dot_x, dot_y, 4u32, Pixel::new(0, 200, 255, 220));
        }

        // Layout name
        ttf_b(
            fb,
            x + 48,
            layout_y + 4,
            label,
            if is_active {
                Pixel::new(0, 220, 255, 255)
            } else {
                Pixel::new(200, 220, 240, 220)
            },
        );

        // Description
        ttf(
            fb,
            x + 48,
            layout_y + 18,
            desc,
            Pixel::new(120, 150, 180, 160),
        );

        // Focus: items 1..6 = keyboard layouts
        draw_focus_indicator(fb, row_rect, focus_idx == (i as i32 + 1));

        layout_y += 34;
    }

    layout_y - y + 20
}

// ═══════════════════════════════════════════════════════════════════════════
// USERS TAB — User account management
// ═══════════════════════════════════════════════════════════════════════════
fn draw_users_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    let mut cy = y + 20;
    let pad = 20i32;

    // Section header: Current user
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Current User");
    cy += 30;

    let current_uid = crate::users::get_current_uid();
    if let Some(user) = crate::users::get_user(current_uid) {
        // User avatar circle
        let avatar_cx = x + pad + 28;
        let avatar_cy = cy + 28;
        fb.fill_circle_aa(avatar_cx, avatar_cy, 24u32, Pixel::new(0, 140, 220, 200));
        fb.draw_rounded_rect(
            Rect::new(avatar_cx - 24, avatar_cy - 24, 48, 48),
            Pixel::new(80, 180, 255, 80),
            24,
            1,
        );
        // Initial letter
        let initial = user.username.chars().next().unwrap_or('U');
        let mut ibuf = [0u8; 4];
        let istr = initial
            .to_uppercase()
            .next()
            .unwrap_or('U')
            .encode_utf8(&mut ibuf);
        let iw = font_engine::measure_ui_text(istr, 15) as i32;
        ttf_h(fb, avatar_cx - iw / 2, avatar_cy - 10, istr, colors::WHITE);

        // Username and full name
        ttf_b(
            fb,
            x + pad + 64,
            cy + 10,
            &user.username,
            Pixel::new(220, 240, 255, 240),
        );
        ttf(
            fb,
            x + pad + 64,
            cy + 28,
            &user.gecos,
            Pixel::new(140, 160, 200, 180),
        );

        // User details
        let uid_str = {
            let mut s = String::from("UID: ");
            use core::fmt::Write;
            let _ = write!(s, "{}", user.uid);
            s
        };
        ttf(
            fb,
            x + pad + 64,
            cy + 44,
            &uid_str,
            Pixel::new(100, 130, 170, 160),
        );
        cy += 64;
    }
    cy += 16;

    // Section: All Users
    draw_section_header(fb, x + pad, cy, w - pad * 2, "System Users");
    cy += 30;

    // List all non-system users (uid >= 1000) and root
    let users = crate::users::list_users();
    for user in &users {
        if user.uid > 0 && user.uid < 1000 {
            continue; // Skip system service accounts
        }

        let row_h = 40i32;
        let row_rect = Rect::new(x + pad, cy, (w - pad * 2) as u32, row_h as u32);

        // Row background
        let is_current = user.uid == current_uid;
        if is_current {
            fb.fill_rounded_rect_aa(row_rect, Pixel::new(0, 100, 180, 40), 6);
        }

        // Small avatar circle
        let small_cx = x + pad + 16;
        let small_cy = cy + row_h / 2;
        let avatar_color = if user.uid == 0 {
            Pixel::new(200, 60, 60, 200) // Red for root
        } else if user.disabled {
            Pixel::new(80, 80, 80, 150)
        } else {
            Pixel::new(0, 140, 200, 180)
        };
        fb.fill_circle_aa(small_cx, small_cy, 12u32, avatar_color);

        // Username
        ttf_b(
            fb,
            x + pad + 36,
            cy + 6,
            &user.username,
            Pixel::new(220, 240, 255, 230),
        );

        // Role / info
        let role = if user.uid == 0 {
            "Administrator (root)"
        } else if user.disabled {
            "Disabled"
        } else {
            "Standard User"
        };
        ttf(
            fb,
            x + pad + 36,
            cy + 22,
            role,
            Pixel::new(120, 150, 190, 160),
        );

        // UID badge on right
        let uid_str = {
            let mut s = String::new();
            use core::fmt::Write;
            let _ = write!(s, "uid:{}", user.uid);
            s
        };
        let uid_w = font_engine::measure_ui_text(&uid_str, 13) as i32;
        ttf(
            fb,
            x + w - pad - uid_w - 8,
            cy + (row_h - 10) / 2,
            &uid_str,
            Pixel::new(80, 120, 160, 140),
        );

        cy += row_h + 4;
    }

    cy += 16;

    // Section: Account Actions
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Account Actions");
    cy += 30;

    // Lock screen button
    let btn_w = 160i32;
    let btn_h = 30i32;
    let btn_rect = Rect::new(x + pad, cy, btn_w as u32, btn_h as u32);
    fb.fill_rounded_rect_aa(btn_rect, Pixel::new(0, 140, 220, 180), 6);
    fb.draw_rounded_rect(btn_rect, Pixel::new(60, 180, 255, 60), 6, 1);
    ttf_b(
        fb,
        x + pad + 12,
        cy + (btn_h - 10) / 2,
        "Lock Screen",
        colors::WHITE,
    );
    // Focus: item 0 = Lock Screen button
    draw_focus_indicator(fb, btn_rect, focus_idx == 0);
    cy += btn_h + 10;

    // Logout button
    let btn_rect2 = Rect::new(x + pad, cy, btn_w as u32, btn_h as u32);
    fb.fill_rounded_rect_aa(btn_rect2, Pixel::new(180, 60, 60, 180), 6);
    fb.draw_rounded_rect(btn_rect2, Pixel::new(220, 100, 100, 60), 6, 1);
    ttf_b(
        fb,
        x + pad + 12,
        cy + (btn_h - 10) / 2,
        "Log Out",
        colors::WHITE,
    );
    // Focus: item 1 = Log Out button
    draw_focus_indicator(fb, btn_rect2, focus_idx == 1);
    cy += btn_h + 10;

    // Total content height
    cy - y + 20
}

// ═══════════════════════════════════════════════════════════════════════════
// DATE & TIME TAB (9.56)
// ═══════════════════════════════════════════════════════════════════════════
fn draw_datetime_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
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

// ═══════════════════════════════════════════════════════════════════════════
// PRIVACY & SECURITY TAB (9.57)
// ═══════════════════════════════════════════════════════════════════════════
fn draw_privacy_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
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
    let privacy = super::settings_ext::PRIVACY.lock();
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

// ═══════════════════════════════════════════════════════════════════════════
// STARTUP APPLICATIONS TAB (9.58)
// ═══════════════════════════════════════════════════════════════════════════
fn draw_startup_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
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

    let startup = super::settings_ext::STARTUP.lock();
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

// ═══════════════════════════════════════════════════════════════════════════
// ABOUT TAB
// ═══════════════════════════════════════════════════════════════════════════
fn draw_about_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32) -> i32 {
    // OS logo area
    let logo_y = y + 24;
    let logo_size: i32 = 56;
    let center_x = x + w / 2;

    // K logo circle
    fb.fill_circle_aa(
        center_x,
        logo_y + logo_size / 2,
        (logo_size / 2) as u32,
        Pixel::rgb(82, 139, 255),
    );
    font_engine::draw_ui_bold(
        fb,
        center_x - 10,
        logo_y + logo_size / 2 - 12,
        "K",
        28,
        colors::WHITE,
    );

    // OS name
    ttf_centered_b(
        fb,
        x,
        logo_y + logo_size + 8,
        w as u32,
        "KnoxOS",
        22,
        colors::WHITE,
    );

    ttf_centered(
        fb,
        x,
        logo_y + logo_size + 34,
        w as u32,
        "Version 0.1.0-dev",
        13,
        Pixel::rgb(140, 140, 140),
    );

    fb.draw_hline(
        x + 16,
        logo_y + logo_size + 52,
        (w - 32) as u32,
        Pixel::rgb(50, 50, 55),
    );

    let info_y = logo_y + logo_size + 64;
    draw_info_row(fb, x, info_y, "Architecture", ARCH_NAME);
    draw_info_row(fb, x, info_y + 20, "Language", "Rust (no_std)");
    draw_info_row(fb, x, info_y + 40, "Bootloader", "bootloader_api 0.11");
    draw_info_row(fb, x, info_y + 60, "Heap Size", "32 MiB");
    draw_info_row(fb, x, info_y + 80, "Font", "Hack 10x20 AA");
    draw_info_row(fb, x, info_y + 100, "Theme", "Tokyo Night");

    fb.draw_hline(
        x + 16,
        info_y + 122,
        (w - 32) as u32,
        Pixel::rgb(50, 50, 55),
    );

    ttf_centered(
        fb,
        x,
        info_y + 136,
        w as u32,
        "KnoxOS: An AI-native operating system written entirely in Rust.",
        13,
        Pixel::rgb(120, 120, 120),
    );
    ttf_centered(
        fb,
        x,
        info_y + 154,
        w as u32,
        "github.com/knoxchat/knoxos",
        13,
        Pixel::rgb(82, 139, 255),
    );

    // Total content height
    info_y - y + 190
}

// ═══════════════════════════════════════════════════════════════════════════
// KEYBOARD NAVIGATION
// ═══════════════════════════════════════════════════════════════════════════

/// Focus region: sidebar tabs vs content items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRegion {
    /// Sidebar tab at index 0..6
    Sidebar(i32),
    /// Content item at index 0..N
    Content(i32),
}

impl SettingsState {
    /// Decode current focus_index into a FocusRegion
    pub fn focus_region(&self) -> FocusRegion {
        if self.focus_index < 0 {
            // Sidebar: index is -(focus_index + 1), i.e. focus_index=-1 → tab 0
            let tab_idx = (-(self.focus_index + 1)).clamp(0, self.sidebar_count() - 1);
            FocusRegion::Sidebar(tab_idx)
        } else {
            FocusRegion::Content(self.focus_index)
        }
    }
}

/// Handle a keyboard event in the Settings window.
/// Returns `true` if the key was consumed.
///
/// Keys handled:
///   Tab / Shift+Tab — cycle focus forward / backward
///   Enter / Space — activate focused element
///   ArrowUp / ArrowDown — move within current region
///   ArrowRight — move from sidebar into content
///   ArrowLeft — move from content back to sidebar
pub fn handle_settings_key(scancode: u8, character: Option<char>, shift: bool) -> bool {
    let mut state = SETTINGS_STATE.lock();
    state.keyboard_nav = true;

    let sidebar_n = state.sidebar_count();
    let content_n = state.content_item_count();

    match character {
        // ── Tab / Shift+Tab: cycle through ALL focusable items ──
        Some('\t') => {
            if shift {
                // Backward
                if state.focus_index < 0 {
                    // In sidebar
                    let tab_idx = -(state.focus_index + 1);
                    if tab_idx > 0 {
                        state.focus_index = -(tab_idx - 1 + 1);
                    } else {
                        // Wrap to last content item (or last sidebar if no content)
                        if content_n > 0 {
                            state.focus_index = content_n - 1;
                        } else {
                            state.focus_index = -(sidebar_n - 1 + 1);
                        }
                    }
                } else {
                    // In content
                    if state.focus_index > 0 {
                        state.focus_index -= 1;
                    } else {
                        // Wrap to last sidebar tab
                        state.focus_index = -(sidebar_n - 1 + 1);
                    }
                }
            } else {
                // Forward
                if state.focus_index < 0 {
                    let tab_idx = -(state.focus_index + 1);
                    if tab_idx < sidebar_n - 1 {
                        state.focus_index = -(tab_idx + 1 + 1);
                    } else {
                        // Move to first content item
                        if content_n > 0 {
                            state.focus_index = 0;
                        } else {
                            state.focus_index = -1; // Wrap to first sidebar
                        }
                    }
                } else {
                    if state.focus_index < content_n - 1 {
                        state.focus_index += 1;
                    } else {
                        // Wrap to first sidebar tab
                        state.focus_index = -1;
                    }
                }
            }
            true
        }

        // ── Enter / Space: activate focused element ──
        Some('\n') | Some('\r') | Some(' ') => {
            let region = state.focus_region();
            match region {
                FocusRegion::Sidebar(tab_idx) => {
                    // Switch to the focused tab
                    if let Some((tab, _, _)) = TABS.get(tab_idx as usize) {
                        state.active_tab = *tab;
                        // Reset content focus when switching tabs
                        state.focus_index = -(tab_idx + 1);
                    }
                }
                FocusRegion::Content(idx) => {
                    activate_content_item(&state.active_tab, idx);
                }
            }
            true
        }

        _ => {
            // Check scancode for arrow keys
            match scancode {
                // Up arrow (0x48 via scancode set 1, or we check via the raw code)
                0x48 => {
                    if state.focus_index < 0 {
                        let tab_idx = -(state.focus_index + 1);
                        if tab_idx > 0 {
                            state.focus_index = -(tab_idx - 1 + 1);
                        }
                    } else if state.focus_index > 0 {
                        state.focus_index -= 1;
                    }
                    true
                }
                // Down arrow
                0x50 => {
                    if state.focus_index < 0 {
                        let tab_idx = -(state.focus_index + 1);
                        if tab_idx < sidebar_n - 1 {
                            state.focus_index = -(tab_idx + 1 + 1);
                        }
                    } else if state.focus_index < content_n - 1 {
                        state.focus_index += 1;
                    }
                    true
                }
                // Right arrow — move from sidebar to content
                0x4D => {
                    if state.focus_index < 0 && content_n > 0 {
                        state.focus_index = 0;
                    }
                    true
                }
                // Left arrow — move from content to sidebar
                0x4B => {
                    if state.focus_index >= 0 {
                        // Go to the sidebar tab matching current active_tab
                        let tab_idx = TABS
                            .iter()
                            .position(|(t, _, _)| *t == state.active_tab)
                            .unwrap_or(0) as i32;
                        state.focus_index = -(tab_idx + 1);
                    }
                    true
                }
                _ => false,
            }
        }
    }
}

/// Activate a content item by index in the given tab
fn activate_content_item(tab: &SettingsTab, idx: i32) {
    match tab {
        SettingsTab::Display => {
            // 0..N-1 = resolution options, N = brightness (no-op for now)
            let res_count = super::RESOLUTIONS.len() as i32;
            if idx < res_count {
                if let Some(&(rw, rh, _)) = super::RESOLUTIONS.get(idx as usize) {
                    let (cur_w, cur_h) = super::screen_size();
                    if rw != cur_w || rh != cur_h {
                        crate::serial_println!(
                            "[Settings] Focus-activate resolution {}x{}",
                            rw,
                            rh
                        );
                        super::change_resolution(rw, rh);
                    }
                }
            }
        }
        SettingsTab::System => {
            // 0 = power saving toggle, 1..6 = keyboard layouts
            if (1..=6).contains(&idx) {
                let layouts = [
                    crate::task::keyboard::KeyboardLayout::Us,
                    crate::task::keyboard::KeyboardLayout::Uk,
                    crate::task::keyboard::KeyboardLayout::De,
                    crate::task::keyboard::KeyboardLayout::Fr,
                    crate::task::keyboard::KeyboardLayout::Es,
                    crate::task::keyboard::KeyboardLayout::Dvorak,
                ];
                if let Some(layout) = layouts.get((idx - 1) as usize) {
                    crate::task::keyboard::set_layout(*layout);
                    crate::serial_println!(
                        "[Settings] Focus-activate keyboard layout: {}",
                        layout.name()
                    );
                }
            }
        }
        SettingsTab::Users => {
            match idx {
                0 => {
                    // Lock Screen
                    super::lock_screen::lock();
                }
                1 => {
                    // Log Out
                    super::lock_screen::lock();
                    super::login::reset();
                }
                _ => {}
            }
        }
        SettingsTab::DateTime => {
            match idx {
                0 => {
                    // Toggle NTP
                    let mut dt = crate::gui::settings_ext::get_datetime();
                    // Toggle NTP by accessing the settings ext module
                    crate::serial_println!("[Settings] Toggle NTP");
                }
                1 => {
                    // Toggle 24h format
                    crate::serial_println!("[Settings] Toggle 24h format");
                }
                2 => {
                    // Toggle show seconds
                    crate::serial_println!("[Settings] Toggle show seconds");
                }
                i if i >= 3 => {
                    // Select timezone
                    let tz_idx = (i - 3) as usize;
                    let timezones = crate::gui::settings_ext::common_timezones();
                    if let Some(tz) = timezones.get(tz_idx) {
                        crate::gui::settings_ext::set_timezone(&tz.name);
                        crate::serial_println!("[Settings] Timezone: {}", tz.name);
                    }
                }
                _ => {}
            }
        }
        SettingsTab::Privacy => {
            let settings = [
                "location",
                "analytics",
                "camera",
                "microphone",
                "firewall",
                "autoupdate",
            ];
            if let Some(key) = settings.get(idx as usize) {
                let new_val = crate::gui::settings_ext::toggle_privacy(key);
                crate::serial_println!("[Settings] Privacy '{}' = {}", key, new_val);
            }
        }
        SettingsTab::Startup => {
            // Toggle startup app by index
            let startup = crate::gui::settings_ext::STARTUP.lock();
            if let Some(app) = startup.apps.get(idx as usize) {
                let name = app.name.clone();
                drop(startup);
                let new_val = crate::gui::settings_ext::toggle_startup_app(&name);
                crate::serial_println!("[Settings] Startup '{}' enabled = {}", name, new_val);
            }
        }
        _ => {
            // Other tabs: toggle/slider items — placeholder (would toggle state)
            crate::serial_println!(
                "[Settings] Focus-activate content item {} in {:?}",
                idx,
                tab
            );
        }
    }
}

/// Draw a focus ring around a rect if keyboard navigation is active and index matches
fn draw_focus_indicator(fb: &mut FrameBuffer, rect: Rect, focused: bool) {
    if focused {
        crate::gui::accessibility::draw_focus_ring(fb, rect);
    }
}

/// Handle click on settings content.
/// `scroll_y` is the current window scroll offset for the content pane.
pub fn handle_settings_click(
    x: i32,
    y: i32,
    content_rect: Rect,
    state: &mut SettingsState,
    scroll_y: i32,
) -> bool {
    let cx = content_rect.x;
    let cy = content_rect.y;
    let cw = content_rect.width as i32;

    // Sidebar tab clicks
    if x >= cx && x < cx + SIDEBAR_WIDTH {
        for (i, (tab, _, _)) in TABS.iter().enumerate() {
            let ty = cy + 44 + i as i32 * TAB_HEIGHT;
            if y >= ty && y < ty + TAB_HEIGHT {
                state.active_tab = *tab;
                return true;
            }
        }
    }

    // Personalization tab: theme and accent color clicks
    if state.active_tab == SettingsTab::Personalization {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;

        // Theme swatches (3 themes, 82px apart, starting at x+24, y+60)
        let theme_ids = [
            super::theme::ThemeId::NebulaDark,
            super::theme::ThemeId::ArcticLight,
            super::theme::ThemeId::SunsetWarm,
        ];
        for (i, theme_id) in theme_ids.iter().enumerate() {
            let sx = content_x + 24 + i as i32 * 82;
            let sy = content_y - scroll_y + 60;
            let swatch_rect = Rect::new(sx, sy, 72, 36);
            if swatch_rect.contains(x, y) {
                super::theme::set_theme(*theme_id);
                crate::serial_println!("[Settings] Theme: {:?}", theme_id);
                super::request_redraw();
                return true;
            }
        }

        // Accent color circles (6 colors, 32px apart, starting at x+24, y+145)
        for i in 0..6u8 {
            let ax = content_x + 24 + i as i32 * 32;
            let ay = content_y - scroll_y + 145;
            // Circle hit test: 10px radius around center (ax+10, ay+10)
            let dx = x - (ax + 10);
            let dy = y - (ay + 10);
            if dx * dx + dy * dy <= 12 * 12 {
                super::theme::set_accent_index(i);
                crate::serial_println!("[Settings] Accent color: {}", i);
                return true;
            }
        }

        // Cursor theme buttons (4 buttons, 55px apart, starting at x+24, y+206)
        let cursor_themes = [
            super::desktop::CursorTheme::Default,
            super::desktop::CursorTheme::Dark,
            super::desktop::CursorTheme::Accent,
            super::desktop::CursorTheme::Large,
        ];
        for (i, theme) in cursor_themes.iter().enumerate() {
            let bx = content_x + 24 + i as i32 * 55;
            let by = content_y - scroll_y + 206;
            let btn_rect = Rect::new(bx, by, 48, 22);
            if btn_rect.contains(x, y) {
                super::desktop::set_cursor_theme(*theme);
                crate::serial_println!("[Settings] Cursor theme: {}", i);
                return true;
            }
        }
    }

    // Display tab: resolution picker clicks
    if state.active_tab == SettingsTab::Display {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let content_w = cw - SIDEBAR_WIDTH;

        crate::serial_println!(
            "[Settings] Display click: mouse=({},{}) content_xy=({},{}) scroll_y={}",
            x,
            y,
            content_x,
            content_y,
            scroll_y
        );

        for (i, &(rw, rh, _label)) in super::RESOLUTIONS.iter().enumerate() {
            let row_y = content_y - scroll_y + RES_LIST_Y_START + i as i32 * RES_ROW_HEIGHT;
            let btn_rect = Rect::new(
                content_x + RES_LIST_X_PAD,
                row_y,
                (content_w - RES_LIST_X_PAD * 2) as u32,
                RES_ROW_HEIGHT as u32 - 4,
            );
            if i < 3 {
                crate::serial_println!(
                    "[Settings]   res[{}] {}x{}: btn=({},{},{}x{}) hit={}",
                    i,
                    rw,
                    rh,
                    btn_rect.x,
                    btn_rect.y,
                    btn_rect.width,
                    btn_rect.height,
                    btn_rect.contains(x, y)
                );
            }
            if btn_rect.contains(x, y) {
                // Don't switch if already at this resolution
                let (cur_w, cur_h) = super::screen_size();
                if rw != cur_w || rh != cur_h {
                    crate::serial_println!("[Settings] User selected resolution {}x{}", rw, rh);
                    // Perform the change (this drops the FB lock internally)
                    super::change_resolution(rw, rh);
                }
                return true;
            }
        }
    }

    // System tab: keyboard layout clicks
    if state.active_tab == SettingsTab::System {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;

        let layouts: &[crate::task::keyboard::KeyboardLayout] = &[
            crate::task::keyboard::KeyboardLayout::Us,
            crate::task::keyboard::KeyboardLayout::Uk,
            crate::task::keyboard::KeyboardLayout::De,
            crate::task::keyboard::KeyboardLayout::Fr,
            crate::task::keyboard::KeyboardLayout::Es,
            crate::task::keyboard::KeyboardLayout::Dvorak,
        ];

        for (i, layout) in layouts.iter().enumerate() {
            let row_y = content_y - scroll_y + 332 + i as i32 * 34;
            let row_rect = Rect::new(content_x + 20, row_y, (cw - SIDEBAR_WIDTH - 40) as u32, 30);
            if row_rect.contains(x, y) {
                crate::task::keyboard::set_layout(*layout);
                crate::serial_println!("[Settings] Keyboard layout: {}", layout.name());
                return true;
            }
        }
    }

    // Users tab: Lock screen and Logout button clicks
    if state.active_tab == SettingsTab::Users {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        // Approximate button positions (match draw_users_tab layout)
        // Lock Screen button
        let lock_btn = Rect::new(content_x + pad, content_y - scroll_y + 460, 160, 30);
        if lock_btn.contains(x, y) {
            super::lock_screen::lock();
            return true;
        }
        // Log Out button
        let logout_btn = Rect::new(content_x + pad, content_y - scroll_y + 500, 160, 30);
        if logout_btn.contains(x, y) {
            super::lock_screen::lock();
            super::login::reset();
            return true;
        }
    }

    // DateTime tab: timezone selection + toggle clicks
    if state.active_tab == SettingsTab::DateTime {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        // NTP toggle (approx y offset matches draw_datetime_tab layout)
        let ntp_rect = Rect::new(
            content_x + pad,
            content_y - scroll_y + 80,
            (cw - SIDEBAR_WIDTH - pad * 2) as u32,
            28,
        );
        if ntp_rect.contains(x, y) {
            crate::serial_println!("[Settings] Toggle NTP");
            return true;
        }

        // 24h format toggle
        let fmt_rect = Rect::new(
            content_x + pad,
            content_y - scroll_y + 170,
            (cw - SIDEBAR_WIDTH - pad * 2) as u32,
            28,
        );
        if fmt_rect.contains(x, y) {
            crate::serial_println!("[Settings] Toggle 24h format");
            return true;
        }

        // Show seconds toggle
        let sec_rect = Rect::new(
            content_x + pad,
            content_y - scroll_y + 206,
            (cw - SIDEBAR_WIDTH - pad * 2) as u32,
            28,
        );
        if sec_rect.contains(x, y) {
            crate::serial_println!("[Settings] Toggle show seconds");
            return true;
        }

        // Timezone list
        let timezones = crate::gui::settings_ext::common_timezones();
        for (i, tz) in timezones.iter().enumerate() {
            let row_y = content_y - scroll_y + 280 + i as i32 * 32;
            let row_rect = Rect::new(
                content_x + pad,
                row_y,
                (cw - SIDEBAR_WIDTH - pad * 2) as u32,
                28,
            );
            if row_rect.contains(x, y) {
                crate::gui::settings_ext::set_timezone(&tz.name);
                crate::serial_println!("[Settings] Timezone: {}", tz.name);
                return true;
            }
        }
    }

    // Privacy tab: toggle clicks
    if state.active_tab == SettingsTab::Privacy {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        let settings = [
            "location",
            "analytics",
            "camera",
            "microphone",
            "firewall",
            "autoupdate",
        ];
        for (i, key) in settings.iter().enumerate() {
            let row_y = content_y - scroll_y + 72 + i as i32 * 52;
            let row_rect = Rect::new(
                content_x + pad - 4,
                row_y - 2,
                (cw - SIDEBAR_WIDTH - pad * 2 + 8) as u32,
                48,
            );
            if row_rect.contains(x, y) {
                let new_val = crate::gui::settings_ext::toggle_privacy(key);
                crate::serial_println!("[Settings] Privacy '{}' = {}", key, new_val);
                return true;
            }
        }
    }

    // Startup tab: toggle clicks
    if state.active_tab == SettingsTab::Startup {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        let app_count = { crate::gui::settings_ext::STARTUP.lock().apps.len() };
        for i in 0..app_count {
            let row_y = content_y - scroll_y + 92 + i as i32 * 48;
            let row_rect = Rect::new(
                content_x + pad - 4,
                row_y - 2,
                (cw - SIDEBAR_WIDTH - pad * 2 + 8) as u32,
                44,
            );
            if row_rect.contains(x, y) {
                let startup = crate::gui::settings_ext::STARTUP.lock();
                if let Some(app) = startup.apps.get(i) {
                    let name = app.name.clone();
                    drop(startup);
                    let new_val = crate::gui::settings_ext::toggle_startup_app(&name);
                    crate::serial_println!("[Settings] Startup '{}' enabled = {}", name, new_val);
                }
                return true;
            }
        }
    }

    true
}
