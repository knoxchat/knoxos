//! Settings Extension — Date/Time, Privacy, and Startup Applications settings
//!
//! Extends the Settings application with additional tabs:
//! - Date & Time settings with timezone and NTP configuration
//! - Privacy & Security settings panel
//! - Startup Applications management
//! - Wi-Fi network picker UI
//! - Bluetooth device picker UI
//!
//! Covers status.md items 9.45, 9.46, 9.56, 9.57, 9.58.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════════
// DATE & TIME SETTINGS (9.56)
// ═══════════════════════════════════════════════════════════════════════════

/// Timezone info
#[derive(Debug, Clone)]
pub struct TimezoneInfo {
    pub name: String,
    pub utc_offset_minutes: i32,
    pub abbreviation: String,
}

/// Date/time settings state
pub struct DateTimeSettings {
    pub use_ntp: bool,
    pub ntp_server: String,
    pub timezone: TimezoneInfo,
    pub use_24h_format: bool,
    pub show_seconds: bool,
}

impl Default for DateTimeSettings {
    fn default() -> Self {
        Self {
            use_ntp: true,
            ntp_server: String::from("pool.ntp.org"),
            timezone: TimezoneInfo {
                name: String::from("UTC"),
                utc_offset_minutes: 0,
                abbreviation: String::from("UTC"),
            },
            use_24h_format: false,
            show_seconds: false,
        }
    }
}

/// Common timezones
pub fn common_timezones() -> Vec<TimezoneInfo> {
    let mut tz = Vec::new();
    let entries = [
        ("UTC", 0, "UTC"),
        ("US/Eastern", -300, "EST"),
        ("US/Central", -360, "CST"),
        ("US/Mountain", -420, "MST"),
        ("US/Pacific", -480, "PST"),
        ("Europe/London", 0, "GMT"),
        ("Europe/Berlin", 60, "CET"),
        ("Europe/Moscow", 180, "MSK"),
        ("Asia/Tokyo", 540, "JST"),
        ("Asia/Shanghai", 480, "CST"),
        ("Asia/Kolkata", 330, "IST"),
        ("Australia/Sydney", 600, "AEST"),
    ];
    for (name, offset, abbr) in entries {
        tz.push(TimezoneInfo {
            name: String::from(name),
            utc_offset_minutes: offset,
            abbreviation: String::from(abbr),
        });
    }
    tz
}

// ═══════════════════════════════════════════════════════════════════════════
// PRIVACY & SECURITY SETTINGS (9.57)
// ═══════════════════════════════════════════════════════════════════════════

/// Privacy setting toggles
pub struct PrivacySettings {
    pub location_services: bool,
    pub analytics_enabled: bool,
    pub crash_reports: bool,
    pub camera_access: bool,
    pub microphone_access: bool,
    pub screen_lock_timeout_secs: u32,
    pub require_password_wake: bool,
    pub firewall_enabled: bool,
    pub auto_updates_enabled: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            location_services: false,
            analytics_enabled: false,
            crash_reports: true,
            camera_access: true,
            microphone_access: true,
            screen_lock_timeout_secs: 300,
            require_password_wake: true,
            firewall_enabled: true,
            auto_updates_enabled: true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STARTUP APPLICATIONS (9.58)
// ═══════════════════════════════════════════════════════════════════════════

/// A startup application entry
#[derive(Debug, Clone)]
pub struct StartupApp {
    pub name: String,
    pub command: String,
    pub enabled: bool,
    pub delay_secs: u32,
    pub description: String,
}

/// Startup applications manager
pub struct StartupManager {
    pub apps: Vec<StartupApp>,
}

impl Default for StartupManager {
    fn default() -> Self {
        Self {
            apps: alloc::vec![
                StartupApp {
                    name: String::from("Desktop Environment"),
                    command: String::from("/usr/bin/knox-desktop"),
                    enabled: true,
                    delay_secs: 0,
                    description: String::from("KnoxOS desktop shell"),
                },
                StartupApp {
                    name: String::from("Network Manager"),
                    command: String::from("/usr/sbin/knox-netd"),
                    enabled: true,
                    delay_secs: 1,
                    description: String::from("Network management daemon"),
                },
                StartupApp {
                    name: String::from("AI Assistant"),
                    command: String::from("/usr/bin/knox-ai"),
                    enabled: true,
                    delay_secs: 3,
                    description: String::from("Background AI inference service"),
                },
            ],
        }
    }
}

impl StartupManager {
    /// Add a startup application
    pub fn add(&mut self, name: &str, command: &str, delay: u32) {
        self.apps.push(StartupApp {
            name: String::from(name),
            command: String::from(command),
            enabled: true,
            delay_secs: delay,
            description: String::new(),
        });
    }

    /// Remove a startup application by name
    pub fn remove(&mut self, name: &str) {
        self.apps.retain(|a| a.name != name);
    }

    /// Toggle a startup application
    pub fn toggle(&mut self, name: &str) -> bool {
        if let Some(app) = self.apps.iter_mut().find(|a| a.name == name) {
            app.enabled = !app.enabled;
            return app.enabled;
        }
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WI-FI NETWORK PICKER (9.45)
// ═══════════════════════════════════════════════════════════════════════════

/// Wi-Fi network info for the picker UI
#[derive(Debug, Clone)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal_strength: i32, // dBm (-100 to 0)
    pub is_secured: bool,
    pub is_connected: bool,
    pub frequency_mhz: u32,
}

/// Signal strength to bar count (0-4)
pub fn signal_bars(dbm: i32) -> u8 {
    if dbm >= -50 {
        4
    } else if dbm >= -60 {
        3
    } else if dbm >= -70 {
        2
    } else if dbm >= -80 {
        1
    } else {
        0
    }
}

/// Get available Wi-Fi networks from the real wifi subsystem scan results.
/// Falls back to a static list if no scan results are available (e.g. no wifi hardware).
pub fn scan_wifi_networks() -> Vec<WifiNetwork> {
    let bss_results = crate::wifi::get_scan_results();
    if !bss_results.is_empty() {
        // Convert real BssEntry results to WifiNetwork for the settings UI
        return bss_results
            .iter()
            .map(|bss| {
                let is_secured = bss.security != crate::wifi::BssSecurity::Open;
                WifiNetwork {
                    ssid: bss.ssid.clone(),
                    signal_strength: bss.rssi,
                    is_secured,
                    is_connected: false, // Would need to cross-reference connected SSID
                    frequency_mhz: bss.frequency,
                }
            })
            .collect();
    }

    // Fallback: no scan results available (no wifi hardware or scan not yet run)
    alloc::vec![
        WifiNetwork {
            ssid: String::from("KnoxOS-Lab"),
            signal_strength: -42,
            is_secured: true,
            is_connected: true,
            frequency_mhz: 5180,
        },
        WifiNetwork {
            ssid: String::from("Guest-Network"),
            signal_strength: -65,
            is_secured: false,
            is_connected: false,
            frequency_mhz: 2437,
        },
        WifiNetwork {
            ssid: String::from("Neighbor-5G"),
            signal_strength: -78,
            is_secured: true,
            is_connected: false,
            frequency_mhz: 5240,
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// BLUETOOTH DEVICE PICKER (9.46)
// ═══════════════════════════════════════════════════════════════════════════

/// Bluetooth device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtDeviceType {
    Headphones,
    Speaker,
    Keyboard,
    Mouse,
    Phone,
    Computer,
    Unknown,
}

/// Bluetooth device info for the picker UI
#[derive(Debug, Clone)]
pub struct BtDevice {
    pub name: String,
    pub address: [u8; 6],
    pub device_type: BtDeviceType,
    pub is_paired: bool,
    pub is_connected: bool,
    pub battery_level: Option<u8>,
}

/// Get known Bluetooth devices from real bluetooth subsystem.
/// Falls back to a static demo list when no real devices are discovered.
pub fn known_bt_devices() -> Vec<BtDevice> {
    let discovered = crate::bluetooth::get_discovered_devices();
    if !discovered.is_empty() {
        return discovered
            .iter()
            .map(|dev| {
                // Map class of device to BtDeviceType
                let major_class = (dev.class_of_device >> 8) & 0x1F;
                let device_type = match major_class {
                    4 => BtDeviceType::Headphones, // Audio/Video
                    5 => BtDeviceType::Keyboard,   // Peripheral
                    3 => BtDeviceType::Computer,   // Networking
                    2 => BtDeviceType::Phone,      // Phone
                    _ => BtDeviceType::Unknown,
                };
                BtDevice {
                    name: dev.name.clone(),
                    address: dev.address.0,
                    device_type,
                    is_paired: dev.paired,
                    is_connected: dev.connected,
                    battery_level: None,
                }
            })
            .collect();
    }

    // Fallback: no real BT devices found (demo list)
    alloc::vec![
        BtDevice {
            name: String::from("Knox AirPods"),
            address: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01],
            device_type: BtDeviceType::Headphones,
            is_paired: true,
            is_connected: true,
            battery_level: Some(85),
        },
        BtDevice {
            name: String::from("BT Mouse"),
            address: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x02],
            device_type: BtDeviceType::Mouse,
            is_paired: true,
            is_connected: false,
            battery_level: None,
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref DATETIME: Mutex<DateTimeSettings> = Mutex::new(DateTimeSettings::default());
    pub static ref PRIVACY: Mutex<PrivacySettings> = Mutex::new(PrivacySettings::default());
    pub static ref STARTUP: Mutex<StartupManager> = Mutex::new(StartupManager::default());
}

static SETTINGS_OPENS: AtomicU64 = AtomicU64::new(0);

/// Get date/time settings
pub fn get_datetime() -> (bool, bool, bool) {
    let dt = DATETIME.lock();
    (dt.use_ntp, dt.use_24h_format, dt.show_seconds)
}

/// Set timezone
pub fn set_timezone(name: &str) {
    let tzs = common_timezones();
    if let Some(tz) = tzs.iter().find(|t| t.name == name) {
        DATETIME.lock().timezone = tz.clone();
        crate::serial_println!("[settings_ext] Timezone set to {}", name);
    }
}

/// Toggle a privacy setting
pub fn toggle_privacy(setting: &str) -> bool {
    let mut p = PRIVACY.lock();
    match setting {
        "location" => {
            p.location_services = !p.location_services;
            p.location_services
        }
        "analytics" => {
            p.analytics_enabled = !p.analytics_enabled;
            p.analytics_enabled
        }
        "camera" => {
            p.camera_access = !p.camera_access;
            p.camera_access
        }
        "microphone" => {
            p.microphone_access = !p.microphone_access;
            p.microphone_access
        }
        "firewall" => {
            p.firewall_enabled = !p.firewall_enabled;
            p.firewall_enabled
        }
        "autoupdate" => {
            p.auto_updates_enabled = !p.auto_updates_enabled;
            p.auto_updates_enabled
        }
        _ => false,
    }
}

/// Toggle a startup app
pub fn toggle_startup_app(name: &str) -> bool {
    STARTUP.lock().toggle(name)
}

/// Initialize settings extensions
pub fn init() {
    crate::serial_println!(
        "[settings_ext] Extended settings initialized (DateTime, Privacy, Startup, WiFi, BT)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// BLUETOOTH SETTINGS PANEL
// ═══════════════════════════════════════════════════════════════════════════

/// Bluetooth settings state
pub struct BluetoothSettings {
    pub enabled: bool,
    pub discoverable: bool,
    pub discoverable_timeout_secs: u32,
    pub device_name: String,
    pub paired_devices: Vec<BtDevice>,
}

impl Default for BluetoothSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            discoverable: false,
            discoverable_timeout_secs: 120,
            device_name: String::from("KnoxOS"),
            paired_devices: Vec::new(),
        }
    }
}

lazy_static::lazy_static! {
    static ref BT_SETTINGS: Mutex<BluetoothSettings> = Mutex::new(BluetoothSettings::default());
}

/// Toggle bluetooth on/off
pub fn toggle_bluetooth() -> bool {
    let mut bt = BT_SETTINGS.lock();
    bt.enabled = !bt.enabled;
    crate::serial_println!(
        "[settings_ext] Bluetooth: {}",
        if bt.enabled { "ON" } else { "OFF" }
    );
    bt.enabled
}

/// Toggle discoverable
pub fn toggle_bt_discoverable() -> bool {
    let mut bt = BT_SETTINGS.lock();
    bt.discoverable = !bt.discoverable;
    bt.discoverable
}

/// Pair a bluetooth device
pub fn pair_bt_device(address: &[u8; 6]) -> bool {
    let devices = known_bt_devices();
    if let Some(dev) = devices.iter().find(|d| &d.address == address) {
        let mut bt = BT_SETTINGS.lock();
        let mut d = dev.clone();
        d.is_paired = true;
        bt.paired_devices.push(d);
        true
    } else {
        false
    }
}

/// Remove a paired bluetooth device
pub fn remove_bt_device(address: &[u8; 6]) {
    BT_SETTINGS
        .lock()
        .paired_devices
        .retain(|d| &d.address != address);
}

// ═══════════════════════════════════════════════════════════════════════════
// MOUSE / TOUCHPAD SETTINGS
// ═══════════════════════════════════════════════════════════════════════════

pub struct MouseTouchpadSettings {
    pub mouse_speed: f32, // 0.1 - 3.0
    pub mouse_acceleration: bool,
    pub natural_scrolling: bool,
    pub scroll_speed: f32, // 0.1 - 3.0
    pub double_click_ms: u32,
    pub left_handed: bool,
    // Touchpad
    pub tap_to_click: bool,
    pub two_finger_scroll: bool,
    pub pinch_zoom: bool,
    pub three_finger_swipe: bool,
    pub touchpad_speed: f32,
    pub touchpad_enabled: bool,
}

impl Default for MouseTouchpadSettings {
    fn default() -> Self {
        Self {
            mouse_speed: 1.0,
            mouse_acceleration: true,
            natural_scrolling: false,
            scroll_speed: 1.0,
            double_click_ms: 400,
            left_handed: false,
            tap_to_click: true,
            two_finger_scroll: true,
            pinch_zoom: true,
            three_finger_swipe: true,
            touchpad_speed: 1.0,
            touchpad_enabled: true,
        }
    }
}

lazy_static::lazy_static! {
    static ref MOUSE_SETTINGS: Mutex<MouseTouchpadSettings> = Mutex::new(MouseTouchpadSettings::default());
}

/// Get mouse/touchpad settings
pub fn get_mouse_settings() -> (f32, bool, bool, f32, bool) {
    let ms = MOUSE_SETTINGS.lock();
    (
        ms.mouse_speed,
        ms.mouse_acceleration,
        ms.natural_scrolling,
        ms.scroll_speed,
        ms.left_handed,
    )
}

/// Set mouse speed
pub fn set_mouse_speed(speed: f32) {
    MOUSE_SETTINGS.lock().mouse_speed = speed.clamp(0.1, 3.0);
}

/// Toggle natural scrolling
pub fn toggle_natural_scrolling() -> bool {
    let mut ms = MOUSE_SETTINGS.lock();
    ms.natural_scrolling = !ms.natural_scrolling;
    ms.natural_scrolling
}

/// Toggle tap to click
pub fn toggle_tap_to_click() -> bool {
    let mut ms = MOUSE_SETTINGS.lock();
    ms.tap_to_click = !ms.tap_to_click;
    ms.tap_to_click
}

// ═══════════════════════════════════════════════════════════════════════════
// LANGUAGE & REGION SETTINGS
// ═══════════════════════════════════════════════════════════════════════════

pub struct LanguageRegionSettings {
    pub language: String,
    pub region: String,
    pub input_sources: Vec<String>,
    pub number_format: String, // e.g., "1,234.56" or "1.234,56"
    pub date_format: String,   // e.g., "MM/DD/YYYY" or "DD.MM.YYYY"
    pub first_day_of_week: u8, // 0=Sunday, 1=Monday
    pub measurement: String,   // "metric" or "imperial"
}

impl Default for LanguageRegionSettings {
    fn default() -> Self {
        Self {
            language: String::from("en_US"),
            region: String::from("US"),
            input_sources: alloc::vec![String::from("English (US)")],
            number_format: String::from("1,234.56"),
            date_format: String::from("MM/DD/YYYY"),
            first_day_of_week: 0,
            measurement: String::from("imperial"),
        }
    }
}

lazy_static::lazy_static! {
    static ref LANG_SETTINGS: Mutex<LanguageRegionSettings> = Mutex::new(LanguageRegionSettings::default());
}

/// Supported languages
pub fn supported_languages() -> Vec<(&'static str, &'static str)> {
    alloc::vec![
        ("en_US", "English (US)"),
        ("en_GB", "English (UK)"),
        ("de_DE", "Deutsch"),
        ("fr_FR", "Français"),
        ("es_ES", "Español"),
        ("it_IT", "Italiano"),
        ("pt_BR", "Português (BR)"),
        ("ja_JP", "日本語"),
        ("zh_CN", "中文 (简体)"),
        ("ko_KR", "한국어"),
        ("ru_RU", "Русский"),
        ("ar_SA", "العربية"),
    ]
}

/// Set system language
pub fn set_language(lang: &str) {
    LANG_SETTINGS.lock().language = String::from(lang);
    crate::serial_println!("[settings_ext] Language set to {}", lang);
}

/// Get current language
pub fn get_language() -> String {
    LANG_SETTINGS.lock().language.clone()
}

/// Set measurement system
pub fn set_measurement(system: &str) {
    LANG_SETTINGS.lock().measurement = String::from(system);
}

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT APPLICATIONS SETTINGS
// ═══════════════════════════════════════════════════════════════════════════

pub struct DefaultApps {
    pub web_browser: String,
    pub email_client: String,
    pub file_manager: String,
    pub terminal: String,
    pub text_editor: String,
    pub image_viewer: String,
    pub music_player: String,
    pub video_player: String,
    pub pdf_viewer: String,
}

impl Default for DefaultApps {
    fn default() -> Self {
        Self {
            web_browser: String::from("knox-browser"),
            email_client: String::from("knox-mail"),
            file_manager: String::from("knox-files"),
            terminal: String::from("knox-terminal"),
            text_editor: String::from("knox-edit"),
            image_viewer: String::from("knox-viewer"),
            music_player: String::from("knox-music"),
            video_player: String::from("knox-player"),
            pdf_viewer: String::from("knox-pdf"),
        }
    }
}

lazy_static::lazy_static! {
    static ref DEFAULT_APPS: Mutex<DefaultApps> = Mutex::new(DefaultApps::default());
}

/// Set default app for a category
pub fn set_default_app(category: &str, app: &str) {
    let mut defaults = DEFAULT_APPS.lock();
    match category {
        "browser" => defaults.web_browser = String::from(app),
        "email" => defaults.email_client = String::from(app),
        "files" => defaults.file_manager = String::from(app),
        "terminal" => defaults.terminal = String::from(app),
        "editor" => defaults.text_editor = String::from(app),
        "image" => defaults.image_viewer = String::from(app),
        "music" => defaults.music_player = String::from(app),
        "video" => defaults.video_player = String::from(app),
        "pdf" => defaults.pdf_viewer = String::from(app),
        _ => {}
    }
    crate::serial_println!("[settings_ext] Default '{}' = {}", category, app);
}

/// Get default app for a category
pub fn get_default_app(category: &str) -> String {
    let defaults = DEFAULT_APPS.lock();
    match category {
        "browser" => defaults.web_browser.clone(),
        "email" => defaults.email_client.clone(),
        "files" => defaults.file_manager.clone(),
        "terminal" => defaults.terminal.clone(),
        "editor" => defaults.text_editor.clone(),
        "image" => defaults.image_viewer.clone(),
        "music" => defaults.music_player.clone(),
        "video" => defaults.video_player.clone(),
        "pdf" => defaults.pdf_viewer.clone(),
        _ => String::new(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FIREWALL SETTINGS UI
// ═══════════════════════════════════════════════════════════════════════════

pub struct FirewallSettings {
    pub enabled: bool,
    pub default_incoming: String, // "deny", "allow"
    pub default_outgoing: String,
    pub rules: Vec<FirewallRule>,
}

#[derive(Debug, Clone)]
pub struct FirewallRule {
    pub port: u16,
    pub protocol: String, // "tcp", "udp"
    pub action: String,   // "allow", "deny"
    pub description: String,
}

impl Default for FirewallSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            default_incoming: String::from("deny"),
            default_outgoing: String::from("allow"),
            rules: alloc::vec![
                FirewallRule {
                    port: 22,
                    protocol: String::from("tcp"),
                    action: String::from("allow"),
                    description: String::from("SSH")
                },
                FirewallRule {
                    port: 80,
                    protocol: String::from("tcp"),
                    action: String::from("allow"),
                    description: String::from("HTTP")
                },
                FirewallRule {
                    port: 443,
                    protocol: String::from("tcp"),
                    action: String::from("allow"),
                    description: String::from("HTTPS")
                },
            ],
        }
    }
}

lazy_static::lazy_static! {
    static ref FIREWALL_SETTINGS: Mutex<FirewallSettings> = Mutex::new(FirewallSettings::default());
}

/// Toggle firewall
pub fn toggle_firewall() -> bool {
    let mut fw = FIREWALL_SETTINGS.lock();
    fw.enabled = !fw.enabled;
    fw.enabled
}

/// Add firewall rule
pub fn add_firewall_rule(port: u16, protocol: &str, action: &str, desc: &str) {
    FIREWALL_SETTINGS.lock().rules.push(FirewallRule {
        port,
        protocol: String::from(protocol),
        action: String::from(action),
        description: String::from(desc),
    });
}

/// Remove firewall rule by port
pub fn remove_firewall_rule(port: u16) {
    FIREWALL_SETTINGS.lock().rules.retain(|r| r.port != port);
}

// ═══════════════════════════════════════════════════════════════════════════
// NOTIFICATION SETTINGS PER APP
// ═══════════════════════════════════════════════════════════════════════════

pub struct AppNotificationSetting {
    pub app_name: String,
    pub enabled: bool,
    pub show_banner: bool,
    pub play_sound: bool,
    pub show_in_center: bool,
    pub show_on_lockscreen: bool,
}

lazy_static::lazy_static! {
    static ref APP_NOTIF_SETTINGS: Mutex<Vec<AppNotificationSetting>> = Mutex::new(Vec::new());
}

/// Set notification preferences for an app
pub fn set_app_notification(app: &str, enabled: bool, banner: bool, sound: bool) {
    let mut settings = APP_NOTIF_SETTINGS.lock();
    if let Some(entry) = settings.iter_mut().find(|s| s.app_name == app) {
        entry.enabled = enabled;
        entry.show_banner = banner;
        entry.play_sound = sound;
    } else {
        settings.push(AppNotificationSetting {
            app_name: String::from(app),
            enabled,
            show_banner: banner,
            play_sound: sound,
            show_in_center: true,
            show_on_lockscreen: true,
        });
    }
}

/// Get notification setting for an app
pub fn get_app_notification(app: &str) -> (bool, bool, bool) {
    let settings = APP_NOTIF_SETTINGS.lock();
    settings
        .iter()
        .find(|s| s.app_name == app)
        .map(|s| (s.enabled, s.show_banner, s.play_sound))
        .unwrap_or((true, true, true))
}

// ═══════════════════════════════════════════════════════════════════════════
// SHARING SETTINGS
// ═══════════════════════════════════════════════════════════════════════════

pub struct SharingSettings {
    pub screen_sharing: bool,
    pub file_sharing: bool,
    pub remote_login: bool,
    pub file_sharing_path: String,
}

impl Default for SharingSettings {
    fn default() -> Self {
        Self {
            screen_sharing: false,
            file_sharing: false,
            remote_login: false,
            file_sharing_path: String::from("/home/Public"),
        }
    }
}

lazy_static::lazy_static! {
    static ref SHARING: Mutex<SharingSettings> = Mutex::new(SharingSettings::default());
}

/// Toggle screen sharing
pub fn toggle_screen_sharing() -> bool {
    let mut s = SHARING.lock();
    s.screen_sharing = !s.screen_sharing;
    s.screen_sharing
}

/// Toggle file sharing
pub fn toggle_file_sharing() -> bool {
    let mut s = SHARING.lock();
    s.file_sharing = !s.file_sharing;
    s.file_sharing
}

/// Toggle remote login
pub fn toggle_remote_login() -> bool {
    let mut s = SHARING.lock();
    s.remote_login = !s.remote_login;
    s.remote_login
}
