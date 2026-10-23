// SPDX-License-Identifier: MIT
//! KnoxOS platform backend for Knox GPUI (`desktopDE-impl.md` D0.4+).
//!
//! Compiled with `feature = "gui"` (the parent `gui` crate module is already
//! gated). Live-path GATEs: platform init, scene raster, pump.

use core::sync::atomic::{AtomicBool, Ordering};

mod apps;
mod atlas;
mod browse;
mod calc;
mod chat;
mod desktop;
mod dialogs;
mod dispatcher;
mod editor;
mod files;
mod keys;
mod launcher;
mod notify;
mod osd;
mod panel;
mod platform;
mod pointer;
pub mod proof;
pub mod pump;
mod session;
mod settings;
mod shortcuts;
mod splash;
mod switcher;
mod tasks;
mod term;
mod text_system;
mod tray;
mod window;

pub use apps::{
    dispatch_pointer_down as apps_pointer_down, dispatch_scroll as apps_scroll,
    is_live as apps_is_live, pointer_moved as apps_pointer_moved, present_apps,
};
pub use browse::{
    dispatch_pointer_down as browse_pointer_down, dispatch_scroll as browse_scroll,
    is_live as browse_is_live, pointer_moved as browse_pointer_moved, present_browse,
};
pub use calc::{
    GATE_GPUI_APP_CALC, dispatch_pointer_down as calc_pointer_down, is_live as calc_is_live,
    pointer_moved as calc_pointer_moved, present_calc,
};
pub use chat::{
    dispatch_pointer_down as chat_pointer_down, dispatch_scroll as chat_scroll,
    is_live as chat_is_live, pointer_moved as chat_pointer_moved, present_chat,
};
pub use desktop::{
    GATE_GPUI_DESKTOP, dispatch_icons_pointer_down as desktop_pointer_down,
    is_live as desktop_is_live, menu_is_open as desktop_menu_open,
    present_menu as present_desktop_menu,
};
pub use dialogs::{
    dispatch_pointer_down as dialogs_pointer_down, dispatch_scroll as dialogs_scroll,
    is_live as dialogs_is_live, pointer_moved as dialogs_pointer_moved, present_install,
    present_picker, present_shot, present_wizard,
};
pub use editor::{
    dispatch_pointer_down as editor_pointer_down, dispatch_scroll as editor_scroll,
    is_live as editor_is_live, pointer_moved as editor_pointer_moved, present_editor,
};
pub use files::{
    dispatch_pointer_down as files_pointer_down, is_live as files_is_live,
    pointer_moved as files_pointer_moved, present_files,
};
pub use keys::{
    char_input, claim_key_focus, has_key_focus, key_down, release_key_focus, set_modifiers,
};
pub use knox_gpui::CursorStyle;
pub use launcher::{
    GATE_GPUI_LAUNCHER, is_live as launcher_is_live, present as present_launcher,
    reset_ui as launcher_reset,
};
pub use notify::{
    GATE_GPUI_NOTIFY, close_center as close_notify, damage as notify_damage,
    is_center_open as notify_center_open, is_live as notify_is_live, present as present_notify,
};
pub use osd::{
    FlyoutKind, GATE_GPUI_OSD, close as close_osd, is_live as osd_is_live, is_open as osd_is_open,
    kind as osd_kind, present as present_osd, toggle as toggle_osd,
};
pub use panel::{
    GATE_GPUI_PANEL, GATE_GPUI_TRAY, damage as panel_damage, is_live as panel_is_live,
};
pub use pointer::{
    cursor_style_at, damage_proof, pointer_down, pointer_down_button, pointer_moved, pointer_up,
    scroll,
};
pub use proof::{
    GATE_GPUI_CHROME, GATE_GPUI_INTERACT, GATE_GPUI_KIT_GALLERY, GATE_GPUI_SCENE_RASTER,
    focus_kit_gallery, gallery_hidden, hide_kit_gallery, proof_dest_rect,
};
pub use pump::{GATE_GPUI_MOTION, GATE_GPUI_PUMP, pump, sync_theme};
pub use session::{
    GATE_GPUI_LOGIN, dispatch_pointer_down as session_pointer_down,
    dispatch_pointer_up as session_pointer_up, is_live as session_is_live,
    pointer_moved as session_pointer_moved, present_lock, present_login,
};
pub use settings::{
    dispatch_pointer_down as settings_pointer_down, dispatch_scroll as settings_scroll,
    is_live as settings_is_live, pointer_moved as settings_pointer_moved, present_settings,
};
pub use shortcuts::{
    dispatch_pointer_down as shortcuts_pointer_down, dispatch_pointer_up as shortcuts_pointer_up,
    is_open as shortcuts_open, pointer_moved as shortcuts_pointer_moved,
    present as present_shortcuts,
};
pub use splash::{blit_card as blit_splash_card, card_rect as splash_card_rect};
pub use switcher::{
    GATE_GPUI_ALT_TAB, damage as switcher_damage, drain_pending as switcher_drain,
    is_live as switcher_is_live, is_open as switcher_is_open, present as present_switcher,
};
pub use tasks::{
    dispatch_pointer_down as tasks_pointer_down, dispatch_scroll as tasks_scroll,
    is_live as tasks_is_live, pointer_moved as tasks_pointer_moved, present_tasks,
};
pub use term::{
    CHROME_H as TERM_CHROME_H, dispatch_pointer_down as term_pointer_down, is_live as term_is_live,
    pointer_moved as term_pointer_moved, present_term,
};
pub use tray::{close as close_tray_menu, is_open as tray_menu_open, present as present_tray_menu};

/// Serial marker grepped by `tests/run_integration.sh`.
pub const GATE_GPUI_PLATFORM_INIT: &str = "GATE_GPUI platform init";

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Bring up the GPUI platform after fonts are registered.
pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    platform::boot_app();
    crate::serial_println!(
        "[gpui] {} (knox-gpui {})",
        GATE_GPUI_PLATFORM_INIT,
        knox_gpui::VERSION
    );
}

pub fn is_ready() -> bool {
    INITIALIZED.load(Ordering::SeqCst)
}
