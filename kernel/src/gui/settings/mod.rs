/// Settings Panel — Rendered inside a Window with WindowContentType::Settings
/// Provides system configuration tabs: Display, Sound, Network, System, About.
mod about;
mod chrome;
mod click;
mod datetime;
mod display;
mod keyboard;
mod network;
mod personalization;
mod privacy;
mod sound;
mod startup;
mod system;
mod users;
mod widgets;

pub use chrome::draw_settings;
pub use click::handle_settings_click;
pub use display::RES_LIST_Y_START;
pub use keyboard::handle_settings_key;

pub(super) const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};

/// Active tab in settings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Display,
    Sound,
    Network,
    Personalization,
    Users,
    System,
    DateTime,
    Privacy,
    Startup,
    About,
}

/// Settings panel state (per window instance)
pub struct SettingsState {
    pub active_tab: SettingsTab,
    pub scroll_y: i32,
    /// Keyboard focus index: -1 = sidebar tabs area, 0..N = content items
    /// When focus_index < 0 the sidebar is focused; Tab_index in sidebar = -(focus_index+1)
    pub focus_index: i32,
    /// Whether keyboard navigation is active (show focus rings)
    pub keyboard_nav: bool,
}

impl SettingsState {
    pub fn new() -> Self {
        SettingsState {
            active_tab: SettingsTab::Display,
            scroll_y: 0,
            focus_index: -1,
            keyboard_nav: false,
        }
    }

    /// Number of focusable items in the sidebar (always 7 tabs)
    pub fn sidebar_count(&self) -> i32 {
        TABS.len() as i32
    }

    /// Number of focusable content items in the current tab
    pub fn content_item_count(&self) -> i32 {
        match self.active_tab {
            SettingsTab::Display => {
                // Resolution options + brightness slider
                crate::gui::RESOLUTIONS.len() as i32 + 1
            }
            SettingsTab::Sound => {
                // Master volume + system sounds sliders + 3 output devices
                5
            }
            SettingsTab::Network => {
                // Wi-Fi toggle + 3 networks
                4
            }
            SettingsTab::Personalization => {
                // 4 themes + 6 accents + 2 toggles
                12
            }
            SettingsTab::Users => {
                // Lock Screen + Log Out buttons
                2
            }
            SettingsTab::System => {
                // Power saving toggle + 6 keyboard layouts
                7
            }
            SettingsTab::DateTime => {
                // NTP toggle + 24h toggle + seconds toggle + 12 timezone entries
                15
            }
            SettingsTab::Privacy => {
                // 6 privacy toggles (location, analytics, camera, mic, firewall, autoupdate)
                6
            }
            SettingsTab::Startup => {
                // startup apps (toggle each)
                let count = crate::gui::settings_ext::STARTUP.lock().apps.len();
                count as i32
            }
            SettingsTab::About => {
                // No interactive items
                0
            }
        }
    }
}

lazy_static::lazy_static! {
    /// Global settings state — shared between draw and click handlers
    pub static ref SETTINGS_STATE: spin::Mutex<SettingsState> =
        spin::Mutex::new(SettingsState::new());
}

/// Tabs definition
pub(super) const TABS: &[(SettingsTab, &str, &str)] = &[
    (SettingsTab::Display, "Display", "D"),
    (SettingsTab::Sound, "Sound", "S"),
    (SettingsTab::Network, "Network", "N"),
    (SettingsTab::Personalization, "Personalization", "P"),
    (SettingsTab::Users, "Users", "U"),
    (SettingsTab::System, "System", "G"),
    (SettingsTab::DateTime, "Date & Time", "T"),
    (SettingsTab::Privacy, "Privacy", "🔒"),
    (SettingsTab::Startup, "Startup Apps", "⚡"),
    (SettingsTab::About, "About", "i"),
];

pub(super) const SIDEBAR_WIDTH: i32 = 160;
pub(super) const TAB_HEIGHT: i32 = 36;
/// Width of the scrollbar track in the content area
pub(super) const SCROLLBAR_WIDTH: i32 = 8;

/// Focus region: sidebar tabs vs content items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRegion {
    /// Sidebar tab at index 0..6
    Sidebar(i32),
    /// Content item at index 0..N
    Content(i32),
}

impl SettingsState {
    /// Decode current focus_index into a FocusRegion
    pub fn focus_region(&self) -> FocusRegion {
        if self.focus_index < 0 {
            // Sidebar: index is -(focus_index + 1), i.e. focus_index=-1 → tab 0
            let tab_idx = (-(self.focus_index + 1)).clamp(0, self.sidebar_count() - 1);
            FocusRegion::Sidebar(tab_idx)
        } else {
            FocusRegion::Content(self.focus_index)
        }
    }
}
