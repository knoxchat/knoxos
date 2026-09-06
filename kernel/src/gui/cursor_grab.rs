use super::window::WindowId;
/// CursorGrabMode — winit-inspired cursor confinement system
///
/// Provides cursor grab/lock modes for windows that need to capture the mouse
/// (games, drawing apps, drag operations, full-screen interactions).
///
/// Adapted from `winit/winit-core/src/window.rs`: `CursorGrabMode`.
use core::sync::atomic::{AtomicU8, Ordering};

// ═══════════════════════════════════════════════════════════════════════
// CursorGrabMode — None / Confined / Locked
// ═══════════════════════════════════════════════════════════════════════

/// The behavior of cursor grabbing.
///
/// Use this with `set_cursor_grab()` to control how the cursor is
/// confined within a window.
///
/// Adapted from `winit::window::CursorGrabMode`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum CursorGrabMode {
    /// No grabbing of the cursor is performed (default).
    /// The cursor moves freely across the entire screen.
    #[default]
    None,

    /// The cursor is confined to the window area.
    /// The cursor remains visible but cannot leave the window bounds.
    /// Useful for: drawing applications, map panning, game UIs.
    Confined,

    /// The cursor is locked to a fixed position (typically center of window).
    /// Mouse movement generates relative deltas without moving the cursor.
    /// The cursor is typically hidden in this mode.
    /// Useful for: first-person games, 3D editors, virtual trackballs.
    Locked,
}

// ═══════════════════════════════════════════════════════════════════════
// Global grab state
// ═══════════════════════════════════════════════════════════════════════

/// Currently active grab mode (0=None, 1=Confined, 2=Locked)
static GRAB_MODE: AtomicU8 = AtomicU8::new(0);

/// Window ID that holds the cursor grab (0 = no window)
static GRAB_WINDOW: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Get the current cursor grab mode.
pub fn cursor_grab_mode() -> CursorGrabMode {
    match GRAB_MODE.load(Ordering::Relaxed) {
        1 => CursorGrabMode::Confined,
        2 => CursorGrabMode::Locked,
        _ => CursorGrabMode::None,
    }
}

/// Get the window that currently holds the cursor grab (if any).
pub fn grab_window() -> Option<WindowId> {
    let id = GRAB_WINDOW.load(Ordering::Relaxed);
    if id == 0 { None } else { Some(id) }
}

/// Set the cursor grab mode for a specific window.
///
/// - `CursorGrabMode::None` releases any existing grab.
/// - `CursorGrabMode::Confined` confines the cursor to the window's content area.
/// - `CursorGrabMode::Locked` locks the cursor position and reports deltas.
///
/// Returns `true` if the grab was set successfully, `false` if another window
/// already holds a grab (release it first).
pub fn set_cursor_grab(window_id: WindowId, mode: CursorGrabMode) -> bool {
    let current_grabber = GRAB_WINDOW.load(Ordering::Relaxed);

    match mode {
        CursorGrabMode::None => {
            // Can only release if we own the grab or no one has it
            if current_grabber == 0 || current_grabber == window_id {
                GRAB_MODE.store(0, Ordering::Relaxed);
                GRAB_WINDOW.store(0, Ordering::Relaxed);
                true
            } else {
                false
            }
        }
        CursorGrabMode::Confined | CursorGrabMode::Locked => {
            // Can only acquire if no one else has it or we already have it
            if current_grabber == 0 || current_grabber == window_id {
                let mode_val = if mode == CursorGrabMode::Confined {
                    1
                } else {
                    2
                };
                GRAB_MODE.store(mode_val, Ordering::Relaxed);
                GRAB_WINDOW.store(window_id, Ordering::Relaxed);
                true
            } else {
                false
            }
        }
    }
}

/// Force-release the cursor grab. Called when a grabbed window is closed or minimized.
pub fn force_release_grab(window_id: WindowId) {
    let current = GRAB_WINDOW.load(Ordering::Relaxed);
    if current == window_id {
        GRAB_MODE.store(0, Ordering::Relaxed);
        GRAB_WINDOW.store(0, Ordering::Relaxed);
    }
}

/// Apply cursor confinement to mouse coordinates.
///
/// If `CursorGrabMode::Confined` is active, clamps (x, y) to the grabbing
/// window's content area. If `Locked`, returns the lock-point position.
/// If `None`, returns the coordinates unchanged.
///
/// Call this in the mouse input handler after computing the new position.
pub fn apply_cursor_grab(x: i32, y: i32) -> (i32, i32) {
    match cursor_grab_mode() {
        CursorGrabMode::None => (x, y),
        CursorGrabMode::Confined => {
            if let Some(wid) = grab_window() {
                let wm = super::window::WINDOW_MANAGER.lock();
                if let Some(win) = wm.windows.iter().find(|w| w.id == wid && w.is_visible()) {
                    let cr = win.content_rect();
                    let cx = x.clamp(cr.x, cr.x + cr.width as i32 - 1);
                    let cy = y.clamp(cr.y, cr.y + cr.height as i32 - 1);
                    return (cx, cy);
                }
            }
            // Grab window not found — release the grab
            GRAB_MODE.store(0, Ordering::Relaxed);
            GRAB_WINDOW.store(0, Ordering::Relaxed);
            (x, y)
        }
        CursorGrabMode::Locked => {
            // In locked mode, the cursor stays at the lock point.
            // The delta should be computed by the caller and sent as a relative event.
            if let Some(wid) = grab_window() {
                let wm = super::window::WINDOW_MANAGER.lock();
                if let Some(win) = wm.windows.iter().find(|w| w.id == wid && w.is_visible()) {
                    let cr = win.content_rect();
                    // Lock to center of content area
                    let lock_x = cr.x + cr.width as i32 / 2;
                    let lock_y = cr.y + cr.height as i32 / 2;
                    return (lock_x, lock_y);
                }
            }
            GRAB_MODE.store(0, Ordering::Relaxed);
            GRAB_WINDOW.store(0, Ordering::Relaxed);
            (x, y)
        }
    }
}
