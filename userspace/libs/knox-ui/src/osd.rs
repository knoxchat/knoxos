// SPDX-License-Identifier: MIT
//! Session OSD / flyouts (D9.3): volume, brightness, calendar, quick settings.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS shell chrome, not a kit catalog control.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{Div, IntoElement, Key, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::calendar::{Calendar, Date};
use crate::interact::{BoolHandler, Handler};
use crate::overlay::FocusTrap;
use crate::separator::Separator;
use crate::size::{Selectable, Sizable};
use crate::slider::Slider;
use crate::theme::Theme;

type F32Handler = Arc<dyn Fn(f32) + Send + Sync>;
type DateHandler = Arc<dyn Fn(Date) + Send + Sync>;

/// Which tray popover is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyoutKind {
    Volume,
    Brightness,
    Calendar,
    QuickSettings,
}

impl FlyoutKind {
    /// Backing-store size for the live GPUI window.
    pub fn size(self) -> (i32, i32) {
        match self {
            Self::Volume => (248, 132),
            Self::Brightness => (248, 112),
            Self::Calendar => (292, 408),
            Self::QuickSettings => (360, 292),
        }
    }
}

/// Anchored session flyout (volume / brightness OSD, calendar, quick settings).
pub struct OsdFlyout {
    theme: Theme,
    kind: FlyoutKind,
    width: Pixels,
    height: Pixels,
    volume: f32,
    muted: bool,
    brightness: f32,
    wifi: bool,
    bluetooth: bool,
    night_light: bool,
    airplane: bool,
    battery: u8,
    time_label: String,
    date_label: String,
    cal_view: Date,
    cal_selected: Date,
    on_volume: Option<F32Handler>,
    on_mute: Option<Handler>,
    on_brightness: Option<F32Handler>,
    on_wifi: Option<BoolHandler>,
    on_bluetooth: Option<BoolHandler>,
    on_night_light: Option<BoolHandler>,
    on_airplane: Option<BoolHandler>,
    on_cal_select: Option<DateHandler>,
    on_cal_view: Option<DateHandler>,
    on_dismiss: Option<Handler>,
}

impl OsdFlyout {
    pub fn new(kind: FlyoutKind) -> Self {
        let (w, h) = kind.size();
        Self {
            theme: Theme::default(),
            kind,
            width: px(w),
            height: px(h),
            volume: 75.0,
            muted: false,
            brightness: 80.0,
            wifi: true,
            bluetooth: false,
            night_light: false,
            airplane: false,
            battery: 100,
            time_label: String::from("12:00 PM"),
            date_label: String::from("Thursday, October 22, 2026"),
            cal_view: Date::new(2026, 10, 1),
            cal_selected: Date::new(2026, 10, 22),
            on_volume: None,
            on_mute: None,
            on_brightness: None,
            on_wifi: None,
            on_bluetooth: None,
            on_night_light: None,
            on_airplane: None,
            on_cal_select: None,
            on_cal_view: None,
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

    pub fn volume(mut self, value: f32) -> Self {
        self.volume = value.clamp(0.0, 100.0);
        self
    }

    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    pub fn brightness(mut self, value: f32) -> Self {
        self.brightness = value.clamp(0.0, 100.0);
        self
    }

    pub fn wifi(mut self, on: bool) -> Self {
        self.wifi = on;
        self
    }

    pub fn bluetooth(mut self, on: bool) -> Self {
        self.bluetooth = on;
        self
    }

    pub fn night_light(mut self, on: bool) -> Self {
        self.night_light = on;
        self
    }

    pub fn airplane(mut self, on: bool) -> Self {
        self.airplane = on;
        self
    }

    pub fn battery(mut self, percent: u8) -> Self {
        self.battery = percent.min(100);
        self
    }

    pub fn time_label(mut self, label: impl Into<String>) -> Self {
        self.time_label = label.into();
        self
    }

    pub fn date_label(mut self, label: impl Into<String>) -> Self {
        self.date_label = label.into();
        self
    }

    pub fn cal_view(mut self, view: Date) -> Self {
        self.cal_view = Date::new(view.year, view.month.clamp(1, 12), 1);
        self
    }

    pub fn cal_selected(mut self, selected: Date) -> Self {
        self.cal_selected = selected.clamp();
        self
    }

    pub fn on_volume(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_volume = Some(Arc::new(f));
        self
    }

    pub fn on_mute(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_mute = Some(Arc::new(f));
        self
    }

    pub fn on_brightness(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_brightness = Some(Arc::new(f));
        self
    }

    pub fn on_wifi(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_wifi = Some(Arc::new(f));
        self
    }

    pub fn on_bluetooth(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_bluetooth = Some(Arc::new(f));
        self
    }

    pub fn on_night_light(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_night_light = Some(Arc::new(f));
        self
    }

    pub fn on_airplane(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_airplane = Some(Arc::new(f));
        self
    }

    pub fn on_cal_select(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_cal_select = Some(Arc::new(f));
        self
    }

    pub fn on_cal_view(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_cal_view = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for OsdFlyout {
    fn default() -> Self {
        Self::new(FlyoutKind::Volume)
    }
}

fn slider_row(
    theme: &Theme,
    label: &str,
    value: f32,
    on_change: Option<F32Handler>,
    width: Pixels,
) -> Div {
    let mut slider = Slider::new()
        .themed(theme)
        .value(value)
        .min(0.0)
        .max(100.0)
        .step(1.0)
        .w(width)
        .small();
    if let Some(h) = on_change {
        slider = slider.on_change(move |v| h(v));
    }
    div()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    text(String::from(label))
                        .color(theme.colors.muted_foreground)
                        .size(12),
                )
                .child(
                    text(format!("{:.0}", value))
                        .color(theme.colors.foreground)
                        .size(12),
                ),
        )
        .child(slider)
}

fn qs_tile(
    theme: &Theme,
    id: &'static str,
    label: &'static str,
    on: bool,
    handler: Option<BoolHandler>,
) -> Div {
    let next = !on;
    let mut btn = Button::new(id)
        .themed(theme)
        .ghost()
        .outline()
        .label(label)
        .selected(on)
        .min_w(px(156))
        .small();
    if let Some(h) = handler {
        btn = btn.on_click(move || h(next));
    }
    btn.into_element()
}

impl IntoElement for OsdFlyout {
    type Element = Div;

    fn into_element(self) -> Div {
        let theme = self.theme;
        let slider_w = px((self.width.0 - 32).max(120));
        let body = match self.kind {
            FlyoutKind::Volume => {
                let vol = if self.muted { 0.0 } else { self.volume };
                let pct = if self.muted {
                    String::from("Muted")
                } else {
                    format!("{:.0}%", self.volume)
                };
                let mut mute = Button::new("osd-mute")
                    .themed(&theme)
                    .ghost()
                    .label(if self.muted { "Unmute" } else { "Mute" })
                    .selected(self.muted)
                    .small();
                if let Some(h) = self.on_mute {
                    mute = mute.on_click(move || h());
                }
                div()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(text("Volume").color(theme.colors.foreground).size(14))
                            .child(text(pct).color(theme.colors.muted_foreground).size(13)),
                    )
                    .child(slider_row(&theme, "Level", vol, self.on_volume, slider_w))
                    .child(mute)
            }
            FlyoutKind::Brightness => div()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(text("Brightness").color(theme.colors.foreground).size(14))
                        .child(
                            text(format!("{:.0}%", self.brightness))
                                .color(theme.colors.muted_foreground)
                                .size(13),
                        ),
                )
                .child(slider_row(
                    &theme,
                    "Level",
                    self.brightness,
                    self.on_brightness,
                    slider_w,
                )),
            FlyoutKind::Calendar => {
                let mut cal = Calendar::new()
                    .themed(&theme)
                    .view(self.cal_view)
                    .selected(self.cal_selected);
                if let Some(h) = self.on_cal_select {
                    cal = cal.on_select(move |d| h(d));
                }
                if let Some(h) = self.on_cal_view {
                    cal = cal.on_view(move |d| h(d));
                }
                div()
                    .flex_col()
                    .gap_2()
                    .child(
                        text(self.time_label)
                            .color(theme.colors.foreground)
                            .size(22),
                    )
                    .child(
                        text(self.date_label)
                            .color(theme.colors.muted_foreground)
                            .size(12),
                    )
                    .child(Separator::horizontal().themed(&theme))
                    .child(cal)
            }
            FlyoutKind::QuickSettings => {
                let tiles = div()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(qs_tile(&theme, "qs-wifi", "Wi-Fi", self.wifi, self.on_wifi))
                            .child(qs_tile(
                                &theme,
                                "qs-bt",
                                "Bluetooth",
                                self.bluetooth,
                                self.on_bluetooth,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(qs_tile(
                                &theme,
                                "qs-night",
                                "Night Light",
                                self.night_light,
                                self.on_night_light,
                            ))
                            .child(qs_tile(
                                &theme,
                                "qs-air",
                                "Airplane",
                                self.airplane,
                                self.on_airplane,
                            )),
                    );
                div()
                    .flex_col()
                    .gap_2()
                    .child(
                        text("Quick Settings")
                            .color(theme.colors.foreground)
                            .size(14),
                    )
                    .child(tiles)
                    .child(slider_row(
                        &theme,
                        "Brightness",
                        self.brightness,
                        self.on_brightness,
                        slider_w,
                    ))
                    .child(slider_row(
                        &theme,
                        "Volume",
                        if self.muted { 0.0 } else { self.volume },
                        self.on_volume,
                        slider_w,
                    ))
                    .child(Separator::horizontal().themed(&theme))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(text("KnoxOS").color(theme.colors.muted_foreground).size(12))
                            .child(
                                text(format!("{}%", self.battery))
                                    .color(theme.colors.primary)
                                    .size(12),
                            ),
                    )
            }
        };

        let mut card = div()
            .w(self.width)
            .h(self.height)
            .flex_col()
            .p(theme.spacing.md)
            .bg(theme.colors.surface.opacity(0.96))
            .border_1()
            .border_color(theme.colors.border)
            .rounded(theme.radius.lg)
            .child(body)
            .focusable();
        if let Some(dismiss) = self.on_dismiss.clone() {
            card = card.on_key_down(move |e| {
                if e.key == Key::Escape {
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
    use super::{FlyoutKind, OsdFlyout};
    use crate::calendar::Date;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        dispatch_mouse_down, dispatch_mouse_up, prepare_element,
    };

    fn click_at(el: &mut knox_gpui::Div, input: &mut WindowInput, p: Point) {
        dispatch_mouse_down(el, input, p);
        dispatch_mouse_up(el, input, p);
    }

    #[test]
    fn constructs_all_flyouts() {
        let theme = Theme::aurora_dark();
        for kind in [
            FlyoutKind::Volume,
            FlyoutKind::Brightness,
            FlyoutKind::Calendar,
            FlyoutKind::QuickSettings,
        ] {
            let _ = OsdFlyout::new(kind).themed(&theme).into_element();
        }
    }

    #[test]
    fn volume_slider_commits() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let (w, h) = FlyoutKind::Volume.size();
        let mut el = OsdFlyout::new(FlyoutKind::Volume)
            .themed(&Theme::aurora_dark())
            .volume(0.0)
            .on_volume(move |v| c.store(v as u32, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .find(|hb| hb.bounds.width() > 140 && hb.bounds.height() <= 24)
            .map(|hb| Point::xy(hb.bounds.x() + hb.bounds.width() * 3 / 4, hb.bounds.y() + 8))
            .expect("volume slider");
        dispatch_mouse_down(&mut el, &mut input, p);
        assert!(n.load(Ordering::SeqCst) >= 40);
    }

    #[test]
    fn brightness_slider_commits() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let (w, h) = FlyoutKind::Brightness.size();
        let mut el = OsdFlyout::new(FlyoutKind::Brightness)
            .themed(&Theme::aurora_dark())
            .brightness(0.0)
            .on_brightness(move |v| c.store(v as u32, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .find(|hb| hb.bounds.width() > 140 && hb.bounds.height() <= 24)
            .map(|hb| Point::xy(hb.bounds.x() + hb.bounds.width() * 3 / 4, hb.bounds.y() + 8))
            .expect("brightness slider");
        dispatch_mouse_down(&mut el, &mut input, p);
        assert!(n.load(Ordering::SeqCst) >= 40);
    }

    #[test]
    fn calendar_selects_day() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let (w, h) = FlyoutKind::Calendar.size();
        let mut el = OsdFlyout::new(FlyoutKind::Calendar)
            .themed(&Theme::aurora_dark())
            .cal_view(Date::new(2026, 10, 1))
            .cal_selected(Date::new(2026, 10, 22))
            .on_cal_select(move |d| c.store(d.day as u32, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .find(|hb| hb.tab_stop && hb.bounds.y() > 80 && hb.bounds.width() <= 48)
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("calendar day");
        click_at(&mut el, &mut input, p);
        assert!(n.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn quick_settings_wifi_toggles() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let (w, h) = FlyoutKind::QuickSettings.size();
        let mut el = OsdFlyout::new(FlyoutKind::QuickSettings)
            .themed(&Theme::aurora_dark())
            .wifi(false)
            .on_wifi(move |on| flag.store(on, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .find(|hb| hb.tab_stop && hb.bounds.width() >= 140 && hb.bounds.width() <= 180)
            .map(|hb| Point::xy(hb.bounds.x() + 12, hb.bounds.y() + 8))
            .expect("wifi tile");
        click_at(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn escape_dismisses() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let (w, h) = FlyoutKind::Volume.size();
        let mut el = OsdFlyout::new(FlyoutKind::Volume)
            .themed(&Theme::aurora_dark())
            .on_dismiss(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Escape, Modifiers::NONE),
        );
        assert!(hit.load(Ordering::SeqCst));
    }
}
