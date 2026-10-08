/// File Explorer logic — Sprint 3 implementation
/// Handles file operations (copy/move/delete/rename), navigation (back/forward/up),
/// address bar editing, sorting, and context menus.
mod actions;
mod address;
mod click;
mod drag;
mod entries;
mod keys;
mod nav;
mod path;
mod preview;
mod search;

pub use actions::{
    build_context_menu, cancel_rename, close_context_menu, confirm_rename, execute_action,
    rename_backspace, rename_char,
};
pub use address::{
    address_bar_backspace, address_bar_char, cancel_address_edit, confirm_address_edit,
    start_address_edit,
};
pub use click::{handle_click, handle_double_click, handle_right_click};
pub use drag::{
    accept_file_drop, cancel_file_drag, dragged_file_path, is_file_drag_active, start_file_drag,
};
pub use entries::{ExplorerFileEntry, read_entries, sort_entries, toggle_sort};
pub use keys::handle_key;
pub use nav::{navigate_back, navigate_forward, navigate_to, navigate_up};
pub use path::current_path;
pub use preview::{is_previewable, load_preview, toggle_grid_view, toggle_preview};
pub use search::{
    cancel_search, filter_entries, search_backspace, search_char, search_recursive, start_search,
};
