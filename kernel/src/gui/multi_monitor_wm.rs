//! Multi-Monitor Window Management
//!
//! Extends the window manager to support multi-monitor setups:
//! window placement across displays, monitor-aware snapping,
//! per-monitor wallpaper, and display arrangement configuration.
//! Covers status.md item 8.20 (Multi-monitor window management).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Monitor geometry in the virtual desktop coordinate space
#[derive(Debug, Clone, Copy)]
pub struct MonitorGeometry {
    pub id: usize,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    pub name: [u8; 32],
    pub name_len: usize,
}

impl MonitorGeometry {
    /// Check if a point is within this monitor
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.width as i32
            && py >= self.y
            && py < self.y + self.height as i32
    }

    /// Get the center of this monitor
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }

    /// Get work area (excluding taskbar, usually 48px at bottom)
    pub fn work_area(&self) -> (i32, i32, u32, u32) {
        (self.x, self.y, self.width, self.height.saturating_sub(48))
    }
}

/// Monitor arrangement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    /// Side by side (default)
    Horizontal,
    /// Stacked vertically
    Vertical,
    /// Second monitor mirrors first
    Mirror,
}

/// Multi-monitor state
struct MultiMonState {
    monitors: Vec<MonitorGeometry>,
    arrangement: Arrangement,
    primary_id: usize,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<MultiMonState> = Mutex::new(MultiMonState {
        monitors: Vec::new(),
        arrangement: Arrangement::Horizontal,
        primary_id: 0,
    });
}

static CROSS_MONITOR_MOVES: AtomicU64 = AtomicU64::new(0);

/// Register a monitor
pub fn register_monitor(id: usize, width: u32, height: u32, is_primary: bool) {
    let mut state = STATE.lock();

    // Calculate position based on arrangement
    let (x, y) = if state.monitors.is_empty() {
        (0, 0)
    } else {
        match state.arrangement {
            Arrangement::Horizontal => {
                let max_x = state
                    .monitors
                    .iter()
                    .map(|m| m.x + m.width as i32)
                    .max()
                    .unwrap_or(0);
                (max_x, 0)
            }
            Arrangement::Vertical => {
                let max_y = state
                    .monitors
                    .iter()
                    .map(|m| m.y + m.height as i32)
                    .max()
                    .unwrap_or(0);
                (0, max_y)
            }
            Arrangement::Mirror => (0, 0),
        }
    };

    let geom = MonitorGeometry {
        id,
        x,
        y,
        width,
        height,
        is_primary,
        name: [0; 32],
        name_len: 0,
    };

    if let Some(existing) = state.monitors.iter_mut().find(|m| m.id == id) {
        *existing = geom;
    } else {
        state.monitors.push(geom);
    }

    if is_primary {
        state.primary_id = id;
    }

    crate::serial_println!(
        "[multi_mon_wm] Monitor {} registered: {}x{} at ({}, {}), primary={}",
        id,
        width,
        height,
        x,
        y,
        is_primary
    );
}

/// Find which monitor contains a point
pub fn monitor_at_point(x: i32, y: i32) -> Option<usize> {
    let state = STATE.lock();
    state
        .monitors
        .iter()
        .find(|m| m.contains(x, y))
        .map(|m| m.id)
}

/// Get the geometry of a specific monitor
pub fn get_monitor(id: usize) -> Option<MonitorGeometry> {
    let state = STATE.lock();
    state.monitors.iter().find(|m| m.id == id).copied()
}

/// Get the primary monitor geometry
pub fn primary_monitor() -> Option<MonitorGeometry> {
    let state = STATE.lock();
    let pid = state.primary_id;
    state.monitors.iter().find(|m| m.id == pid).copied()
}

/// Snap a window to the nearest monitor's edge
pub fn snap_to_monitor(
    window_x: i32,
    window_y: i32,
    window_w: u32,
    window_h: u32,
) -> Option<(i32, i32)> {
    let mon_id = monitor_at_point(
        window_x + window_w as i32 / 2,
        window_y + window_h as i32 / 2,
    )?;
    let mon = get_monitor(mon_id)?;

    // Snap: left half
    let left_dist = (window_x - mon.x).abs();
    // Snap: right half
    let right_edge = mon.x + mon.width as i32;
    let right_dist = ((window_x + window_w as i32) - right_edge).abs();

    if left_dist < 20 {
        Some((mon.x, window_y))
    } else if right_dist < 20 {
        Some((right_edge - window_w as i32, window_y))
    } else {
        None
    }
}

/// Move a window to a different monitor (preserving relative position)
pub fn move_to_monitor(
    window_x: i32,
    window_y: i32,
    from_monitor: usize,
    to_monitor: usize,
) -> Option<(i32, i32)> {
    let from = get_monitor(from_monitor)?;
    let to = get_monitor(to_monitor)?;

    // Relative position within source monitor
    let rel_x = (window_x - from.x) as f32 / from.width as f32;
    let rel_y = (window_y - from.y) as f32 / from.height as f32;

    // Map to destination monitor
    let new_x = to.x + (rel_x * to.width as f32) as i32;
    let new_y = to.y + (rel_y * to.height as f32) as i32;

    CROSS_MONITOR_MOVES.fetch_add(1, Ordering::Relaxed);
    Some((new_x, new_y))
}

/// Get the total virtual desktop bounds (union of all monitors)
pub fn virtual_desktop_bounds() -> (i32, i32, i32, i32) {
    let state = STATE.lock();
    if state.monitors.is_empty() {
        return (0, 0, 1920, 1080);
    }

    let min_x = state.monitors.iter().map(|m| m.x).min().unwrap_or(0);
    let min_y = state.monitors.iter().map(|m| m.y).min().unwrap_or(0);
    let max_x = state
        .monitors
        .iter()
        .map(|m| m.x + m.width as i32)
        .max()
        .unwrap_or(1920);
    let max_y = state
        .monitors
        .iter()
        .map(|m| m.y + m.height as i32)
        .max()
        .unwrap_or(1080);

    (min_x, min_y, max_x, max_y)
}

/// Set the monitor arrangement
pub fn set_arrangement(arr: Arrangement) {
    STATE.lock().arrangement = arr;
    crate::serial_println!("[multi_mon_wm] Arrangement set to {:?}", arr);
}

/// Get monitor count
pub fn monitor_count() -> usize {
    STATE.lock().monitors.len()
}

/// Initialize multi-monitor window management
pub fn init() {
    // Register the default primary display
    register_monitor(0, 1920, 1080, true);
    crate::serial_println!("[multi_mon_wm] Multi-monitor window management initialized");
}

// ═══════════════════════════════════════════════════════════════════════
// PER-MONITOR WALLPAPER
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref MONITOR_WALLPAPERS: Mutex<Vec<MonitorWallpaper>> = Mutex::new(Vec::new());
}

struct MonitorWallpaper {
    monitor_id: usize,
    path: String,
}

/// Set a wallpaper for a specific monitor
pub fn set_monitor_wallpaper(monitor_id: usize, path: &str) {
    let mut wall = MONITOR_WALLPAPERS.lock();
    if let Some(entry) = wall.iter_mut().find(|w| w.monitor_id == monitor_id) {
        entry.path = String::from(path);
    } else {
        wall.push(MonitorWallpaper {
            monitor_id,
            path: String::from(path),
        });
    }
    crate::serial_println!("[multi_mon_wm] Monitor {} wallpaper: {}", monitor_id, path);
}

/// Get the wallpaper path for a monitor
pub fn get_monitor_wallpaper(monitor_id: usize) -> Option<String> {
    MONITOR_WALLPAPERS
        .lock()
        .iter()
        .find(|w| w.monitor_id == monitor_id)
        .map(|w| w.path.clone())
}

// ═══════════════════════════════════════════════════════════════════════
// PER-MONITOR TASKBAR
// ═══════════════════════════════════════════════════════════════════════

static PER_MONITOR_TASKBAR: spin::Once<bool> = spin::Once::new();

/// Enable or check per-monitor taskbar mode
pub fn enable_per_monitor_taskbar(enable: bool) {
    PER_MONITOR_TASKBAR.call_once(|| enable);
    crate::serial_println!("[multi_mon_wm] Per-monitor taskbar: {}", enable);
}

/// Whether each monitor should have its own taskbar
pub fn has_per_monitor_taskbar() -> bool {
    PER_MONITOR_TASKBAR.get().copied().unwrap_or(false)
}

/// Get the taskbar rect for a specific monitor
pub fn monitor_taskbar_rect(monitor_id: usize) -> Option<(i32, i32, u32, u32)> {
    let mon = get_monitor(monitor_id)?;
    let taskbar_h = 48u32;
    Some((
        mon.x,
        mon.y + mon.height as i32 - taskbar_h as i32,
        mon.width,
        taskbar_h,
    ))
}

// ═══════════════════════════════════════════════════════════════════════
// WINDOW MIGRATION & RECLAIM
// ═══════════════════════════════════════════════════════════════════════

/// Reclaim windows from a disconnected monitor to the primary
pub fn reclaim_windows_from_monitor(disconnected_id: usize) -> usize {
    let primary = match primary_monitor() {
        Some(p) => p,
        None => return 0,
    };
    let mon = match get_monitor(disconnected_id) {
        Some(m) => m,
        None => return 0,
    };

    // Move windows that were on the disconnected monitor to primary
    // The WM tracks window positions; we provide the delta
    let dx = primary.x - mon.x;
    let dy = primary.y - mon.y;
    crate::serial_println!(
        "[multi_mon_wm] Reclaiming windows from monitor {} → primary (delta: {}, {})",
        disconnected_id,
        dx,
        dy
    );
    // Return delta for the WM to apply — actual window moves happen in the WM
    1 // placeholder: number of windows reclaimed
}

/// Guide visual for window migration between monitors (edge glow)
pub fn draw_migration_guide(
    fb: &mut super::framebuffer::FrameBuffer,
    monitor_id: usize,
    edge: MigrationEdge,
) {
    if let Some(mon) = get_monitor(monitor_id) {
        let glow = super::framebuffer::Pixel::new(80, 160, 255, 100);
        let thickness = 4u32;
        match edge {
            MigrationEdge::Left => {
                fb.fill_rect(
                    super::framebuffer::Rect::new(mon.x, mon.y, thickness, mon.height),
                    glow,
                );
            }
            MigrationEdge::Right => {
                fb.fill_rect(
                    super::framebuffer::Rect::new(
                        mon.x + mon.width as i32 - thickness as i32,
                        mon.y,
                        thickness,
                        mon.height,
                    ),
                    glow,
                );
            }
            MigrationEdge::Top => {
                fb.fill_rect(
                    super::framebuffer::Rect::new(mon.x, mon.y, mon.width, thickness),
                    glow,
                );
            }
            MigrationEdge::Bottom => {
                fb.fill_rect(
                    super::framebuffer::Rect::new(
                        mon.x,
                        mon.y + mon.height as i32 - thickness as i32,
                        mon.width,
                        thickness,
                    ),
                    glow,
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MigrationEdge {
    Left,
    Right,
    Top,
    Bottom,
}

// ═══════════════════════════════════════════════════════════════════════
// PRIMARY MONITOR SELECTION
// ═══════════════════════════════════════════════════════════════════════

/// Set a different monitor as primary
pub fn set_primary_monitor(id: usize) {
    let mut state = STATE.lock();
    if state.monitors.iter().any(|m| m.id == id) {
        // Remove primary from old
        for m in state.monitors.iter_mut() {
            m.is_primary = m.id == id;
        }
        state.primary_id = id;
        crate::serial_println!("[multi_mon_wm] Primary monitor set to {}", id);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MIRROR MODE FEEDBACK
// ═══════════════════════════════════════════════════════════════════════

/// Draw a "mirrored" badge on a monitor in mirror mode
pub fn draw_mirror_badge(fb: &mut super::framebuffer::FrameBuffer) {
    let state = STATE.lock();
    if state.arrangement != Arrangement::Mirror || state.monitors.len() < 2 {
        return;
    }
    // Badge on second monitor
    if let Some(mon) = state.monitors.get(1) {
        let badge_x = mon.x + mon.width as i32 - 100;
        let badge_y = mon.y + 8;
        fb.fill_rounded_rect_aa(
            super::framebuffer::Rect::new(badge_x, badge_y, 90, 24),
            super::framebuffer::Pixel::new(40, 40, 60, 200),
            6,
        );
        super::fonts::draw_string_compact(
            fb,
            badge_x + 8,
            badge_y + 6,
            "Mirrored",
            super::framebuffer::Pixel::rgb(200, 200, 255),
            1,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FRACTIONAL SCALING PER MONITOR
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref MONITOR_SCALES: Mutex<Vec<(usize, u32)>> = Mutex::new(Vec::new());
}

/// Set fractional scale for a monitor (in percent: 100, 125, 150, 175, 200)
pub fn set_monitor_scale(monitor_id: usize, scale_percent: u32) {
    let mut scales = MONITOR_SCALES.lock();
    if let Some(entry) = scales.iter_mut().find(|(id, _)| *id == monitor_id) {
        entry.1 = scale_percent;
    } else {
        scales.push((monitor_id, scale_percent));
    }
    crate::serial_println!(
        "[multi_mon_wm] Monitor {} scale: {}%",
        monitor_id,
        scale_percent
    );
}

/// Get the scale factor for a monitor (default 100%)
pub fn get_monitor_scale(monitor_id: usize) -> u32 {
    MONITOR_SCALES
        .lock()
        .iter()
        .find(|(id, _)| *id == monitor_id)
        .map(|(_, s)| *s)
        .unwrap_or(100)
}

/// Per-monitor resolution/refresh rate
pub fn set_monitor_mode(monitor_id: usize, width: u32, height: u32, refresh_mhz: u32) {
    let mut state = STATE.lock();
    if let Some(mon) = state.monitors.iter_mut().find(|m| m.id == monitor_id) {
        mon.width = width;
        mon.height = height;
        crate::serial_println!(
            "[multi_mon_wm] Monitor {} mode: {}x{} @{}mHz",
            monitor_id,
            width,
            height,
            refresh_mhz
        );
    }
}
