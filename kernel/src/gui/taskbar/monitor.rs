/// Per-monitor taskbar config — mirrors the dock on each connected display.
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::window;

use super::state::{
    DOCK_BAR_HEIGHT, DOCK_FLOAT_GAP, DOCK_RADIUS, MAX_DOCK_ENTRIES, TASKBAR, TASKBAR_HEIGHT,
    TaskbarPosition, effective_item_size,
};

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
    crate::gui::request_redraw();
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
}
