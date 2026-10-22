//! System tray indicator state.
//!
//! D9.5: icons and context menus paint as Knox GPUI panel children
//! (`gpui_platform/panel.rs` + `gpui_platform/tray.rs`). This module keeps
//! Wi-Fi / volume / battery / Bluetooth atomics the rest of the kernel reads.
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Size of each tray icon cell (legacy dock layout).
pub const TRAY_ITEM_SIZE: u32 = 28;

/// Gap between tray icons (legacy dock layout).
pub const TRAY_ITEM_GAP: i32 = 4;

/// Number of tray icons (Wi-Fi, Volume, Battery, Bluetooth, Notifications).
pub const TRAY_ICON_COUNT: u32 = 5;

/// Total width of the tray area (legacy dock layout).
pub fn tray_width() -> u32 {
    TRAY_ICON_COUNT * TRAY_ITEM_SIZE + (TRAY_ICON_COUNT - 1) * TRAY_ITEM_GAP as u32
}

static WIFI_STRENGTH: AtomicU8 = AtomicU8::new(3);
static WIFI_ENABLED: AtomicBool = AtomicBool::new(true);
static VOLUME_LEVEL: AtomicU8 = AtomicU8::new(75);
static VOLUME_MUTED: AtomicBool = AtomicBool::new(false);
static BATTERY_PERCENT: AtomicU8 = AtomicU8::new(85);
static BATTERY_CHARGING: AtomicBool = AtomicBool::new(true);
static BT_ENABLED: AtomicBool = AtomicBool::new(true);
static BT_CONNECTED: AtomicBool = AtomicBool::new(false);

pub fn is_context_menu_open() -> bool {
    crate::gui::gpui_platform::tray_menu_open()
}

pub fn close_context_menu() {
    crate::gui::gpui_platform::close_tray_menu();
}

pub fn set_wifi_strength(strength: u8) {
    WIFI_STRENGTH.store(strength.min(4), Ordering::Relaxed);
}
pub fn set_wifi_enabled(enabled: bool) {
    WIFI_ENABLED.store(enabled, Ordering::Relaxed);
}
pub fn get_wifi_strength() -> u8 {
    WIFI_STRENGTH.load(Ordering::Relaxed)
}
pub fn is_wifi_enabled() -> bool {
    WIFI_ENABLED.load(Ordering::Relaxed)
}

pub fn set_volume(level: u8) {
    VOLUME_LEVEL.store(level.min(100), Ordering::Relaxed);
}
pub fn set_volume_muted(muted: bool) {
    VOLUME_MUTED.store(muted, Ordering::Relaxed);
}
pub fn get_volume() -> u8 {
    VOLUME_LEVEL.load(Ordering::Relaxed)
}
pub fn is_muted() -> bool {
    VOLUME_MUTED.load(Ordering::Relaxed)
}

pub fn set_battery(percent: u8) {
    BATTERY_PERCENT.store(percent.min(100), Ordering::Relaxed);
}
pub fn set_battery_charging(charging: bool) {
    BATTERY_CHARGING.store(charging, Ordering::Relaxed);
}
pub fn get_battery() -> u8 {
    BATTERY_PERCENT.load(Ordering::Relaxed)
}
pub fn is_charging() -> bool {
    BATTERY_CHARGING.load(Ordering::Relaxed)
}

pub fn set_bluetooth_enabled(enabled: bool) {
    BT_ENABLED.store(enabled, Ordering::Relaxed);
}
pub fn set_bluetooth_connected(connected: bool) {
    BT_CONNECTED.store(connected, Ordering::Relaxed);
}
pub fn is_bluetooth_enabled() -> bool {
    BT_ENABLED.load(Ordering::Relaxed)
}
pub fn is_bluetooth_connected() -> bool {
    BT_CONNECTED.load(Ordering::Relaxed)
}

/// Execute a tray context-menu action (Wi-Fi toggle, mute, …).
pub fn run_menu_action(icon: u8, item: usize) {
    match icon {
        0 => match item {
            0 => {
                let cur = WIFI_ENABLED.load(Ordering::Relaxed);
                WIFI_ENABLED.store(!cur, Ordering::Relaxed);
                if cur {
                    WIFI_STRENGTH.store(0, Ordering::Relaxed);
                } else {
                    WIFI_STRENGTH.store(3, Ordering::Relaxed);
                }
                let msg = if cur {
                    "Wi-Fi disabled"
                } else {
                    "Wi-Fi enabled"
                };
                super::notifications::info("Network", msg);
            }
            1 | 2 => super::popups::toggle_quick_settings(),
            _ => {}
        },
        1 => match item {
            0 => {
                let cur = VOLUME_MUTED.load(Ordering::Relaxed);
                VOLUME_MUTED.store(!cur, Ordering::Relaxed);
                let msg = if cur {
                    "Volume unmuted"
                } else {
                    "Volume muted"
                };
                super::notifications::info("Sound", msg);
            }
            1 => {
                let cur = VOLUME_LEVEL.load(Ordering::Relaxed);
                VOLUME_LEVEL.store(cur.saturating_add(10).min(100), Ordering::Relaxed);
                VOLUME_MUTED.store(false, Ordering::Relaxed);
            }
            2 => {
                let cur = VOLUME_LEVEL.load(Ordering::Relaxed);
                VOLUME_LEVEL.store(cur.saturating_sub(10), Ordering::Relaxed);
            }
            3 => super::popups::toggle_volume(),
            _ => {}
        },
        2 => match item {
            0 => super::popups::toggle_quick_settings(),
            1 => {
                super::notifications::info("Battery", "Power Saver mode toggled");
            }
            2 => {
                let pct = BATTERY_PERCENT.load(Ordering::Relaxed);
                let charging = if BATTERY_CHARGING.load(Ordering::Relaxed) {
                    "charging"
                } else {
                    "discharging"
                };
                let msg = alloc::format!("Battery: {}%, {}", pct, charging);
                super::notifications::info("Battery", &msg);
            }
            _ => {}
        },
        3 => match item {
            0 => {
                let cur = BT_ENABLED.load(Ordering::Relaxed);
                BT_ENABLED.store(!cur, Ordering::Relaxed);
                if cur {
                    BT_CONNECTED.store(false, Ordering::Relaxed);
                }
                let msg = if cur {
                    "Bluetooth disabled"
                } else {
                    "Bluetooth enabled"
                };
                super::notifications::info("Bluetooth", msg);
            }
            1 => {
                super::notifications::info("Bluetooth", "No paired devices");
            }
            2 => super::popups::toggle_quick_settings(),
            _ => {}
        },
        4 => match item {
            0 => {
                super::notifications::NOTIFICATIONS.lock().clear_all();
                super::notifications::info("Notifications", "All notifications cleared");
            }
            1 => {
                super::notifications::info("Notifications", "Do Not Disturb toggled");
            }
            2 => super::notifications::toggle_panel(),
            _ => {}
        },
        _ => {}
    }
}

pub fn init() {
    crate::serial_println!("[KnoxOS] System tray initialized");
}
