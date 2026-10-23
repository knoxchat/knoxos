/// Shared dock geometry so draw / hover / click stay aligned.
use crate::gui::system_tray;

use super::state::{
    DOCK_BAR_HEIGHT, DOCK_FLOAT_GAP, DOCK_ITEM_SIZE, DOCK_MIN_WIDTH, DOCK_PADDING, auto_hide_offset,
};

pub(super) struct DockLayout {
    pub(super) dock_x: i32,
    pub(super) dock_y: i32,
    pub(super) dock_w: u32,
    pub(super) dock_cy: i32,
    /// Left edge of the launcher icon (already inset past the circular cap).
    pub(super) content_x: i32,
    pub(super) entry_spacing: i32,
    pub(super) start_btn_w: i32,
    pub(super) separator_w: i32,
    pub(super) workspace_area_w: i32,
    pub(super) workspace_dot_size: i32,
    pub(super) workspace_dot_gap: i32,
    pub(super) workspace_count: i32,
}

pub(super) fn compute_dock_layout(
    entry_count: usize,
    screen_width: u32,
    screen_height: u32,
) -> DockLayout {
    let entry_spacing = 8i32;
    let start_btn_w = DOCK_ITEM_SIZE as i32;
    let separator_w = 12i32;
    let tray_w = system_tray::tray_width() as i32;
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
        + tray_w;
    let dock_w = (dock_content_w + DOCK_PADDING * 2).max(DOCK_MIN_WIDTH as i32) as u32;

    let dock_x = (screen_width as i32 - dock_w as i32) / 2;
    let hide_offset = auto_hide_offset() as i32;
    let dock_y =
        screen_height as i32 - DOCK_BAR_HEIGHT as i32 - DOCK_FLOAT_GAP as i32 + hide_offset;
    let dock_cy = dock_y + DOCK_BAR_HEIGHT as i32 / 2;
    // Leftover min-width space is split so both circular caps stay empty.
    let extra = (dock_w as i32 - DOCK_PADDING * 2 - dock_content_w).max(0);
    let content_x = dock_x + DOCK_PADDING + extra / 2;

    DockLayout {
        dock_x,
        dock_y,
        dock_w,
        dock_cy,
        content_x,
        entry_spacing,
        start_btn_w,
        separator_w,
        workspace_area_w,
        workspace_dot_size,
        workspace_dot_gap,
        workspace_count,
    }
}
