/// Dock geometry, position, icon scale, auto-hide, and the global taskbar.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::window;

/// Total dock vertical footprint (including float gap)
pub const TASKBAR_HEIGHT: u32 = 64;

/// Actual dock bar height (the visible pill)
pub(super) const DOCK_BAR_HEIGHT: u32 = 52;

/// Gap between dock and screen bottom edge
pub(super) const DOCK_FLOAT_GAP: u32 = 10;

/// Min dock width (capsule)
pub(super) const DOCK_MIN_WIDTH: u32 = 360;

/// Interior inset so icons sit in the pill's straight section, past the
/// circular end caps (radius + a small gap).
pub(super) const DOCK_PADDING: i32 = DOCK_RADIUS as i32 + 8;

/// Entry item size (icon-only square cells in dock)
pub(super) const DOCK_ITEM_SIZE: u32 = 36;

/// Dock pill corner radius
pub(super) const DOCK_RADIUS: u32 = 18;

/// Entry icon size
pub(super) const ENTRY_ICON_SIZE: u32 = 20;

/// Max visible entries before clipping
pub(super) const MAX_DOCK_ENTRIES: usize = 14;

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
    crate::gui::request_redraw();
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
    crate::gui::request_redraw();
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
    let taskbar_h = crate::gui::scale::taskbar_height();
    let hot_zone = 4i32; // 4px trigger zone at screen bottom
    let mouse_near_bottom = mouse_y >= screen_h as i32 - hot_zone;
    let mouse_in_taskbar = mouse_y >= screen_h as i32 - taskbar_h as i32;

    let should_show = mouse_near_bottom
        || mouse_in_taskbar
        || crate::gui::startmenu::is_visible()
        || crate::gui::system_tray::is_context_menu_open()
        || crate::gui::popups::any_popup_open();

    MOUSE_IN_HOT_ZONE.store(should_show, core::sync::atomic::Ordering::Relaxed);

    let current = AUTO_HIDE_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let step = 6u32; // animation speed (pixels per frame)
    if should_show {
        // Slide in
        if current > 0 {
            let new_offset = current.saturating_sub(step);
            AUTO_HIDE_OFFSET.store(new_offset, core::sync::atomic::Ordering::Relaxed);
            crate::gui::request_redraw();
        }
    } else {
        // Slide out
        if current < taskbar_h {
            let new_offset = (current + step).min(taskbar_h);
            AUTO_HIDE_OFFSET.store(new_offset, core::sync::atomic::Ordering::Relaxed);
            crate::gui::request_redraw();
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
