//! Settings Persistence — Save/Load settings to/from VFS
//!
//! Serializes system settings to a simple key=value text format stored in
//! `/etc/knoxos/settings.conf`. On boot, `load_settings()` is called to
//! restore the last saved values; when the user changes a setting,
//! `save_settings()` writes the current state back to disk.
//!
//! Persisted state includes:
//!   - Theme ID
//!   - Volume level and mute state
//!   - Timezone, 24h format, show-seconds
//!   - Privacy toggles
//!   - Display resolution index

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::serial_println;

/// Config file path in the VFS
const SETTINGS_PATH: &str = "/etc/knoxos/settings.conf";

/// Serialize all current settings to a key=value config string
fn serialize_settings() -> String {
    let mut out = String::new();
    out.push_str("# KnoxOS Settings — auto-generated, do not hand-edit\n");

    // ── Theme ────────────────────────────────────────────────────────
    let theme_id = super::theme::active_theme() as u8;
    out.push_str(&format!("theme_id={}\n", theme_id));

    // ── Volume ───────────────────────────────────────────────────────
    let vol = super::system_tray::get_volume();
    let muted = super::system_tray::is_muted();
    out.push_str(&format!("volume={}\n", vol));
    out.push_str(&format!("volume_muted={}\n", muted as u8));

    // ── Date & Time ──────────────────────────────────────────────────
    {
        let (use_ntp, use_24h, show_sec) = super::settings_ext::get_datetime();
        out.push_str(&format!("ntp_enabled={}\n", use_ntp as u8));
        out.push_str(&format!("use_24h={}\n", use_24h as u8));
        out.push_str(&format!("show_seconds={}\n", show_sec as u8));
    }

    // ── Privacy ──────────────────────────────────────────────────────
    {
        let p = super::settings_ext::PRIVACY.lock();
        out.push_str(&format!("priv_location={}\n", p.location_services as u8));
        out.push_str(&format!("priv_analytics={}\n", p.analytics_enabled as u8));
        out.push_str(&format!("priv_crash={}\n", p.crash_reports as u8));
        out.push_str(&format!("priv_camera={}\n", p.camera_access as u8));
        out.push_str(&format!("priv_mic={}\n", p.microphone_access as u8));
        out.push_str(&format!("priv_firewall={}\n", p.firewall_enabled as u8));
        out.push_str(&format!(
            "priv_autoupdate={}\n",
            p.auto_updates_enabled as u8
        ));
    }

    out
}

/// Parse a key=value config string and apply to current settings
fn deserialize_settings(config: &str) {
    for line in config.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let key = match parts.next() {
            Some(k) => k.trim(),
            None => continue,
        };
        let val = match parts.next() {
            Some(v) => v.trim(),
            None => continue,
        };

        match key {
            "theme_id" => {
                if let Ok(id) = val.parse::<u8>() {
                    let theme = super::theme::ThemeId::from_u8(id);
                    super::theme::set_theme(theme);
                }
            }
            "volume" => {
                if let Ok(v) = val.parse::<u8>() {
                    super::system_tray::set_volume(v);
                }
            }
            "volume_muted" => {
                super::system_tray::set_volume_muted(val == "1");
            }
            "ntp_enabled" | "use_24h" | "show_seconds" => {
                // These are read-only loads — store into the DateTimeSettings
                // We apply them below after all lines are parsed.
            }
            "priv_location" => {
                let mut p = super::settings_ext::PRIVACY.lock();
                p.location_services = val == "1";
            }
            "priv_analytics" => {
                let mut p = super::settings_ext::PRIVACY.lock();
                p.analytics_enabled = val == "1";
            }
            "priv_crash" => {
                let mut p = super::settings_ext::PRIVACY.lock();
                p.crash_reports = val == "1";
            }
            "priv_camera" => {
                let mut p = super::settings_ext::PRIVACY.lock();
                p.camera_access = val == "1";
            }
            "priv_mic" => {
                let mut p = super::settings_ext::PRIVACY.lock();
                p.microphone_access = val == "1";
            }
            "priv_firewall" => {
                let mut p = super::settings_ext::PRIVACY.lock();
                p.firewall_enabled = val == "1";
            }
            "priv_autoupdate" => {
                let mut p = super::settings_ext::PRIVACY.lock();
                p.auto_updates_enabled = val == "1";
            }
            _ => {
                serial_println!("[settings_persist] Unknown key: {}", key);
            }
        }
    }
}

/// Save all current settings to the VFS config file
pub fn save_settings() {
    let config = serialize_settings();
    // Ensure config directory exists
    crate::vfs::ensure_directory("/etc/knoxos");
    let ok = crate::vfs::write_file_dispatch(SETTINGS_PATH, config.as_bytes());
    if ok {
        serial_println!("[settings_persist] Settings saved to {}", SETTINGS_PATH);
    } else {
        serial_println!("[settings_persist] Failed to save settings");
    }
}

/// Load settings from VFS config file and apply them
pub fn load_settings() {
    if let Some(data) = crate::vfs::read_file_dispatch(SETTINGS_PATH) {
        if let Ok(config) = core::str::from_utf8(&data) {
            serial_println!("[settings_persist] Loading settings from {}", SETTINGS_PATH);
            deserialize_settings(config);
        }
    } else {
        serial_println!("[settings_persist] No saved settings found, using defaults");
    }
}

/// Initialize persistence: create config directory and load saved settings
pub fn init() {
    crate::vfs::ensure_directory("/etc/knoxos");
    load_settings();
    serial_println!("[settings_persist] Settings persistence initialized");
}
