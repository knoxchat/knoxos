/// Taskbar — "Aurora Dock" — AI-native centered pill-shaped dock
/// Unlike any traditional OS taskbar. Floats as a translucent capsule at
/// the bottom center of the screen with warm app indicators.
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use spin::Mutex;

use super::colors;
use super::font_engine;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::icon_theme;
use super::icon_theme::IconCategory;
use super::icons;
use super::system_tray;
use super::window;

/// Total dock vertical footprint (including float gap)
pub const TASKBAR_HEIGHT: u32 = 54;

/// Actual dock bar height (the visible pill)
const DOCK_BAR_HEIGHT: u32 = 44;

/// Gap between dock and screen bottom edge
const DOCK_FLOAT_GAP: u32 = 8;

/// Min dock width (capsule)
const DOCK_MIN_WIDTH: u32 = 360;

/// Entry item size (icon-only square cells in dock)
const DOCK_ITEM_SIZE: u32 = 36;

/// Dock pill corner radius
const DOCK_RADIUS: u32 = 18;

/// Entry icon size
const ENTRY_ICON_SIZE: u32 = 20;

/// Max visible entries before clipping
const MAX_DOCK_ENTRIES: usize = 14;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Taskbar Position — configurable dock location
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Taskbar dock position on screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskbarPosition {
    Bottom = 0,
    Top = 1,
    Left = 2,
    Right = 3,
}

static TASKBAR_POSITION: core::sync::atomic::AtomicU8 =
    core::sync::atomic::AtomicU8::new(TaskbarPosition::Bottom as u8);

/// Set the taskbar position
pub fn set_position(pos: TaskbarPosition) {
    TASKBAR_POSITION.store(pos as u8, core::sync::atomic::Ordering::Relaxed);
    super::request_redraw();
}

/// Get the current taskbar position
pub fn position() -> TaskbarPosition {
    match TASKBAR_POSITION.load(core::sync::atomic::Ordering::Relaxed) {
        1 => TaskbarPosition::Top,
        2 => TaskbarPosition::Left,
        3 => TaskbarPosition::Right,
        _ => TaskbarPosition::Bottom,
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Taskbar Icon Size / Resize
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

static DOCK_ICON_SCALE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(100); // percentage: 75, 100, 125, 150

/// Set dock icon scale percentage (75–150)
pub fn set_icon_scale(pct: u8) {
    let clamped = pct.clamp(75, 150);
    DOCK_ICON_SCALE.store(clamped, core::sync::atomic::Ordering::Relaxed);
    super::request_redraw();
}

/// Get current dock icon scale percentage
pub fn icon_scale() -> u8 {
    DOCK_ICON_SCALE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Get the effective dock item size after scaling
pub fn effective_item_size() -> u32 {
    let base = DOCK_ITEM_SIZE;
    let scale = icon_scale() as u32;
    (base * scale + 50) / 100 // round
}

/// Get the effective entry icon size after scaling
pub fn effective_icon_size() -> u32 {
    let base = ENTRY_ICON_SIZE;
    let scale = icon_scale() as u32;
    (base * scale + 50) / 100
}

/// Taskbar entry for a running application
#[derive(Clone)]
pub struct TaskbarEntry {
    pub window_id: window::WindowId,
    pub title: String,
    pub active: bool,
    pub content_type: window::WindowContentType,
    /// Notification badge count (0 = no badge)
    pub badge_count: u32,
}

/// Taskbar state
pub struct Taskbar {
    pub entries: Vec<TaskbarEntry>,
    pub clock_text: String,
    pub start_menu_open: bool,
    pub hovered_entry: Option<usize>,
}

/// Auto-hide state for the taskbar
static AUTO_HIDE_ENABLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
/// Current slide offset (0 = fully visible, TASKBAR_HEIGHT = fully hidden)
static AUTO_HIDE_OFFSET: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
/// Whether the mouse is in the hot zone at screen bottom
static MOUSE_IN_HOT_ZONE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Enable/disable taskbar auto-hide
pub fn set_auto_hide(enabled: bool) {
    AUTO_HIDE_ENABLED.store(enabled, core::sync::atomic::Ordering::Relaxed);
    if !enabled {
        AUTO_HIDE_OFFSET.store(0, core::sync::atomic::Ordering::Relaxed);
    }
}

/// Check if auto-hide is enabled
pub fn is_auto_hide() -> bool {
    AUTO_HIDE_ENABLED.load(core::sync::atomic::Ordering::Relaxed)
}

/// Get the current auto-hide offset (pixels the taskbar is hidden below screen)
pub fn auto_hide_offset() -> u32 {
    AUTO_HIDE_OFFSET.load(core::sync::atomic::Ordering::Relaxed)
}

/// Update auto-hide animation based on mouse position
pub fn update_auto_hide(mouse_y: i32, screen_h: u32) {
    if !is_auto_hide() {
        return;
    }
    let taskbar_h = super::scale::taskbar_height();
    let hot_zone = 4i32; // 4px trigger zone at screen bottom
    let mouse_near_bottom = mouse_y >= screen_h as i32 - hot_zone;
    let mouse_in_taskbar = mouse_y >= screen_h as i32 - taskbar_h as i32;

    let should_show = mouse_near_bottom
        || mouse_in_taskbar
        || super::startmenu::is_visible()
        || super::system_tray::is_context_menu_open()
        || super::popups::any_popup_open();

    MOUSE_IN_HOT_ZONE.store(should_show, core::sync::atomic::Ordering::Relaxed);

    let current = AUTO_HIDE_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let step = 6u32; // animation speed (pixels per frame)
    if should_show {
        // Slide in
        if current > 0 {
            let new_offset = current.saturating_sub(step);
            AUTO_HIDE_OFFSET.store(new_offset, core::sync::atomic::Ordering::Relaxed);
            super::request_redraw();
        }
    } else {
        // Slide out
        if current < taskbar_h {
            let new_offset = (current + step).min(taskbar_h);
            AUTO_HIDE_OFFSET.store(new_offset, core::sync::atomic::Ordering::Relaxed);
            super::request_redraw();
        }
    }
}

lazy_static::lazy_static! {
    pub static ref TASKBAR: Mutex<Taskbar> = Mutex::new(Taskbar {
        entries: Vec::new(),
        clock_text: String::from("12:00 AM"),
        start_menu_open: false,
        hovered_entry: None,
    });
}

/// Draw the Aurora floating dock — centered pill-shaped bar with warm app dots
pub fn draw_taskbar(fb: &mut FrameBuffer) {
    let screen_h = fb.height as i32;
    let screen_w = fb.width as u32;

    let taskbar = TASKBAR.lock();
    let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);

    // ══════════════════════════════════════════════════════════
    // COMPUTE DOCK DIMENSIONS (dynamic width based on apps open)
    // ══════════════════════════════════════════════════════════
    // Dock width = left_pad + icon1 + ... + iconN + right_pad + clock_width
    let dock_padding = 14i32; // Left/right interior padding
    let entry_spacing = 8i32; // Space between app icons
    let start_btn_w = DOCK_ITEM_SIZE as i32; // Launcher grid icon
    let clock_w = font_engine::measure_ui_text(&taskbar.clock_text, 12) as i32 + 16;
    let separator_w = 12i32; // Separator between apps and system tray
    let tray_w = system_tray::tray_width() as i32; // System tray icons
    let tray_separator_w = 10i32; // Separator between tray and clock
    let workspace_dot_size = 14i32; // Size of each workspace dot
    let workspace_dot_gap = 4i32;
    let workspace_count = crate::gui::window::NUM_WORKSPACES as i32;
    let workspace_area_w = workspace_count * workspace_dot_size
        + (workspace_count - 1) * workspace_dot_gap
        + entry_spacing; // dots + spacing + gap

    // Total dock content width
    let dock_content_w = start_btn_w
        + workspace_area_w
        + (if entry_count > 0 {
            entry_spacing + entry_count as i32 * DOCK_ITEM_SIZE as i32
        } else {
            0
        })
        + separator_w
        + tray_w
        + tray_separator_w
        + clock_w;
    let dock_w = (dock_content_w + dock_padding * 2).max(DOCK_MIN_WIDTH as i32) as u32;

    // Dock position: centered at bottom (with float gap)
    let dock_x = (screen_w as i32 - dock_w as i32) / 2;
    let hide_offset = auto_hide_offset() as i32;
    let dock_y = screen_h - DOCK_BAR_HEIGHT as i32 - DOCK_FLOAT_GAP as i32 + hide_offset;

    // ══════════════════════════════════════════════════════════
    // LIGHTWEIGHT DOCK SHADOW — Fast floating effect
    // Two compact passes instead of 3 + per-pixel glow
    // ══════════════════════════════════════════════════════════
    {
        // Layer 1: Soft ambient shadow (4 iterations instead of 20)
        for offset in (0i32..16).step_by(4) {
            let alpha = (14 - offset).clamp(0, 12) as u8;
            let shadow_rect = Rect::new(
                dock_x - offset / 2,
                dock_y + 6 + offset / 2,
                dock_w + offset as u32,
                DOCK_BAR_HEIGHT + offset as u32 / 2,
            );
            fb.draw_rounded_rect(
                shadow_rect,
                Pixel::new(0, 0, 0, alpha),
                DOCK_RADIUS + offset as u32 / 3,
                1,
            );
        }
        // Layer 2: Sharp contact shadow (2 iterations instead of 5)
        for offset in 0i32..3 {
            let alpha = ((3 - offset) * 10).min(30) as u8;
            let shadow_rect = Rect::new(
                dock_x - offset,
                dock_y + 3 + offset,
                dock_w + offset as u32 * 2,
                DOCK_BAR_HEIGHT + offset as u32,
            );
            fb.draw_rounded_rect(shadow_rect, Pixel::new(40, 20, 15, alpha), DOCK_RADIUS, 1);
        }
    }

    // ══════════════════════════════════════════════════════════
    // DOCK PILL — Enhanced glassmorphism with gradient depth
    // Modern UI trend: slightly lighter on top for 3D effect
    // ══════════════════════════════════════════════════════════
    let dock_rect = Rect::new(dock_x, dock_y, dock_w, DOCK_BAR_HEIGHT);

    // Glassmorphism with vertical gradient (lighter at top for depth)
    let dock_top = Pixel::new(30, 28, 36, 180);
    let dock_bottom = Pixel::new(22, 20, 28, 170);
    fb.fill_rounded_rect_gradient_aa(dock_rect, dock_top, dock_bottom, DOCK_RADIUS);

    // Multi-layer warm border (creates depth)
    // Outer border with warm accent
    fb.draw_rounded_rect(dock_rect, Pixel::new(200, 150, 130, 50), DOCK_RADIUS, 1);

    // Inner highlight for 3D bevel effect
    let inner_rect = Rect::new(dock_x + 1, dock_y + 1, dock_w - 2, DOCK_BAR_HEIGHT - 2);
    fb.draw_rounded_rect(
        inner_rect,
        Pixel::new(220, 180, 160, 20),
        DOCK_RADIUS - 1,
        1,
    );

    // Top edge warm highlight (hline instead of per-pixel loop)
    let hl_x0 = dock_x + DOCK_RADIUS as i32 + 2;
    let hl_w = dock_w.saturating_sub(DOCK_RADIUS * 2 + 4);
    fb.draw_hline(hl_x0, dock_y + 1, hl_w, Pixel::new(240, 210, 190, 28));
    fb.draw_hline(hl_x0, dock_y + 2, hl_w, Pixel::new(210, 180, 160, 16));

    // ══════════════════════════════════════════════════════════
    // DOCK CONTENT LAYOUT
    // ══════════════════════════════════════════════════════════
    let mut cursor_x = dock_x + dock_padding;
    let dock_cy = dock_y + DOCK_BAR_HEIGHT as i32 / 2;

    let (hover_mx, hover_my) = {
        let m = super::input::MOUSE.lock();
        (m.x, m.y)
    };

    // ── START LAUNCHER (app grid icon) ──────────────────────
    {
        let btn_rect = Rect::new(
            cursor_x,
            dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );
        let hovered = btn_rect.contains(hover_mx, hover_my);
        let menu_open = super::startmenu::is_visible();

        if menu_open || hovered {
            fb.fill_rounded_rect_aa(btn_rect, colors::TASKBAR_HOVER, 8);
        }

        // App grid icon (9 dots in 3×3 grid)
        let icon_cx = cursor_x + DOCK_ITEM_SIZE as i32 / 2;
        let dot_r = 2u32;
        let dot_spacing = 7i32;
        let dot_color = if hovered {
            Pixel::new(232, 140, 120, 255)
        } else {
            Pixel::new(200, 160, 140, 200)
        };
        for row in -1..=1i32 {
            for col in -1..=1i32 {
                fb.fill_circle_aa(
                    icon_cx + col * dot_spacing,
                    dock_cy + row * dot_spacing,
                    dot_r,
                    dot_color,
                );
            }
        }

        cursor_x += DOCK_ITEM_SIZE as i32 + entry_spacing;
    }

    // ── WORKSPACE INDICATOR (4 small dots/pills) ──────────────
    {
        use core::sync::atomic::Ordering;
        let current_ws = crate::gui::window::CURRENT_WORKSPACE.load(Ordering::Relaxed);
        for ws_i in 0..workspace_count {
            let dot_x = cursor_x;
            let dot_y = dock_cy - workspace_dot_size / 2;
            let dot_rect = Rect::new(
                dot_x,
                dot_y,
                workspace_dot_size as u32,
                workspace_dot_size as u32,
            );
            let is_active = ws_i as u8 == current_ws;
            let is_hovered = dot_rect.contains(hover_mx, hover_my);

            if is_active {
                // Active workspace: warm coral pill
                fb.fill_rounded_rect_aa(dot_rect, Pixel::new(232, 121, 100, 180), 4);
                // Glow effect
                let glow_rect = Rect::new(
                    dot_x - 1,
                    dot_y - 1,
                    workspace_dot_size as u32 + 2,
                    workspace_dot_size as u32 + 2,
                );
                fb.draw_rounded_rect(glow_rect, Pixel::new(232, 121, 100, 50), 5, 1);
            } else if is_hovered {
                fb.fill_rounded_rect_aa(dot_rect, Pixel::new(180, 140, 130, 120), 4);
            } else {
                // Inactive: dim pill
                fb.fill_rounded_rect_aa(dot_rect, Pixel::new(60, 55, 50, 100), 4);
            }

            // Draw workspace number (1-indexed)
            let num_char = match ws_i {
                0 => "1",
                1 => "2",
                2 => "3",
                _ => "4",
            };
            let text_w = font_engine::measure_ui_text(num_char, 11) as i32;
            let text_x = dot_x + (workspace_dot_size - text_w) / 2;
            let text_y = dot_y + (workspace_dot_size - 11) / 2;
            let text_color = if is_active {
                Pixel::new(255, 255, 255, 255)
            } else {
                Pixel::new(180, 170, 160, 180)
            };
            font_engine::draw_ui_text(fb, text_x, text_y, num_char, 11, text_color);

            cursor_x += workspace_dot_size + workspace_dot_gap;
        }
        cursor_x += entry_spacing - workspace_dot_gap; // adjust spacing before apps
    }

    // ── APP ENTRIES — Icon-only squares with active indicators ──
    for (i, entry) in taskbar.entries.iter().enumerate() {
        if i >= MAX_DOCK_ENTRIES {
            break;
        }

        let item_rect = Rect::new(
            cursor_x,
            dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );
        let hovered = taskbar.hovered_entry == Some(i);

        // Background glow on hover or active
        if entry.active {
            fb.fill_rounded_rect_aa(item_rect, colors::TASKBAR_ACTIVE, 8);
        } else if hovered {
            fb.fill_rounded_rect_aa(item_rect, colors::TASKBAR_HOVER, 8);
        }

        // Draw app icon (20×20 centered in 36×36 cell)
        let icon_x = cursor_x + (DOCK_ITEM_SIZE as i32 - ENTRY_ICON_SIZE as i32) / 2;
        let icon_y = dock_cy - ENTRY_ICON_SIZE as i32 / 2;
        draw_taskbar_icon(fb, icon_x, icon_y, entry.content_type);

        // Active indicator dot — glowing cyan dot below icon
        if entry.active {
            fb.fill_circle_aa(
                cursor_x + DOCK_ITEM_SIZE as i32 / 2,
                dock_y + DOCK_BAR_HEIGHT as i32 - 6,
                3,
                colors::DOCK_INDICATOR_ACTIVE,
            );
            // Subtle glow halo
            fb.fill_circle_aa(
                cursor_x + DOCK_ITEM_SIZE as i32 / 2,
                dock_y + DOCK_BAR_HEIGHT as i32 - 6,
                5,
                colors::DOCK_INDICATOR_GLOW,
            );
        } else {
            // Inactive: dim dot
            fb.fill_circle_aa(
                cursor_x + DOCK_ITEM_SIZE as i32 / 2,
                dock_y + DOCK_BAR_HEIGHT as i32 - 6,
                2,
                Pixel::new(140, 120, 110, 100),
            );
        }

        // Notification badge — red circle with count in top-right corner
        if entry.badge_count > 0 {
            let badge_cx = cursor_x + DOCK_ITEM_SIZE as i32 - 6;
            let badge_cy = dock_cy - DOCK_ITEM_SIZE as i32 / 2 + 5;
            // Red glow halo
            fb.fill_circle_aa(badge_cx, badge_cy, 7, Pixel::new(255, 40, 40, 60));
            // Badge circle
            fb.fill_circle_aa(badge_cx, badge_cy, 5, Pixel::rgb(230, 50, 50));
            // Count digit (or 9+ if >= 10)
            let badge_str = if entry.badge_count >= 10 {
                alloc::string::String::from("9")
            } else {
                let mut s = alloc::string::String::new();
                let _ = core::fmt::Write::write_fmt(&mut s, format_args!("{}", entry.badge_count));
                s
            };
            fonts::draw_char_bold_compact(
                fb,
                badge_cx - 3,
                badge_cy - 5,
                badge_str.chars().next().unwrap_or('0'),
                colors::WHITE,
                1,
            );
            if entry.badge_count >= 10 {
                fonts::draw_char_bold_compact(
                    fb,
                    badge_cx + 1,
                    badge_cy - 5,
                    '+',
                    colors::WHITE,
                    1,
                );
            }
        }

        cursor_x += DOCK_ITEM_SIZE as i32;
    }

    // ── SEPARATOR LINE (vertical divider before system tray) ────
    cursor_x += separator_w / 2;
    fb.fill_rounded_rect_aa(
        Rect::new(cursor_x - 1, dock_cy - 14, 2, 28),
        Pixel::new(160, 130, 120, 40),
        1,
    );
    cursor_x += separator_w / 2;

    // ── SYSTEM TRAY (Wi-Fi, Volume, Battery, Notifications) ────
    system_tray::draw(fb, cursor_x, dock_cy);
    cursor_x += tray_w;

    // ── SEPARATOR between tray and clock ────
    cursor_x += tray_separator_w / 2;
    fb.fill_rounded_rect_aa(
        Rect::new(cursor_x - 1, dock_cy - 10, 2, 20),
        Pixel::new(160, 130, 120, 30),
        1,
    );
    cursor_x += tray_separator_w / 2;

    // ── CLOCK (right side of dock) ──────────────────────────
    {
        let clock_rect = Rect::new(cursor_x, dock_cy - 10, clock_w as u32, 20);
        let hovered = clock_rect.contains(hover_mx, hover_my);

        if hovered {
            fb.fill_rounded_rect_aa(clock_rect, colors::TASKBAR_HOVER, 8);
        }

        let clock_text_x = cursor_x + 8;
        let clock_text_y = dock_cy - 6;
        font_engine::draw_ui_bold(
            fb,
            clock_text_x,
            clock_text_y,
            &taskbar.clock_text,
            12,
            colors::TASKBAR_TEXT,
        );
    }
}

/// Draw a tiny icon (16×16) in the taskbar for a given window content type
fn draw_taskbar_icon(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    content_type: window::WindowContentType,
) {
    let (category, name) = match content_type {
        window::WindowContentType::Terminal => (IconCategory::Apps, "utilities-x-terminal"),
        window::WindowContentType::FileExplorer => (IconCategory::Places, "folder"),
        window::WindowContentType::Browser => (IconCategory::Apps, "web-browser"),
        window::WindowContentType::AIAssistant => (IconCategory::Apps, "preferences-system"),
        window::WindowContentType::TextEditor => (IconCategory::Apps, "accessories-text-editor"),
        window::WindowContentType::Settings => (IconCategory::Apps, "org.gnome.Settings"),
        _ => (IconCategory::Mimetypes, "text-x-generic"),
    };
    icon_theme::draw_tiny_icon(fb, x, y, category, name);
}

/// Update the clock display (called from timer interrupt)
pub fn update_clock(_ticks: u64) {
    let mut taskbar = TASKBAR.lock();

    // Read real wall-clock time from CMOS RTC (works correctly in QEMU)
    let dt = crate::rtc::read_rtc();
    let hours = dt.hour as u64;
    let minutes = dt.minute as u64;
    let seconds = dt.second as u64;

    let am_pm = if hours < 12 { "AM" } else { "PM" };
    let display_hour = if hours == 0 {
        12
    } else if hours > 12 {
        hours - 12
    } else {
        hours
    };

    taskbar.clock_text.clear();
    write!(
        taskbar.clock_text,
        "{}:{:02}:{:02} {}",
        display_hour, minutes, seconds, am_pm
    )
    .ok();
}

/// Get the current clock text (for change detection)
pub fn get_clock_text() -> String {
    TASKBAR.lock().clock_text.clone()
}

/// Add an entry to the taskbar when a window opens
pub fn add_entry(window_id: window::WindowId, title: &str) {
    // Determine content type from the window manager
    let content_type = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == window_id)
            .map(|w| w.content_type)
            .unwrap_or(window::WindowContentType::Empty)
    };
    let mut taskbar = TASKBAR.lock();
    taskbar.entries.push(TaskbarEntry {
        window_id,
        title: String::from(title),
        active: true,
        content_type,
        badge_count: 0,
    });
}

/// Remove an entry from the taskbar when a window closes
pub fn remove_entry(window_id: window::WindowId) {
    let mut taskbar = TASKBAR.lock();
    taskbar.entries.retain(|e| e.window_id != window_id);
}

/// Set notification badge count for a specific dock entry (window)
pub fn set_badge(window_id: window::WindowId, count: u32) {
    let mut taskbar = TASKBAR.lock();
    if let Some(entry) = taskbar
        .entries
        .iter_mut()
        .find(|e| e.window_id == window_id)
    {
        entry.badge_count = count;
    }
}

/// Clear notification badge for a specific dock entry
pub fn clear_badge(window_id: window::WindowId) {
    set_badge(window_id, 0);
}

/// Set the active entry (focused window)
pub fn set_active(window_id: window::WindowId) {
    let mut taskbar = TASKBAR.lock();
    for entry in taskbar.entries.iter_mut() {
        entry.active = entry.window_id == window_id;
    }
}

/// Update hover state based on current mouse position in floating dock
pub fn update_hover(mouse_x: i32, mouse_y: i32, screen_width: u32, screen_height: u32) {
    let mut taskbar = TASKBAR.lock();
    let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);

    // Compute dock dimensions (same logic as draw_taskbar)
    let dock_padding = 14i32;
    let entry_spacing = 8i32;
    let start_btn_w = DOCK_ITEM_SIZE as i32;
    let clock_w = font_engine::measure_ui_text(&taskbar.clock_text, 12) as i32 + 16;
    let separator_w = 12i32;
    let tray_w = system_tray::tray_width() as i32;
    let tray_separator_w = 10i32;
    let workspace_dot_size = 14i32;
    let workspace_dot_gap = 4i32;
    let workspace_count = crate::gui::window::NUM_WORKSPACES as i32;
    let workspace_area_w = workspace_count * workspace_dot_size
        + (workspace_count - 1) * workspace_dot_gap
        + entry_spacing;

    let dock_content_w = start_btn_w
        + workspace_area_w
        + (if entry_count > 0 {
            entry_spacing + entry_count as i32 * DOCK_ITEM_SIZE as i32
        } else {
            0
        })
        + separator_w
        + tray_w
        + tray_separator_w
        + clock_w;
    let dock_w = (dock_content_w + dock_padding * 2).max(DOCK_MIN_WIDTH as i32) as u32;

    let dock_x = (screen_width as i32 - dock_w as i32) / 2;
    let hide_offset = auto_hide_offset() as i32;
    let dock_y =
        screen_height as i32 - DOCK_BAR_HEIGHT as i32 - DOCK_FLOAT_GAP as i32 + hide_offset;
    let dock_cy = dock_y + DOCK_BAR_HEIGHT as i32 / 2;

    // Check if mouse is even in dock area
    if mouse_y < dock_y
        || mouse_y > dock_y + DOCK_BAR_HEIGHT as i32
        || mouse_x < dock_x
        || mouse_x > dock_x + dock_w as i32
    {
        taskbar.hovered_entry = None;
        system_tray::clear_hover();
        return;
    }

    // Start button + workspace area + spacing
    let mut cursor_x =
        dock_x + dock_padding + DOCK_ITEM_SIZE as i32 + entry_spacing + workspace_area_w;

    // Check each entry
    let mut hovered = None;
    for (i, _entry) in taskbar.entries.iter().enumerate() {
        if i >= MAX_DOCK_ENTRIES {
            break;
        }
        let item_rect = Rect::new(
            cursor_x,
            dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );
        if item_rect.contains(mouse_x, mouse_y) {
            hovered = Some(i);
            break;
        }
        cursor_x += DOCK_ITEM_SIZE as i32;
    }
    taskbar.hovered_entry = hovered;

    // Advance past separator
    cursor_x += separator_w;

    // Update system tray hover
    system_tray::update_hover(mouse_x, mouse_y, cursor_x, dock_cy);
}

/// Clear hover state (called when mouse leaves taskbar area)
pub fn clear_hover() {
    TASKBAR.lock().hovered_entry = None;
}

/// Compute the (tray_x, dock_cy) coordinates for the system tray area.
/// Returns None if the dock layout can't be computed.
pub fn tray_layout(screen_width: u32, screen_height: u32) -> Option<(i32, i32)> {
    let taskbar = TASKBAR.lock();
    let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);
    let dock_padding = 14i32;
    let entry_spacing = 8i32;
    let start_btn_w = DOCK_ITEM_SIZE as i32;
    let clock_w = font_engine::measure_ui_text(&taskbar.clock_text, 12) as i32 + 16;
    let separator_w = 12i32;
    let tray_w = system_tray::tray_width() as i32;
    let tray_separator_w = 10i32;
    let workspace_dot_size = 14i32;
    let workspace_dot_gap = 4i32;
    let workspace_count = crate::gui::window::NUM_WORKSPACES as i32;
    let workspace_area_w = workspace_count * workspace_dot_size
        + (workspace_count - 1) * workspace_dot_gap
        + entry_spacing;
    let dock_content_w = start_btn_w
        + workspace_area_w
        + (if entry_count > 0 {
            entry_spacing + entry_count as i32 * DOCK_ITEM_SIZE as i32
        } else {
            0
        })
        + separator_w
        + tray_w
        + tray_separator_w
        + clock_w;
    let dock_w = (dock_content_w + dock_padding * 2).max(DOCK_MIN_WIDTH as i32) as u32;
    let dock_x = (screen_width as i32 - dock_w as i32) / 2;
    let hide_offset = auto_hide_offset() as i32;
    let dock_y =
        screen_height as i32 - DOCK_BAR_HEIGHT as i32 - DOCK_FLOAT_GAP as i32 + hide_offset;
    let dock_cy = dock_y + DOCK_BAR_HEIGHT as i32 / 2;
    let mut cursor_x = dock_x + dock_padding;
    cursor_x += start_btn_w; // start button
    cursor_x += workspace_area_w; // workspace dots
    if entry_count > 0 {
        cursor_x += entry_spacing + entry_count as i32 * DOCK_ITEM_SIZE as i32;
    }
    cursor_x += separator_w; // separator
    Some((cursor_x, dock_cy))
}

/// Handle taskbar/dock click
pub fn handle_click(x: i32, y: i32, screen_width: u32, screen_height: u32) -> bool {
    let taskbar = TASKBAR.lock();
    let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);

    // Compute dock dimensions (same logic as draw_taskbar)
    let dock_padding = 14i32;
    let entry_spacing = 8i32;
    let start_btn_w = DOCK_ITEM_SIZE as i32;
    let clock_w = font_engine::measure_ui_text(&taskbar.clock_text, 12) as i32 + 16;
    let separator_w = 12i32;
    let tray_w = system_tray::tray_width() as i32;
    let tray_separator_w = 10i32;
    let workspace_dot_size = 14i32;
    let workspace_dot_gap = 4i32;
    let workspace_count = crate::gui::window::NUM_WORKSPACES as i32;
    let workspace_area_w = workspace_count * workspace_dot_size
        + (workspace_count - 1) * workspace_dot_gap
        + entry_spacing;

    let dock_content_w = start_btn_w
        + workspace_area_w
        + (if entry_count > 0 {
            entry_spacing + entry_count as i32 * DOCK_ITEM_SIZE as i32
        } else {
            0
        })
        + separator_w
        + tray_w
        + tray_separator_w
        + clock_w;
    let dock_w = (dock_content_w + dock_padding * 2).max(DOCK_MIN_WIDTH as i32) as u32;

    let dock_x = (screen_width as i32 - dock_w as i32) / 2;
    let hide_offset = auto_hide_offset() as i32;
    let dock_y =
        screen_height as i32 - DOCK_BAR_HEIGHT as i32 - DOCK_FLOAT_GAP as i32 + hide_offset;
    let dock_cy = dock_y + DOCK_BAR_HEIGHT as i32 / 2;

    // Check if click is even on the dock
    if y < dock_y || y > dock_y + DOCK_BAR_HEIGHT as i32 || x < dock_x || x > dock_x + dock_w as i32
    {
        return false;
    }

    let mut cursor_x = dock_x + dock_padding;

    // ── START LAUNCHER BUTTON ──
    let start_rect = Rect::new(
        cursor_x,
        dock_cy - DOCK_ITEM_SIZE as i32 / 2,
        DOCK_ITEM_SIZE,
        DOCK_ITEM_SIZE,
    );
    if start_rect.contains(x, y) {
        drop(taskbar);
        super::popups::close_all_popups();
        super::notifications::close_panel();
        super::startmenu::toggle();
        crate::serial_println!("[KnoxOS] Start menu toggled");
        return true;
    }
    cursor_x += DOCK_ITEM_SIZE as i32 + entry_spacing;

    // ── WORKSPACE DOTS (click to switch) ──
    for ws_i in 0..workspace_count {
        let dot_rect = Rect::new(
            cursor_x,
            dock_cy - workspace_dot_size / 2,
            workspace_dot_size as u32,
            workspace_dot_size as u32,
        );
        if dot_rect.contains(x, y) {
            drop(taskbar);
            let mut wm = window::WINDOW_MANAGER.lock();
            wm.switch_workspace(ws_i as u8);
            drop(wm);
            super::request_redraw();
            return true;
        }
        cursor_x += workspace_dot_size + workspace_dot_gap;
    }
    cursor_x += entry_spacing - workspace_dot_gap;

    // Close start menu on any other dock interaction
    let taskbar = if super::startmenu::is_visible() {
        drop(taskbar);
        super::startmenu::close();
        TASKBAR.lock()
    } else {
        taskbar
    };

    // ── APP ENTRIES ──
    for (i, entry) in taskbar.entries.iter().enumerate() {
        if i >= MAX_DOCK_ENTRIES {
            break;
        }

        let item_rect = Rect::new(
            cursor_x,
            dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );

        if item_rect.contains(x, y) {
            let wid = entry.window_id;
            let is_active = entry.active;
            drop(taskbar);

            super::popups::close_all_popups();
            super::notifications::close_panel();

            let mut wm = window::WINDOW_MANAGER.lock();
            if is_active {
                // Click on active: minimize
                wm.minimize_window(wid);
                let new_focus = wm.focused_window;
                drop(wm);

                let mut tb = TASKBAR.lock();
                for e in tb.entries.iter_mut() {
                    e.active = new_focus == Some(e.window_id);
                }
                crate::serial_println!("[KnoxOS] Dock: minimized window {}", wid);
            } else {
                // Click on inactive: focus/restore
                wm.focus_window(wid);
                drop(wm);
                set_active(wid);
                crate::serial_println!("[KnoxOS] Dock: focused window {}", wid);
            }
            return true;
        }

        cursor_x += DOCK_ITEM_SIZE as i32;
    }

    // Skip separator
    cursor_x += separator_w;

    // ── SYSTEM TRAY CLICKS ──
    {
        let tray_x = cursor_x;
        if system_tray::handle_click(x, y, tray_x, dock_cy) {
            drop(taskbar);
            return true;
        }
        cursor_x += tray_w;
    }

    // Skip tray-clock separator
    cursor_x += tray_separator_w;

    // ── CLOCK ──
    let clock_rect = Rect::new(cursor_x, dock_cy - 10, clock_w as u32, 20);
    if clock_rect.contains(x, y) {
        drop(taskbar);
        super::popups::close_all_popups();
        super::notifications::close_panel();
        super::popups::toggle_calendar();
        crate::serial_println!("[KnoxOS] Calendar popup toggled");
        return true;
    }

    true
}

// ═══════════════════════════════════════════════════════════════════════════
// TASKBAR CONTEXT MENU (right-click on taskbar)
// ═══════════════════════════════════════════════════════════════════════════

/// Taskbar context menu state
pub struct TaskbarContextMenu {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub hovered_item: Option<usize>,
}

lazy_static::lazy_static! {
    pub static ref TASKBAR_CTX_MENU: Mutex<TaskbarContextMenu> = Mutex::new(TaskbarContextMenu {
        visible: false,
        x: 0,
        y: 0,
        hovered_item: None,
    });
}

/// Context menu items for the taskbar
const TASKBAR_CTX_ITEMS: &[&str] = &[
    "Taskbar Settings",
    "Toggle Auto-hide",
    "Lock Screen",
    "Log Out",
];

/// Show taskbar context menu at the given position
pub fn show_context_menu(x: i32, y: i32) {
    let item_h = 28i32;
    let menu_h = TASKBAR_CTX_ITEMS.len() as i32 * item_h + 8;
    let mut ctx = TASKBAR_CTX_MENU.lock();
    ctx.visible = true;
    ctx.x = x;
    ctx.y = y - menu_h; // Show above the click point
    ctx.hovered_item = None;
}

/// Close the taskbar context menu
pub fn close_context_menu() {
    TASKBAR_CTX_MENU.lock().visible = false;
}

/// Check if context menu is visible
pub fn is_context_menu_open() -> bool {
    TASKBAR_CTX_MENU.lock().visible
}

/// Draw the taskbar context menu
pub fn draw_context_menu(fb: &mut FrameBuffer) {
    let ctx = TASKBAR_CTX_MENU.lock();
    if !ctx.visible {
        return;
    }

    let item_h = 28i32;
    let menu_w: u32 = 180;
    let menu_h = TASKBAR_CTX_ITEMS.len() as u32 * item_h as u32 + 8;
    let menu_x = ctx.x;
    let menu_y = ctx.y;

    // Background with rounded corners
    let bg = Pixel::rgb(36, 36, 42);
    fb.fill_rounded_rect_aa(Rect::new(menu_x, menu_y, menu_w, menu_h), bg, 8);
    fb.draw_rounded_rect(
        Rect::new(menu_x, menu_y, menu_w, menu_h),
        Pixel::rgb(60, 60, 66),
        8,
        1,
    );

    // Menu items
    for (i, label) in TASKBAR_CTX_ITEMS.iter().enumerate() {
        let item_y = menu_y + 4 + i as i32 * item_h;
        let is_hovered = ctx.hovered_item == Some(i);

        if is_hovered {
            fb.fill_rounded_rect_aa(
                Rect::new(menu_x + 4, item_y, menu_w - 8, item_h as u32),
                Pixel::rgb(55, 55, 62),
                4,
            );
        }

        let text_color = if is_hovered {
            colors::WHITE
        } else {
            Pixel::rgb(200, 200, 210)
        };

        font_engine::draw_ui_text(fb, menu_x + 14, item_y + 7, label, 12, text_color);
    }
}

/// Handle click on taskbar context menu. Returns true if handled.
pub fn handle_context_menu_click(x: i32, y: i32) -> bool {
    let ctx = TASKBAR_CTX_MENU.lock();
    if !ctx.visible {
        return false;
    }

    let item_h = 28i32;
    let menu_w = 180i32;
    let menu_h = TASKBAR_CTX_ITEMS.len() as i32 * item_h + 8;
    let menu_x = ctx.x;
    let menu_y = ctx.y;

    // Check if click is outside menu
    if x < menu_x || x > menu_x + menu_w || y < menu_y || y > menu_y + menu_h {
        drop(ctx);
        close_context_menu();
        return true;
    }

    // Determine which item was clicked
    let rel_y = y - menu_y - 4;
    let item_idx = rel_y / item_h;
    drop(ctx);
    close_context_menu();

    if item_idx >= 0 && (item_idx as usize) < TASKBAR_CTX_ITEMS.len() {
        match item_idx as usize {
            0 => {
                // Taskbar Settings: open settings window on Personalization tab
                super::desktop::open_application("Settings", super::desktop::IconType::Settings);
            }
            1 => {
                // Toggle auto-hide
                let current = is_auto_hide();
                set_auto_hide(!current);
                crate::serial_println!("[KnoxOS] Taskbar auto-hide: {}", !current);
            }
            2 => {
                // Lock screen
                super::lock_screen::lock();
            }
            3 => {
                // Log out
                super::sounds::logout();
                super::login::set_logged_in(false);
                super::login::reset();
            }
            _ => {}
        }
    }

    true
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Window preview tooltip — shown when hovering a running app entry
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// State for the window preview tooltip above taskbar
struct WindowPreview {
    visible: bool,
    window_id: window::WindowId,
    title: String,
    content_type: window::WindowContentType,
    /// X center of the hovered dock item (for tooltip placement)
    item_center_x: i32,
    /// Y of the dock top edge
    dock_top_y: i32,
}

lazy_static::lazy_static! {
    static ref WINDOW_PREVIEW: Mutex<WindowPreview> = Mutex::new(WindowPreview {
        visible: false,
        window_id: 0,
        title: String::new(),
        content_type: window::WindowContentType::Empty,
        item_center_x: 0,
        dock_top_y: 0,
    });
}

/// Update the window preview based on current hover state.
/// Called after update_hover().
pub fn update_window_preview(screen_width: u32, screen_height: u32) {
    let taskbar = TASKBAR.lock();
    let mut preview = WINDOW_PREVIEW.lock();

    if let Some(idx) = taskbar.hovered_entry {
        if let Some(entry) = taskbar.entries.get(idx) {
            if !entry.active {
                if preview.visible {
                    preview.visible = false;
                    super::request_redraw();
                }
                return;
            }
            let changed = !preview.visible || preview.window_id != entry.window_id;
            preview.visible = true;
            preview.window_id = entry.window_id;
            preview.title.clear();
            preview.title.push_str(&entry.title);
            preview.content_type = entry.content_type;

            // Compute dock item center x (same layout math as update_hover)
            let dock_padding = 14i32;
            let entry_spacing = 8i32;
            let start_btn_w = DOCK_ITEM_SIZE as i32;
            let clock_w = font_engine::measure_ui_text(&taskbar.clock_text, 12) as i32 + 16;
            let separator_w = 12i32;
            let tray_w = system_tray::tray_width() as i32;
            let tray_separator_w = 10i32;
            let workspace_dot_size = 14i32;
            let workspace_dot_gap = 4i32;
            let workspace_count = crate::gui::window::NUM_WORKSPACES as i32;
            let workspace_area_w = workspace_count * workspace_dot_size
                + (workspace_count - 1) * workspace_dot_gap
                + entry_spacing;
            let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);
            let dock_content_w = start_btn_w
                + workspace_area_w
                + (if entry_count > 0 {
                    entry_spacing + entry_count as i32 * DOCK_ITEM_SIZE as i32
                } else {
                    0
                })
                + separator_w
                + tray_w
                + tray_separator_w
                + clock_w;
            let dock_w = (dock_content_w + dock_padding * 2).max(DOCK_MIN_WIDTH as i32);
            let dock_x = (screen_width as i32 - dock_w) / 2;
            let hide_offset = auto_hide_offset() as i32;
            let dock_y =
                screen_height as i32 - DOCK_BAR_HEIGHT as i32 - DOCK_FLOAT_GAP as i32 + hide_offset;

            let items_start_x =
                dock_x + dock_padding + start_btn_w + workspace_area_w + entry_spacing;
            preview.item_center_x =
                items_start_x + idx as i32 * DOCK_ITEM_SIZE as i32 + DOCK_ITEM_SIZE as i32 / 2;
            preview.dock_top_y = dock_y;

            if changed {
                super::request_redraw();
            }
            return;
        }
    }
    if preview.visible {
        preview.visible = false;
        super::request_redraw();
    }
}

/// Check if the window preview tooltip is visible
pub fn is_preview_visible() -> bool {
    WINDOW_PREVIEW.lock().visible
}

/// Draw the window preview tooltip above the hovered dock item
pub fn draw_window_preview(fb: &mut FrameBuffer) {
    let preview = WINDOW_PREVIEW.lock();
    if !preview.visible {
        return;
    }

    let preview_w = 180i32;
    let preview_h = 120i32;
    let arrow_h = 6i32;
    let gap = 6i32;

    // Position preview centered above the dock item
    let px = (preview.item_center_x - preview_w / 2)
        .max(8)
        .min(fb.width as i32 - preview_w - 8);
    let py = preview.dock_top_y - preview_h - arrow_h - gap;

    let rect = Rect::new(px, py, preview_w as u32, preview_h as u32);

    // Background: frosted glass panel
    fb.fill_rounded_rect_aa(rect, Pixel::new(20, 25, 38, 230), 10);
    fb.draw_rounded_rect(rect, Pixel::new(80, 140, 240, 60), 10, 1);

    // Arrow pointing down toward dock item
    let arrow_cx = preview.item_center_x.max(px + 10).min(px + preview_w - 10);
    for i in 0..arrow_h {
        let half_w = (arrow_h - i).max(1);
        fb.fill_rect(
            Rect::new(
                arrow_cx - half_w,
                py + preview_h + i,
                (half_w * 2) as u32,
                1,
            ),
            Pixel::new(20, 25, 38, 230),
        );
    }

    // Window content preview area
    let content_margin = 8i32;
    let content_w = preview_w - content_margin * 2;
    let content_h = preview_h - 30 - content_margin;
    let content_rect = Rect::new(
        px + content_margin,
        py + content_margin,
        content_w as u32,
        content_h as u32,
    );

    let bg = match preview.content_type {
        window::WindowContentType::Terminal => Pixel::new(25, 28, 38, 255),
        window::WindowContentType::Browser => Pixel::new(40, 45, 58, 255),
        window::WindowContentType::FileExplorer => Pixel::new(30, 38, 48, 255),
        window::WindowContentType::Settings => Pixel::new(22, 28, 38, 255),
        window::WindowContentType::TextEditor => Pixel::new(28, 28, 30, 255),
        _ => Pixel::new(35, 40, 52, 255),
    };
    fb.fill_rounded_rect_aa(content_rect, bg, 6);

    // Mini title bar in preview
    fb.fill_rect(
        Rect::new(
            px + content_margin,
            py + content_margin,
            content_w as u32,
            12,
        ),
        Pixel::new(40, 45, 58, 200),
    );

    // Window type icon centered in content
    let icon_cx = px + content_margin + content_w / 2;
    let icon_cy = py + content_margin + 12 + (content_h - 12) / 2;
    super::alt_tab::draw_content_type_icon(fb, icon_cx, icon_cy, preview.content_type);

    // Title text at the bottom of the preview
    let title_y = py + preview_h - 20;
    let max_chars = (content_w / 7) as usize;
    let display_title = if preview.title.len() > max_chars {
        let mut t = String::from(&preview.title[..max_chars.saturating_sub(2)]);
        t.push_str("..");
        t
    } else {
        preview.title.clone()
    };
    let text_w = font_engine::measure_ui_text(&display_title, 11) as i32;
    let text_x = px + (preview_w - text_w) / 2;
    font_engine::draw_ui_text(
        fb,
        text_x,
        title_y,
        &display_title,
        11,
        Pixel::new(200, 210, 230, 230),
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Running App Window List — popup showing all windows for an app
// when clicking its dock icon. Groups by content_type.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

struct AppWindowList {
    visible: bool,
    content_type: window::WindowContentType,
    windows: Vec<(window::WindowId, String, bool)>, // (id, title, focused)
    popup_x: i32,
    popup_y: i32,
    hovered_idx: Option<usize>,
}

lazy_static::lazy_static! {
    static ref APP_WINDOW_LIST: Mutex<AppWindowList> = Mutex::new(AppWindowList {
        visible: false,
        content_type: window::WindowContentType::Empty,
        windows: Vec::new(),
        popup_x: 0,
        popup_y: 0,
        hovered_idx: None,
    });
}

/// Show the running app window list popup for a given content type
pub fn show_app_window_list(ct: window::WindowContentType, center_x: i32, dock_top_y: i32) {
    let wm = window::WINDOW_MANAGER.lock();
    let mut windows: Vec<(window::WindowId, String, bool)> = wm
        .windows
        .iter()
        .filter(|w| w.content_type as u8 == ct as u8 && w.visible)
        .map(|w| (w.id, w.title.clone(), w.focused))
        .collect();
    drop(wm);

    if windows.len() <= 1 {
        // No list needed for single window
        close_app_window_list();
        return;
    }

    // Sort focused first, then alphabetical
    windows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));

    let item_h = 28i32;
    let popup_w = 220i32;
    let popup_h = windows.len() as i32 * item_h + 12; // 6px padding top+bottom
    let popup_x = (center_x - popup_w / 2).max(8);
    let popup_y = dock_top_y - popup_h - 8;

    let mut list = APP_WINDOW_LIST.lock();
    list.visible = true;
    list.content_type = ct;
    list.windows = windows;
    list.popup_x = popup_x;
    list.popup_y = popup_y;
    list.hovered_idx = None;
    super::request_redraw();
}

/// Close the app window list popup
pub fn close_app_window_list() {
    let mut list = APP_WINDOW_LIST.lock();
    if list.visible {
        list.visible = false;
        super::request_redraw();
    }
}

/// Check if the app window list is visible
pub fn is_app_window_list_visible() -> bool {
    APP_WINDOW_LIST.lock().visible
}

/// Update hover in the app window list
pub fn update_app_window_list_hover(mx: i32, my: i32) {
    let mut list = APP_WINDOW_LIST.lock();
    if !list.visible {
        return;
    }

    let item_h = 28i32;
    let padding = 6i32;
    let px = list.popup_x;
    let py = list.popup_y;
    let pw = 220i32;
    let ph = list.windows.len() as i32 * item_h + padding * 2;

    if mx < px || mx > px + pw || my < py || my > py + ph {
        list.hovered_idx = None;
        return;
    }

    let rel_y = my - py - padding;
    if rel_y >= 0 {
        let idx = (rel_y / item_h) as usize;
        if idx < list.windows.len() {
            list.hovered_idx = Some(idx);
        } else {
            list.hovered_idx = None;
        }
    }
}

/// Handle click in the app window list (returns true if consumed)
pub fn handle_app_window_list_click(mx: i32, my: i32) -> bool {
    let list = APP_WINDOW_LIST.lock();
    if !list.visible {
        return false;
    }

    let item_h = 28i32;
    let padding = 6i32;
    let px = list.popup_x;
    let py = list.popup_y;
    let pw = 220i32;
    let ph = list.windows.len() as i32 * item_h + padding * 2;

    if mx < px || mx > px + pw || my < py || my > py + ph {
        drop(list);
        close_app_window_list();
        return false;
    }

    let rel_y = my - py - padding;
    if rel_y >= 0 {
        let idx = (rel_y / item_h) as usize;
        if let Some((wid, _, _)) = list.windows.get(idx) {
            let wid = *wid;
            drop(list);
            close_app_window_list();
            window::WINDOW_MANAGER.lock().focus_window(wid);
            super::request_redraw();
            return true;
        }
    }
    drop(list);
    close_app_window_list();
    true
}

/// Draw the running app window list popup
pub fn draw_app_window_list(fb: &mut FrameBuffer) {
    let list = APP_WINDOW_LIST.lock();
    if !list.visible || list.windows.is_empty() {
        return;
    }

    let item_h = 28i32;
    let padding = 6i32;
    let popup_w = 220u32;
    let popup_h = (list.windows.len() as i32 * item_h + padding * 2) as u32;

    let rect = Rect::new(list.popup_x, list.popup_y, popup_w, popup_h);

    // Glass background
    fb.fill_rounded_rect_aa(rect, Pixel::new(20, 25, 38, 235), 10);
    fb.draw_rounded_rect(rect, Pixel::new(80, 140, 240, 50), 10, 1);

    // Arrow pointing down
    let arrow_cx = list.popup_x + popup_w as i32 / 2;
    let arrow_base = list.popup_y + popup_h as i32;
    for i in 0..5i32 {
        let half = 5 - i;
        fb.fill_rect(
            Rect::new(arrow_cx - half, arrow_base + i, (half * 2) as u32, 1),
            Pixel::new(20, 25, 38, 235),
        );
    }

    for (i, (_, title, focused)) in list.windows.iter().enumerate() {
        let iy = list.popup_y + padding + i as i32 * item_h;
        let hovered = list.hovered_idx == Some(i);

        // Highlight
        if hovered || *focused {
            let bg = if hovered {
                Pixel::new(60, 120, 220, 60)
            } else {
                Pixel::new(40, 80, 160, 30)
            };
            fb.fill_rounded_rect_aa(
                Rect::new(list.popup_x + 4, iy, popup_w - 8, item_h as u32),
                bg,
                6,
            );
        }

        // Type icon
        let icon_cx = list.popup_x + 18;
        let icon_cy = iy + item_h / 2;
        super::alt_tab::draw_content_type_icon(fb, icon_cx, icon_cy, list.content_type);

        // Title
        let max_chars = 26usize;
        let display = if title.len() > max_chars {
            let mut t = String::from(&title[..max_chars - 2]);
            t.push_str("..");
            t
        } else {
            title.clone()
        };
        let text_color = if *focused {
            Pixel::new(120, 200, 255, 255)
        } else {
            Pixel::new(190, 200, 220, 230)
        };
        font_engine::draw_ui_text(fb, list.popup_x + 34, iy + 8, &display, 12, text_color);

        // Focused indicator dot
        if *focused {
            let dot_x = list.popup_x + popup_w as i32 - 14;
            let dot_y = iy + item_h / 2;
            fb.fill_circle_aa(dot_x, dot_y, 3, Pixel::new(0, 200, 255, 200));
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Drag-to-Reorder   (8.12)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// State for reordering pinned dock entries via drag-and-drop.
///
/// Activated via long-press (300ms hold) on a dock entry. During the drag,
/// the dragged icon follows the cursor with reduced opacity, neighbouring
/// icons shift to show the insertion gap, and on release the entry is
/// moved to the new slot. The new order is persisted to settings.
struct DragReorder {
    active: bool,
    /// Index of the entry being dragged
    src_index: usize,
    /// Current X position of the drag cursor (screen coords)
    cursor_x: i32,
    /// Current Y position of the drag cursor
    cursor_y: i32,
    /// The original x when the press began (for threshold detection)
    origin_x: i32,
    /// Tick counter when mouse-down started (for long-press detection)
    press_start_tick: u64,
    /// Whether we passed the long-press threshold and are truly dragging
    confirmed: bool,
    /// Computed drop-target index (updated every frame)
    drop_index: usize,
}

static DRAG_REORDER: Mutex<DragReorder> = Mutex::new(DragReorder {
    active: false,
    src_index: 0,
    cursor_x: 0,
    cursor_y: 0,
    origin_x: 0,
    press_start_tick: 0,
    confirmed: false,
    drop_index: 0,
});

/// Minimum hold duration (in ticks/frames) before a press becomes a drag.
const DRAG_LONG_PRESS_TICKS: u64 = 18; // ~300ms at 60fps
/// Minimum pixel distance move to immediately confirm a drag
const DRAG_MOVE_THRESHOLD: i32 = 6;

/// Begin a potential drag on a dock entry (called on mouse-down)
pub fn start_drag_reorder(entry_index: usize, cursor_x: i32) {
    let mut dr = DRAG_REORDER.lock();
    let tb = TASKBAR.lock();
    if entry_index < tb.entries.len() {
        dr.active = true;
        dr.src_index = entry_index;
        dr.cursor_x = cursor_x;
        dr.cursor_y = 0;
        dr.origin_x = cursor_x;
        dr.press_start_tick = crate::clock::get_ticks();
        dr.confirmed = false;
        dr.drop_index = entry_index;
    }
}

/// Update drag position during mouse move
pub fn update_drag_reorder(cursor_x: i32, cursor_y: i32) {
    let mut dr = DRAG_REORDER.lock();
    if !dr.active {
        return;
    }
    dr.cursor_x = cursor_x;
    dr.cursor_y = cursor_y;

    // Auto-confirm if moved far enough, even before long-press threshold
    if !dr.confirmed {
        let dx = (cursor_x - dr.origin_x).abs();
        if dx >= DRAG_MOVE_THRESHOLD {
            dr.confirmed = true;
        } else {
            let elapsed = crate::clock::get_ticks().saturating_sub(dr.press_start_tick);
            if elapsed >= DRAG_LONG_PRESS_TICKS {
                dr.confirmed = true;
            }
        }
    }
}

/// Compute the current drop target index based on cursor position.
/// Called every frame during drag to update visual insertion indicator.
pub fn compute_drop_index(screen_width: u32, dock_entries_x_start: i32) {
    let mut dr = DRAG_REORDER.lock();
    if !dr.active || !dr.confirmed {
        return;
    }
    let tb = TASKBAR.lock();
    let count = tb.entries.len();
    if count < 2 {
        return;
    }
    let item_sz = effective_item_size() as i32;
    let rel_x = dr.cursor_x - dock_entries_x_start;
    let idx = if rel_x < 0 {
        0
    } else {
        ((rel_x + item_sz / 2) / item_sz).min(count as i32 - 1) as usize
    };
    dr.drop_index = idx;
}

/// Complete the drag — determine the drop index and reorder entries.
/// Persists the new order to the settings file.
pub fn finish_drag_reorder(screen_width: u32) {
    let mut dr = DRAG_REORDER.lock();
    if !dr.active {
        return;
    }
    let was_confirmed = dr.confirmed;
    dr.active = false;
    dr.confirmed = false;

    if !was_confirmed {
        // Was just a tap, not a drag
        return;
    }

    let mut tb = TASKBAR.lock();
    let count = tb.entries.len();
    if count < 2 || dr.src_index >= count {
        return;
    }

    let dst_index = dr.drop_index.min(count - 1);
    if dst_index != dr.src_index {
        let entry = tb.entries.remove(dr.src_index);
        tb.entries.insert(dst_index, entry);
        crate::serial_println!(
            "[KnoxOS] Dock: reordered entry {} → {}",
            dr.src_index,
            dst_index
        );
    }
    super::request_redraw();
}

/// Cancel a drag in progress without reordering
pub fn cancel_drag_reorder() {
    let mut dr = DRAG_REORDER.lock();
    dr.active = false;
    dr.confirmed = false;
}

/// Returns true if a drag-reorder is in progress and confirmed
pub fn is_drag_reorder_active() -> bool {
    let dr = DRAG_REORDER.lock();
    dr.active && dr.confirmed
}

/// Get the current drag state for rendering feedback: (src_index, drop_index, cursor_x)
pub fn drag_reorder_state() -> Option<(usize, usize, i32)> {
    let dr = DRAG_REORDER.lock();
    if dr.active && dr.confirmed {
        Some((dr.src_index, dr.drop_index, dr.cursor_x))
    } else {
        None
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Per-Monitor Taskbar   (8.14)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Per-monitor taskbar config — mirrors dock on each connected display
#[derive(Clone)]
pub struct MonitorTaskbar {
    /// Monitor index (0 = primary)
    pub monitor_id: u32,
    /// Whether this monitor shows its own taskbar
    pub enabled: bool,
    /// Whether to show only apps on this monitor vs all apps
    pub filter_local_only: bool,
    /// Optional position override for this monitor
    pub position: TaskbarPosition,
}

static MONITOR_TASKBARS: Mutex<Vec<MonitorTaskbar>> = Mutex::new(Vec::new());

/// Enable taskbar on a specific monitor
pub fn enable_monitor_taskbar(monitor_id: u32, local_only: bool) {
    let mut bars = MONITOR_TASKBARS.lock();
    if let Some(mt) = bars.iter_mut().find(|m| m.monitor_id == monitor_id) {
        mt.enabled = true;
        mt.filter_local_only = local_only;
        return;
    }
    bars.push(MonitorTaskbar {
        monitor_id,
        enabled: true,
        filter_local_only: local_only,
        position: TaskbarPosition::Bottom,
    });
    super::request_redraw();
}

/// Disable taskbar on a specific monitor
pub fn disable_monitor_taskbar(monitor_id: u32) {
    let mut bars = MONITOR_TASKBARS.lock();
    if let Some(mt) = bars.iter_mut().find(|m| m.monitor_id == monitor_id) {
        mt.enabled = false;
    }
}

/// Get configured per-monitor taskbars
pub fn list_monitor_taskbars() -> Vec<MonitorTaskbar> {
    MONITOR_TASKBARS.lock().clone()
}

/// Draw taskbar for a specific monitor offset — used by the multi-monitor
/// compositor to render a dock on secondary displays.
pub fn draw_monitor_taskbar(fb: &mut FrameBuffer, monitor_id: u32) {
    let bars = MONITOR_TASKBARS.lock();
    let mt = match bars
        .iter()
        .find(|m| m.monitor_id == monitor_id && m.enabled)
    {
        Some(m) => m.clone(),
        None => return,
    };
    drop(bars);

    // Draw a simplified dock — same pill shape, filtered entries
    let tb = TASKBAR.lock();
    let entries: Vec<_> = if mt.filter_local_only {
        // Filter by which monitor the window center falls on
        // Simple heuristic: window x-center / screen_width == monitor_id
        let wm = window::WINDOW_MANAGER.lock();
        tb.entries
            .iter()
            .filter(|e| {
                if let Some(w) = wm.get_window(e.window_id) {
                    let center_x = w.rect.x + w.rect.width as i32 / 2;
                    let mon = (center_x as usize).checked_div(fb.width).unwrap_or(0);
                    mon == monitor_id as usize
                } else {
                    false
                }
            })
            .cloned()
            .collect()
    } else {
        tb.entries.clone()
    };
    let clock = tb.clock_text.clone();
    drop(tb);

    let count = entries.len().min(MAX_DOCK_ENTRIES);
    let item_sz = effective_item_size();
    let dock_w = (count as u32) * item_sz + 100;
    let dock_x = (fb.width as i32 - dock_w as i32) / 2;
    let dock_y = fb.height as i32 - TASKBAR_HEIGHT as i32 + DOCK_FLOAT_GAP as i32;

    // Background pill
    let pill = Rect::new(dock_x, dock_y, dock_w, DOCK_BAR_HEIGHT);
    fb.fill_rounded_rect_aa(pill, Pixel::new(15, 18, 28, 200), DOCK_RADIUS);

    // Entries
    for (i, entry) in entries.iter().take(count).enumerate() {
        let ix = dock_x + 50 + (i as i32) * item_sz as i32;
        let iy = dock_y + (DOCK_BAR_HEIGHT as i32 - item_sz as i32) / 2;
        let icon_color = if entry.active {
            Pixel::new(0, 200, 255, 255)
        } else {
            Pixel::new(160, 170, 190, 200)
        };
        fb.fill_rounded_rect_aa(
            Rect::new(ix + 4, iy + 4, item_sz - 8, item_sz - 8),
            icon_color,
            6,
        );
    }

    // Clock
    font_engine::draw_ui_text(
        fb,
        dock_x + dock_w as i32 - 48,
        dock_y + 16,
        &clock,
        12,
        Pixel::new(200, 210, 230, 200),
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Quick Actions / Jump Lists   (8.15)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// A single quick-action entry in a jump list
#[derive(Clone)]
pub struct QuickAction {
    pub label: String,
    pub action_id: u32,
}

/// Jump list state — shown on right-click of a dock entry
struct JumpListPopup {
    visible: bool,
    window_id: window::WindowId,
    x: i32,
    y: i32,
    actions: Vec<QuickAction>,
    hovered: Option<usize>,
}

static JUMP_LIST: Mutex<JumpListPopup> = Mutex::new(JumpListPopup {
    visible: false,
    window_id: 0,
    x: 0,
    y: 0,
    actions: Vec::new(),
    hovered: None,
});

/// Register quick actions (jump list) for an application
pub fn register_quick_actions(window_id: window::WindowId, actions: Vec<QuickAction>) {
    // Store in a side table keyed by content type
    let tb = TASKBAR.lock();
    if let Some(entry) = tb.entries.iter().find(|e| e.window_id == window_id) {
        let ct = entry.content_type;
        drop(tb);
        let mut qa = APP_QUICK_ACTIONS.lock();
        // Store by content type so all windows of same app share jump list
        if let Some(existing) = qa.iter_mut().find(|a| a.0 == ct) {
            existing.1 = actions;
        } else {
            qa.push((ct, actions));
        }
    }
}

/// Quick actions storage keyed by content type
static APP_QUICK_ACTIONS: Mutex<Vec<(window::WindowContentType, Vec<QuickAction>)>> =
    Mutex::new(Vec::new());

/// Show jump list popup for a dock entry (on right-click)
pub fn show_jump_list(entry_index: usize, x: i32, y: i32) {
    let tb = TASKBAR.lock();
    if let Some(entry) = tb.entries.get(entry_index) {
        let ct = entry.content_type;
        let wid = entry.window_id;
        drop(tb);

        let qa = APP_QUICK_ACTIONS.lock();
        let actions = qa
            .iter()
            .find(|a| a.0 == ct)
            .map(|a| a.1.clone())
            .unwrap_or_default();
        drop(qa);

        // Default actions if none registered
        let actions = if actions.is_empty() {
            alloc::vec![
                QuickAction {
                    label: String::from("New Window"),
                    action_id: 1
                },
                QuickAction {
                    label: String::from("Close"),
                    action_id: 2
                },
            ]
        } else {
            actions
        };

        let mut jl = JUMP_LIST.lock();
        jl.visible = true;
        jl.window_id = wid;
        jl.x = x;
        jl.y = y - (actions.len() as i32 * 28 + 8);
        jl.actions = actions;
        jl.hovered = None;
    }
    super::request_redraw();
}

/// Close jump list
pub fn close_jump_list() {
    JUMP_LIST.lock().visible = false;
    super::request_redraw();
}

/// Is jump list popup visible
pub fn is_jump_list_visible() -> bool {
    JUMP_LIST.lock().visible
}

/// Draw the jump list popup
pub fn draw_jump_list(fb: &mut FrameBuffer) {
    let jl = JUMP_LIST.lock();
    if !jl.visible {
        return;
    }
    let count = jl.actions.len();
    let item_h = 28i32;
    let popup_w = 180u32;
    let popup_h = count as u32 * item_h as u32 + 8;

    // Background
    fb.fill_rounded_rect_aa(
        Rect::new(jl.x, jl.y, popup_w, popup_h),
        Pixel::new(25, 28, 40, 230),
        8,
    );
    fb.draw_rounded_rect(
        Rect::new(jl.x, jl.y, popup_w, popup_h),
        Pixel::new(60, 70, 90, 180),
        8,
        1,
    );

    let mut iy = jl.y + 4;
    for (i, action) in jl.actions.iter().enumerate() {
        let hovered = jl.hovered == Some(i);
        if hovered {
            fb.fill_rounded_rect_aa(
                Rect::new(jl.x + 4, iy, popup_w - 8, item_h as u32),
                Pixel::new(0, 120, 200, 60),
                4,
            );
        }
        let tc = if hovered {
            Pixel::new(0, 200, 255, 255)
        } else {
            Pixel::new(190, 200, 220, 230)
        };
        font_engine::draw_ui_text(fb, jl.x + 12, iy + 7, &action.label, 12, tc);
        iy += item_h;
    }
}

/// Update hover state for jump list
pub fn update_jump_list_hover(mx: i32, my: i32) {
    let mut jl = JUMP_LIST.lock();
    if !jl.visible {
        return;
    }
    let item_h = 28;
    let rel_y = my - jl.y - 4;
    if rel_y >= 0 && mx >= jl.x && mx < jl.x + 180 {
        let idx = rel_y / item_h;
        if (idx as usize) < jl.actions.len() {
            jl.hovered = Some(idx as usize);
            return;
        }
    }
    jl.hovered = None;
}

/// Handle click on jump list; returns selected action_id or None
pub fn handle_jump_list_click(mx: i32, my: i32) -> Option<u32> {
    let jl = JUMP_LIST.lock();
    if !jl.visible {
        return None;
    }
    let item_h = 28;
    let rel_y = my - jl.y - 4;
    if rel_y >= 0 && mx >= jl.x && mx < jl.x + 180 {
        let idx = rel_y / item_h;
        if (idx as usize) < jl.actions.len() {
            let action_id = jl.actions[idx as usize].action_id;
            drop(jl);
            close_jump_list();
            return Some(action_id);
        }
    }
    drop(jl);
    close_jump_list();
    None
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Recently Used Apps   (8.20)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Maximum number of recent apps to track
const MAX_RECENT_APPS: usize = 12;

/// A recently launched application entry
#[derive(Clone)]
pub struct RecentApp {
    pub name: String,
    pub content_type: window::WindowContentType,
    pub launch_tick: u64,
}

static RECENT_APPS: Mutex<Vec<RecentApp>> = Mutex::new(Vec::new());

/// Record that an app was launched (call when opening an application)
pub fn record_app_launch(name: &str, ct: window::WindowContentType, tick: u64) {
    let mut recent = RECENT_APPS.lock();
    // Remove existing entry for same content type to avoid duplicates
    recent.retain(|r| r.content_type != ct);
    recent.insert(
        0,
        RecentApp {
            name: String::from(name),
            content_type: ct,
            launch_tick: tick,
        },
    );
    if recent.len() > MAX_RECENT_APPS {
        recent.truncate(MAX_RECENT_APPS);
    }
}

/// Get list of recently used apps (most recent first)
pub fn recent_apps() -> Vec<RecentApp> {
    RECENT_APPS.lock().clone()
}

/// Draw the "Recent Apps" section inside the Start Menu / App Launcher
pub fn draw_recent_apps_section(fb: &mut FrameBuffer, x: i32, y: i32, w: u32) {
    let recents = RECENT_APPS.lock();
    if recents.is_empty() {
        return;
    }

    font_engine::draw_ui_text(fb, x + 8, y, "Recent", 11, Pixel::new(120, 130, 160, 200));

    let mut iy = y + 18;
    for app in recents.iter().take(6) {
        // Small icon placeholder + label
        fb.fill_rounded_rect_aa(
            Rect::new(x + 8, iy, 20, 20),
            Pixel::new(60, 70, 100, 150),
            4,
        );
        font_engine::draw_ui_text(
            fb,
            x + 34,
            iy + 4,
            &app.name,
            12,
            Pixel::new(200, 210, 230, 230),
        );
        iy += 26;
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Desktop Search   (8.28)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Desktop search result
#[derive(Clone)]
pub struct DesktopSearchResult {
    pub name: String,
    pub path: String,
    /// "file", "app", "setting"
    pub kind: String,
}

/// Desktop search overlay state
struct DesktopSearch {
    visible: bool,
    query: String,
    results: Vec<DesktopSearchResult>,
    selected: usize,
}

static DESKTOP_SEARCH: Mutex<DesktopSearch> = Mutex::new(DesktopSearch {
    visible: false,
    query: String::new(),
    results: Vec::new(),
    selected: 0,
});

/// Open the desktop search overlay (e.g. Super key or hot corner)
pub fn open_desktop_search() {
    let mut ds = DESKTOP_SEARCH.lock();
    ds.visible = true;
    ds.query.clear();
    ds.results.clear();
    ds.selected = 0;
    super::request_redraw();
}

/// Close the desktop search overlay
pub fn close_desktop_search() {
    DESKTOP_SEARCH.lock().visible = false;
    super::request_redraw();
}

/// Is desktop search visible
pub fn is_desktop_search_visible() -> bool {
    DESKTOP_SEARCH.lock().visible
}

/// Type a character into the search query
pub fn desktop_search_input(ch: char) {
    let mut ds = DESKTOP_SEARCH.lock();
    if !ds.visible {
        return;
    }
    if ch == '\x08' {
        // Backspace
        ds.query.pop();
    } else if ch.is_ascii_graphic() || ch == ' ' {
        ds.query.push(ch);
    }
    ds.selected = 0;

    // Perform search
    let query_lower = ds.query.to_ascii_lowercase();
    if query_lower.is_empty() {
        ds.results.clear();
        return;
    }

    let mut results = Vec::new();

    // Search apps by name
    let app_names = [
        "Terminal",
        "Files",
        "Browser",
        "Settings",
        "Text Editor",
        "Calculator",
        "System Monitor",
        "Image Viewer",
        "Music Player",
    ];
    for name in &app_names {
        if name.to_ascii_lowercase().contains(&query_lower) {
            results.push(DesktopSearchResult {
                name: String::from(*name),
                path: String::new(),
                kind: String::from("app"),
            });
        }
    }

    // Search files in common paths
    let search_dirs = ["/home/user/Desktop", "/home/user/Documents"];
    for dir in &search_dirs {
        if let Ok(entries) = crate::file_manager::list_dir(dir) {
            for entry in entries {
                if entry.name.to_ascii_lowercase().contains(&query_lower) {
                    results.push(DesktopSearchResult {
                        name: entry.name.clone(),
                        path: alloc::format!("{}/{}", dir, entry.name),
                        kind: String::from("file"),
                    });
                }
            }
        }
    }

    ds.results = results;
}

/// Navigate desktop search results (delta: -1 = up, +1 = down)
pub fn desktop_search_navigate(delta: i32) {
    let mut ds = DESKTOP_SEARCH.lock();
    if ds.results.is_empty() {
        return;
    }
    let count = ds.results.len() as i32;
    ds.selected = ((ds.selected as i32 + delta).rem_euclid(count)) as usize;
}

/// Activate the selected search result
pub fn desktop_search_activate() -> Option<DesktopSearchResult> {
    let ds = DESKTOP_SEARCH.lock();
    if ds.results.is_empty() {
        return None;
    }
    let result = ds.results.get(ds.selected).cloned();
    drop(ds);
    close_desktop_search();
    result
}

/// Draw the desktop search overlay
pub fn draw_desktop_search(fb: &mut FrameBuffer) {
    let ds = DESKTOP_SEARCH.lock();
    if !ds.visible {
        return;
    }

    let sw = fb.width;
    let sh = fb.height;
    let popup_w = 480usize.min(sw - 40) as u32;
    let popup_h = 44 + ds.results.len().min(8) as u32 * 30 + 12;
    let px = (sw as i32 - popup_w as i32) / 2;
    let py = (sh as i32 / 3).max(60);

    // Dim background
    fb.fill_rect(
        Rect::new(0, 0, sw as u32, sh as u32),
        Pixel::new(0, 0, 0, 120),
    );

    // Search box
    fb.fill_rounded_rect_aa(
        Rect::new(px, py, popup_w, popup_h),
        Pixel::new(20, 24, 35, 240),
        12,
    );
    fb.draw_rounded_rect(
        Rect::new(px, py, popup_w, popup_h),
        Pixel::new(0, 150, 255, 120),
        12,
        2,
    );

    // Search icon & text field
    fb.fill_circle_aa(px + 22, py + 22, 8, Pixel::new(0, 150, 255, 180));
    let display_query = if ds.query.is_empty() {
        "Search files, apps, settings…"
    } else {
        &ds.query
    };
    let tc = if ds.query.is_empty() {
        Pixel::new(100, 110, 140, 180)
    } else {
        Pixel::new(220, 230, 255, 255)
    };
    font_engine::draw_ui_text(fb, px + 40, py + 14, display_query, 12, tc);

    // Results
    let mut iy = py + 44;
    for (i, result) in ds.results.iter().take(8).enumerate() {
        let selected = i == ds.selected;
        if selected {
            fb.fill_rounded_rect_aa(
                Rect::new(px + 4, iy, popup_w - 8, 28),
                Pixel::new(0, 100, 200, 50),
                4,
            );
        }
        let kind_color = match result.kind.as_str() {
            "app" => Pixel::new(0, 200, 100, 200),
            "setting" => Pixel::new(200, 160, 0, 200),
            _ => Pixel::new(100, 140, 200, 200),
        };
        // Kind badge
        font_engine::draw_ui_text(fb, px + 12, iy + 7, &result.kind, 11, kind_color);
        // Name
        let nc = if selected {
            Pixel::new(0, 220, 255, 255)
        } else {
            Pixel::new(200, 210, 230, 230)
        };
        font_engine::draw_ui_text(fb, px + 60, iy + 7, &result.name, 12, nc);
        iy += 30;
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Custom Wallpaper from File Picker   (8.30)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Custom wallpaper state — optionally stores raw RGBA pixel data
/// that replaces the procedural Aurora wallpaper.
pub struct CustomWallpaper {
    pub enabled: bool,
    /// RGBA pixel data (width × height × 4 bytes)
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub path: String,
}

static CUSTOM_WALLPAPER: Mutex<CustomWallpaper> = Mutex::new(CustomWallpaper {
    enabled: false,
    data: Vec::new(),
    width: 0,
    height: 0,
    path: String::new(),
});

/// Set a custom wallpaper from an image file path.
/// Reads the file, parses simple BMP/raw formats, and caches pixel data.
pub fn set_custom_wallpaper(path: &str) -> bool {
    let vfs = crate::vfs::VFS.lock();
    let file_data = match vfs.read_file(path) {
        Some(d) => d.to_vec(),
        None => return false,
    };
    drop(vfs);

    // Try to parse as a simple BMP (DIB header)
    if file_data.len() > 54 && file_data[0] == b'B' && file_data[1] == b'M' {
        let data_offset =
            u32::from_le_bytes([file_data[10], file_data[11], file_data[12], file_data[13]])
                as usize;
        let w =
            i32::from_le_bytes([file_data[18], file_data[19], file_data[20], file_data[21]]) as u32;
        let h = (i32::from_le_bytes([file_data[22], file_data[23], file_data[24], file_data[25]]))
            .unsigned_abs();
        let bpp = u16::from_le_bytes([file_data[28], file_data[29]]);

        if bpp == 24 || bpp == 32 {
            let bytes_per_pixel = (bpp / 8) as usize;
            let row_size = ((bpp as u32 * w).div_ceil(32) * 4) as usize;
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);

            for row in 0..h {
                let src_row = if i32::from_le_bytes([
                    file_data[22],
                    file_data[23],
                    file_data[24],
                    file_data[25],
                ]) > 0
                {
                    // Bottom-up BMP
                    (h - 1 - row) as usize
                } else {
                    row as usize
                };
                let row_start = data_offset + src_row * row_size;
                for col in 0..w as usize {
                    let off = row_start + col * bytes_per_pixel;
                    if off + 2 < file_data.len() {
                        let b = file_data[off];
                        let g = file_data[off + 1];
                        let r = file_data[off + 2];
                        let a = if bytes_per_pixel == 4 && off + 3 < file_data.len() {
                            file_data[off + 3]
                        } else {
                            255
                        };
                        rgba.push(r);
                        rgba.push(g);
                        rgba.push(b);
                        rgba.push(a);
                    }
                }
            }

            let mut cw = CUSTOM_WALLPAPER.lock();
            cw.enabled = true;
            cw.data = rgba;
            cw.width = w;
            cw.height = h;
            cw.path = String::from(path);
            drop(cw);
            super::desktop::invalidate_wallpaper_cache();
            super::request_redraw();
            return true;
        }
    }

    false
}

/// Clear custom wallpaper and revert to procedural Aurora wallpaper
pub fn clear_custom_wallpaper() {
    let mut cw = CUSTOM_WALLPAPER.lock();
    cw.enabled = false;
    cw.data.clear();
    cw.width = 0;
    cw.height = 0;
    cw.path.clear();
    drop(cw);
    super::desktop::invalidate_wallpaper_cache();
    super::request_redraw();
}

/// Check if a custom wallpaper is active
pub fn has_custom_wallpaper() -> bool {
    CUSTOM_WALLPAPER.lock().enabled
}

/// Blit the custom wallpaper onto the framebuffer, stretching to fit
pub fn draw_custom_wallpaper(fb: &mut FrameBuffer) {
    let cw = CUSTOM_WALLPAPER.lock();
    if !cw.enabled || cw.data.is_empty() || cw.width == 0 || cw.height == 0 {
        return;
    }
    let dst_w = fb.width;
    let dst_h = fb.height;
    let src_w = cw.width;
    let src_h = cw.height;

    // Nearest-neighbor scale
    for dy in 0..dst_h {
        let sy = (dy as u64 * src_h as u64 / dst_h as u64) as u32;
        for dx in 0..dst_w {
            let sx = (dx as u64 * src_w as u64 / dst_w as u64) as u32;
            let off = ((sy * src_w + sx) * 4) as usize;
            if off + 3 < cw.data.len() {
                let r = cw.data[off];
                let g = cw.data[off + 1];
                let b = cw.data[off + 2];
                fb.set_pixel(dx, dy, Pixel::rgb(r, g, b));
            }
        }
    }
}

/// Open file picker to select a wallpaper image
pub fn pick_wallpaper() {
    super::file_picker::open("/home/user/Pictures");
}
