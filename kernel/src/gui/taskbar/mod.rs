/// Taskbar — "Aurora Dock" — SI-native centered pill-shaped dock
/// Unlike any traditional OS taskbar. Floats as a translucent capsule at
/// the bottom center of the screen with warm app indicators.
mod app_list;
mod context_menu;
mod draw;
mod entries;
mod jump_list;
mod layout;
mod monitor;
mod preview;
mod recent;
mod reorder;
mod search;
mod state;
mod wallpaper;

pub use app_list::{
    close_app_window_list, draw_app_window_list, handle_app_window_list_click,
    is_app_window_list_visible, show_app_window_list, update_app_window_list_hover,
};
pub use context_menu::{
    TASKBAR_CTX_MENU, TaskbarContextMenu, close_context_menu, draw_context_menu,
    handle_context_menu_click, is_context_menu_open, show_context_menu,
};
pub use draw::draw_taskbar;
pub use entries::{
    activate_dock_index, add_entry, clear_badge, clear_hover, get_clock_text, handle_click,
    remove_entry, set_active, set_badge, update_clock, update_hover,
};
pub use jump_list::{
    QuickAction, close_jump_list, draw_jump_list, handle_jump_list_click, is_jump_list_visible,
    register_quick_actions, show_jump_list, update_jump_list_hover,
};
pub use monitor::{
    MonitorTaskbar, disable_monitor_taskbar, draw_monitor_taskbar, enable_monitor_taskbar,
    list_monitor_taskbars,
};
pub use preview::{draw_window_preview, is_preview_visible, update_window_preview};
pub use recent::{RecentApp, draw_recent_apps_section, recent_apps, record_app_launch};
pub use reorder::{
    cancel_drag_reorder, compute_drop_index, drag_reorder_state, finish_drag_reorder,
    is_drag_reorder_active, start_drag_reorder, update_drag_reorder,
};
pub use search::{
    DesktopSearchResult, close_desktop_search, desktop_search_activate, desktop_search_input,
    desktop_search_navigate, draw_desktop_search, is_desktop_search_visible, open_desktop_search,
};
pub use state::{
    TASKBAR, TASKBAR_HEIGHT, Taskbar, TaskbarEntry, TaskbarPosition, auto_hide_offset,
    effective_icon_size, effective_item_size, icon_scale, is_auto_hide, position, set_auto_hide,
    set_icon_scale, set_position, update_auto_hide,
};
pub use wallpaper::{
    CustomWallpaper, clear_custom_wallpaper, draw_custom_wallpaper, has_custom_wallpaper,
    pick_wallpaper, set_custom_wallpaper,
};
