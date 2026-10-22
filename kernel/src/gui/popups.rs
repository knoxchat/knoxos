/// System flyout state — volume, brightness, calendar, quick settings.
///
/// D9.3: paint and pointer handling live in `gpui_platform/osd.rs`. This
/// module keeps the data the rest of the kernel reads (night light, BT, …).
use spin::Mutex;

use super::gpui_platform::{FlyoutKind, close_osd, osd_is_open, toggle_osd};

pub struct CalendarPopup {
    pub visible: bool,
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

lazy_static::lazy_static! {
    pub static ref CALENDAR: Mutex<CalendarPopup> = Mutex::new(CalendarPopup {
        visible: false,
        year: 2026,
        month: 10,
        day: 22,
        hour: 12,
        minute: 0,
        second: 0,
    });
}

pub struct VolumePopup {
    pub visible: bool,
    pub volume: u8,
    pub muted: bool,
}

lazy_static::lazy_static! {
    pub static ref VOLUME: Mutex<VolumePopup> = Mutex::new(VolumePopup {
        visible: false,
        volume: 75,
        muted: false,
    });
}

pub struct QuickSettings {
    pub visible: bool,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub night_light: bool,
    pub airplane_mode: bool,
    pub brightness: u8,
}

lazy_static::lazy_static! {
    pub static ref QUICK_SETTINGS: Mutex<QuickSettings> = Mutex::new(QuickSettings {
        visible: false,
        wifi_enabled: true,
        bluetooth_enabled: false,
        night_light: false,
        airplane_mode: false,
        brightness: 80,
    });
}

pub fn toggle_calendar() {
    toggle_osd(FlyoutKind::Calendar);
}

pub fn is_calendar_visible() -> bool {
    CALENDAR.lock().visible
}

pub fn toggle_volume() {
    toggle_osd(FlyoutKind::Volume);
}

pub fn toggle_brightness() {
    toggle_osd(FlyoutKind::Brightness);
}

pub fn toggle_quick_settings() {
    toggle_osd(FlyoutKind::QuickSettings);
}

pub fn close_all_popups() {
    close_osd();
    CALENDAR.lock().visible = false;
    VOLUME.lock().visible = false;
    QUICK_SETTINGS.lock().visible = false;
}

pub fn any_popup_open() -> bool {
    osd_is_open()
        || CALENDAR.lock().visible
        || VOLUME.lock().visible
        || QUICK_SETTINGS.lock().visible
}
