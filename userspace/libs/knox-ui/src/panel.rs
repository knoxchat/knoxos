// SPDX-License-Identifier: MIT
//! Session panel / dock (D9.1) with live system-tray children (D9.5).
//! Knox GPUI view, not `taskbar.rs` / `system_tray.rs` paint.
//! Time lives on the desktop clock widget, not in this dock.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, MouseButton, Pixels, Styled, div, px};

use crate::button::{Button, ButtonVariants};
use crate::dock::{DOCK_BAR_H, Dock};
use crate::icon::IconName;
use crate::interact::{Handler, IndexHandler};
use crate::menu::{MenuItem, PopupMenu};
use crate::overlay::FocusTrap;
use crate::size::{Selectable, Sizable};
use crate::theme::Theme;

type TrayHandler = Arc<dyn Fn(TrayIcon) + Send + Sync>;

/// Running-app slot on the panel.
#[derive(Clone, Debug)]
pub struct PanelEntry {
    pub title: String,
    pub icon: IconName,
    pub active: bool,
}

impl PanelEntry {
    pub fn new(title: impl Into<String>, icon: IconName) -> Self {
        Self {
            title: title.into(),
            icon,
            active: false,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

/// Which tray icon was activated (matches the old `system_tray` index order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayIcon {
    Network,
    Volume,
    Battery,
    Bluetooth,
    Notify,
}

impl TrayIcon {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Network),
            1 => Some(Self::Volume),
            2 => Some(Self::Battery),
            3 => Some(Self::Bluetooth),
            4 => Some(Self::Notify),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Network => 0,
            Self::Volume => 1,
            Self::Battery => 2,
            Self::Bluetooth => 3,
            Self::Notify => 4,
        }
    }

    /// Right-click menu labels (same actions as the old fill_rect tray menu).
    pub fn menu_labels(self) -> &'static [&'static str] {
        match self {
            Self::Network => &["Toggle Wi-Fi", "Network Settings", "Available Networks"],
            Self::Volume => &["Toggle Mute", "Volume Up", "Volume Down", "Sound Settings"],
            Self::Battery => &["Battery Settings", "Power Saver Mode", "Battery Health"],
            Self::Bluetooth => &["Toggle Bluetooth", "Paired Devices", "Bluetooth Settings"],
            Self::Notify => &["Clear All", "Do Not Disturb", "Notification Settings"],
        }
    }

    /// Backing-store size for the live GPUI tray menu window.
    pub fn menu_size(self) -> (i32, i32) {
        let n = self.menu_labels().len() as i32;
        (188, n * 28 + 16)
    }
}

/// Live indicator values painted into the panel tray cluster (D9.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayStatus {
    pub wifi: bool,
    pub wifi_strength: u8,
    pub volume: u8,
    pub muted: bool,
    pub brightness: u8,
    pub battery: u8,
    pub charging: bool,
    pub bluetooth: bool,
    pub unread: u32,
}

impl Default for TrayStatus {
    fn default() -> Self {
        Self {
            wifi: true,
            wifi_strength: 3,
            volume: 75,
            muted: false,
            brightness: 80,
            battery: 85,
            charging: true,
            bluetooth: true,
            unread: 0,
        }
    }
}

/// Dock / panel chrome (D9.1) with live tray icons (D9.5).
pub struct Panel {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    entries: Vec<PanelEntry>,
    start_open: bool,
    tray: TrayStatus,
    volume_open: bool,
    brightness_open: bool,
    qs_open: bool,
    notify_open: bool,
    on_start: Option<Handler>,
    on_app: Option<IndexHandler>,
    on_network: Option<Handler>,
    on_volume: Option<Handler>,
    on_brightness: Option<Handler>,
    on_battery: Option<Handler>,
    on_bluetooth: Option<Handler>,
    on_notify: Option<Handler>,
    on_tray_right: Option<TrayHandler>,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(560),
            height: px(DOCK_BAR_H),
            entries: Vec::new(),
            start_open: false,
            tray: TrayStatus::default(),
            volume_open: false,
            brightness_open: false,
            qs_open: false,
            notify_open: false,
            on_start: None,
            on_app: None,
            on_network: None,
            on_volume: None,
            on_brightness: None,
            on_battery: None,
            on_bluetooth: None,
            on_notify: None,
            on_tray_right: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn entries(mut self, entries: impl IntoIterator<Item = PanelEntry>) -> Self {
        self.entries = entries.into_iter().collect();
        self
    }

    pub fn start_open(mut self, open: bool) -> Self {
        self.start_open = open;
        self
    }

    pub fn tray(mut self, tray: TrayStatus) -> Self {
        self.tray = tray;
        self
    }

    pub fn on_start(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_start = Some(Arc::new(f));
        self
    }

    pub fn on_app(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_app = Some(Arc::new(f));
        self
    }

    pub fn on_network(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_network = Some(Arc::new(f));
        self
    }

    pub fn volume_open(mut self, open: bool) -> Self {
        self.volume_open = open;
        self
    }

    pub fn brightness_open(mut self, open: bool) -> Self {
        self.brightness_open = open;
        self
    }

    pub fn qs_open(mut self, open: bool) -> Self {
        self.qs_open = open;
        self
    }

    pub fn notify_open(mut self, open: bool) -> Self {
        self.notify_open = open;
        self
    }

    pub fn on_volume(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_volume = Some(Arc::new(f));
        self
    }

    pub fn on_brightness(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_brightness = Some(Arc::new(f));
        self
    }

    pub fn on_battery(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_battery = Some(Arc::new(f));
        self
    }

    pub fn on_bluetooth(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_bluetooth = Some(Arc::new(f));
        self
    }

    pub fn on_notify(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_notify = Some(Arc::new(f));
        self
    }

    pub fn on_tray_right(mut self, f: impl Fn(TrayIcon) + Send + Sync + 'static) -> Self {
        self.on_tray_right = Some(Arc::new(f));
        self
    }
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

fn bind_right(icon: TrayIcon, handler: &Option<TrayHandler>) -> Option<Handler> {
    handler.as_ref().map(|f| {
        let f = f.clone();
        let h: Handler = Arc::new(move || f(icon));
        h
    })
}

fn tray_chip(
    theme: &Theme,
    id: &'static str,
    icon: IconName,
    selected: bool,
    on_left: Option<Handler>,
    on_right: Option<Handler>,
) -> Div {
    let btn = Button::new(id)
        .themed(theme)
        .ghost()
        .icon(icon)
        .selected(selected)
        .compact()
        .small();
    let mut el = btn.into_element();
    el = el.on_mouse_down(move |e| match e.button {
        MouseButton::Left => {
            if let Some(h) = &on_left {
                h();
            }
        }
        MouseButton::Right => {
            if let Some(h) = &on_right {
                h();
            }
        }
        _ => {}
    });
    el
}

impl IntoElement for Panel {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut start = Button::new("panel-start")
            .themed(&self.theme)
            .ghost()
            .compact()
            .large()
            .icon(IconName::LayoutGrid)
            .selected(self.start_open);
        if let Some(s) = self.on_start {
            start = start.on_click(move || s());
        }
        let mut dock = Dock::new()
            .themed(&self.theme)
            .size(self.width, self.height)
            .add_leading(start);
        for (i, entry) in self.entries.into_iter().enumerate() {
            let pick = self.on_app.clone();
            dock = dock.add_item(
                Button::new(alloc::format!("panel-app-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .compact()
                    .large()
                    .icon(entry.icon)
                    .selected(entry.active)
                    .on_click(move || {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    }),
            );
        }

        let right = self.on_tray_right;
        let network = tray_chip(
            &self.theme,
            "panel-net",
            IconName::Wifi,
            self.tray.wifi || self.qs_open,
            self.on_network,
            bind_right(TrayIcon::Network, &right),
        );
        let volume = tray_chip(
            &self.theme,
            "panel-vol",
            if self.tray.muted {
                IconName::VolumeX
            } else {
                IconName::Volume2
            },
            self.volume_open || self.tray.muted,
            self.on_volume,
            bind_right(TrayIcon::Volume, &right),
        );
        let brightness = tray_chip(
            &self.theme,
            "panel-bri",
            IconName::Sun,
            self.brightness_open,
            self.on_brightness,
            None,
        );
        let battery = tray_chip(
            &self.theme,
            "panel-bat",
            if self.tray.charging {
                IconName::BatteryCharging
            } else {
                IconName::Battery
            },
            self.tray.charging,
            self.on_battery,
            bind_right(TrayIcon::Battery, &right),
        );
        let bluetooth = tray_chip(
            &self.theme,
            "panel-bt",
            IconName::Bluetooth,
            self.tray.bluetooth,
            self.on_bluetooth,
            bind_right(TrayIcon::Bluetooth, &right),
        );
        let notify = tray_chip(
            &self.theme,
            "panel-n",
            IconName::Bell,
            self.notify_open || self.tray.unread > 0,
            self.on_notify,
            bind_right(TrayIcon::Notify, &right),
        );

        dock.add_trailing(network)
            .add_trailing(volume)
            .add_trailing(brightness)
            .add_trailing(battery)
            .add_trailing(bluetooth)
            .add_trailing(notify)
            .into_element()
    }
}

/// Right-click tray menu (D9.5). Separate GPUI overlay so it is not clipped
/// by the dock panel window.
pub struct TrayMenu {
    theme: Theme,
    icon: TrayIcon,
    width: Pixels,
    height: Pixels,
    on_pick: Option<IndexHandler>,
    on_dismiss: Option<Handler>,
}

impl TrayMenu {
    pub fn new(icon: TrayIcon) -> Self {
        let (w, h) = icon.menu_size();
        Self {
            theme: Theme::default(),
            icon,
            width: px(w),
            height: px(h),
            on_pick: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_pick = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl IntoElement for TrayMenu {
    type Element = Div;

    fn into_element(self) -> Div {
        let items: Vec<MenuItem> = self
            .icon
            .menu_labels()
            .iter()
            .map(|l| MenuItem::new(*l))
            .collect();
        let menu = PopupMenu::new("tray-menu")
            .themed(&self.theme)
            .items(items)
            .open(true)
            .on_pick({
                let p = self.on_pick.clone();
                move |i| {
                    if let Some(h) = &p {
                        h(i);
                    }
                }
            })
            .on_dismiss({
                let d = self.on_dismiss.clone();
                move || {
                    if let Some(h) = &d {
                        h();
                    }
                }
            });
        let mut card = div()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.surface.opacity(0.96))
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.md)
            .child(menu)
            .focusable();
        if let Some(dismiss) = self.on_dismiss.clone() {
            card = card.on_key_down(move |e| {
                if e.key == knox_gpui::Key::Escape {
                    dismiss();
                    true
                } else {
                    false
                }
            });
        }
        let trap = if let Some(d) = self.on_dismiss {
            FocusTrap::new(card).on_escape(move || d())
        } else {
            FocusTrap::new(card)
        };
        div().w(self.width).h(self.height).child(trap)
    }
}

#[cfg(test)]
mod tests {
    use super::{Panel, PanelEntry, TrayIcon, TrayMenu, TrayStatus};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, MouseButton, Point, Size, WindowInput,
        dispatch_key_down, dispatch_mouse_down, dispatch_mouse_down_button, dispatch_mouse_up,
        prepare_element,
    };

    fn click_at(el: &mut knox_gpui::Div, input: &mut WindowInput, p: Point) {
        dispatch_mouse_down(el, input, p);
        dispatch_mouse_up(el, input, p);
    }

    fn tab_stops(input: &WindowInput) -> alloc::vec::Vec<Point> {
        let mut boxes: alloc::vec::Vec<_> = input
            .hitboxes()
            .iter()
            .filter(|h| h.tab_stop)
            .map(|h| Point::xy(h.bounds.x() + h.bounds.width() / 2, h.bounds.y() + 8))
            .collect();
        boxes.sort_by_key(|p| p.x.0);
        boxes
    }

    #[test]
    fn start_button_toggles() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = Panel::new()
            .themed(&Theme::aurora_dark())
            .entries([PanelEntry::new("Term", crate::icon::IconName::SquareTerminal).active(true)])
            .on_start(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(560, 52), &mut input);
        dispatch_mouse_down(&mut el, &mut input, Point::xy(16, 16));
        dispatch_mouse_up(&mut el, &mut input, Point::xy(16, 16));
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn network_volume_live() {
        let net = Arc::new(AtomicBool::new(false));
        let vol = Arc::new(AtomicBool::new(false));
        let n = net.clone();
        let v = vol.clone();
        let mut el = Panel::new()
            .themed(&Theme::aurora_dark())
            .tray(TrayStatus {
                wifi: true,
                wifi_strength: 3,
                volume: 40,
                muted: false,
                brightness: 80,
                battery: 85,
                charging: false,
                bluetooth: true,
                unread: 0,
            })
            .on_network(move || n.store(true, Ordering::SeqCst))
            .on_volume(move || v.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(640, 52), &mut input);
        let stops = tab_stops(&input);
        assert!(stops.len() >= 3, "start + tray chips, got {}", stops.len());
        dispatch_mouse_down_button(&mut el, &mut input, stops[1], MouseButton::Left);
        assert!(net.load(Ordering::SeqCst), "network chip");
        dispatch_mouse_down_button(&mut el, &mut input, stops[2], MouseButton::Left);
        assert!(vol.load(Ordering::SeqCst), "volume chip");
    }

    #[test]
    fn tray_right_click_opens_icon() {
        let icon = Arc::new(AtomicU32::new(255));
        let slot = icon.clone();
        let mut el = Panel::new()
            .themed(&Theme::aurora_dark())
            .on_tray_right(move |i| slot.store(i.as_u8() as u32, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(640, 52), &mut input);
        let stops = tab_stops(&input);
        dispatch_mouse_down_button(&mut el, &mut input, stops[1], MouseButton::Right);
        assert_eq!(
            icon.load(Ordering::SeqCst),
            TrayIcon::Network.as_u8() as u32
        );
        dispatch_mouse_down_button(&mut el, &mut input, stops[2], MouseButton::Right);
        assert_eq!(icon.load(Ordering::SeqCst), TrayIcon::Volume.as_u8() as u32);
    }

    #[test]
    fn tray_menu_pick_and_escape() {
        let pick = Arc::new(AtomicU32::new(99));
        let esc = Arc::new(AtomicBool::new(false));
        let p = pick.clone();
        let e = esc.clone();
        let (w, h) = TrayIcon::Volume.menu_size();
        let mut el = TrayMenu::new(TrayIcon::Volume)
            .themed(&Theme::aurora_dark())
            .on_pick(move |i| p.store(i as u32, Ordering::SeqCst))
            .on_dismiss(move || e.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let row = input
            .hitboxes()
            .iter()
            .find(|hb| hb.bounds.width() >= 140 && hb.bounds.height() <= 32)
            .map(|hb| Point::xy(hb.bounds.x() + 12, hb.bounds.y() + 8))
            .expect("menu row");
        click_at(&mut el, &mut input, row);
        assert_eq!(pick.load(Ordering::SeqCst), 0);

        let esc2 = esc.clone();
        let mut el = TrayMenu::new(TrayIcon::Network)
            .themed(&Theme::aurora_dark())
            .on_dismiss(move || esc2.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Escape, Modifiers::NONE),
        );
        assert!(esc.load(Ordering::SeqCst));
    }

    #[test]
    fn tray_icon_menu_sizes() {
        for icon in [
            TrayIcon::Network,
            TrayIcon::Volume,
            TrayIcon::Battery,
            TrayIcon::Bluetooth,
            TrayIcon::Notify,
        ] {
            let (w, h) = icon.menu_size();
            assert!(w >= 180);
            assert!(h >= 40);
            assert!(!icon.menu_labels().is_empty());
        }
    }
}
