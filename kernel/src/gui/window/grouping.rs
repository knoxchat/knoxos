/// Window grouping / tabbed windows
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU32;

use super::manager::WINDOW_MANAGER;
use super::types::WindowId;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Window Grouping / Tabbed Windows
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

static NEXT_GROUP_ID: AtomicU32 = AtomicU32::new(1);

/// Allocate a new unique group ID
fn next_group_id() -> u32 {
    NEXT_GROUP_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

/// Group two windows together into a tabbed group.
/// If either already belongs to a group, the other joins that group.
/// If neither has a group, a new group is created.
pub fn group_windows(id_a: WindowId, id_b: WindowId) {
    let mut wm = WINDOW_MANAGER.lock();
    let gid_a = wm
        .windows
        .iter()
        .find(|w| w.id == id_a)
        .map(|w| w.group_id)
        .unwrap_or(0);
    let gid_b = wm
        .windows
        .iter()
        .find(|w| w.id == id_b)
        .map(|w| w.group_id)
        .unwrap_or(0);

    let gid = if gid_a != 0 {
        gid_a
    } else if gid_b != 0 {
        gid_b
    } else {
        next_group_id()
    };

    for w in wm.windows.iter_mut() {
        if w.id == id_a || w.id == id_b {
            w.group_id = gid;
        }
    }
    // Make id_b the active tab
    for w in wm.windows.iter_mut() {
        if w.group_id == gid {
            w.group_active = w.id == id_b;
        }
    }
}

/// Remove a window from its group. If only one window remains in the
/// group, ungroup it too.
pub fn ungroup_window(id: WindowId) {
    let mut wm = WINDOW_MANAGER.lock();
    let gid = wm
        .windows
        .iter()
        .find(|w| w.id == id)
        .map(|w| w.group_id)
        .unwrap_or(0);
    if gid == 0 {
        return;
    }

    // Remove from group
    if let Some(w) = wm.windows.iter_mut().find(|w| w.id == id) {
        w.group_id = 0;
        w.group_active = true;
    }

    // Count remaining in group
    let remaining: Vec<WindowId> = wm
        .windows
        .iter()
        .filter(|w| w.group_id == gid)
        .map(|w| w.id)
        .collect();

    if remaining.len() == 1 {
        // Only one left — ungroup it
        if let Some(w) = wm.windows.iter_mut().find(|w| w.id == remaining[0]) {
            w.group_id = 0;
            w.group_active = true;
        }
    } else if !remaining.is_empty() {
        // Ensure at least one is active
        let has_active = wm
            .windows
            .iter()
            .any(|w| w.group_id == gid && w.group_active);
        if !has_active {
            let first = remaining[0];
            if let Some(w) = wm.windows.iter_mut().find(|w| w.id == first) {
                w.group_active = true;
            }
        }
    }
}

/// Switch the active tab within a group
pub fn switch_group_tab(gid: u32, target_id: WindowId) {
    let mut wm = WINDOW_MANAGER.lock();
    for w in wm.windows.iter_mut() {
        if w.group_id == gid {
            w.group_active = w.id == target_id;
        }
    }
}

/// Get all windows in a group (by group id), returns (id, title, is_active)
pub fn get_group_members(gid: u32) -> Vec<(WindowId, String, bool)> {
    let wm = WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.group_id == gid)
        .map(|w| (w.id, w.title.clone(), w.group_active))
        .collect()
}
