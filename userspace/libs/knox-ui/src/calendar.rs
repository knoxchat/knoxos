// SPDX-License-Identifier: MIT
//! Calendar, DatePicker, TimeField (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{
    Bounds, CursorStyle, Div, Element, HitboxId, IntoElement, Key, KeyDownEvent, LayoutHints,
    MouseEvent, PaintEnv, Point, Quad, Scene, Size, Styled, WindowInput, div, px, ring_color, text,
};

use crate::button::{Button, ButtonVariants};
use crate::size::{Disableable, Selectable, Sizable};
use crate::theme::Theme;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

type DateHandler = Arc<dyn Fn(Date) + Send + Sync>;
type TimeHandler = Arc<dyn Fn(Time) + Send + Sync>;
type ToggleHandler = Arc<dyn Fn() + Send + Sync>;

/// Civil date (proleptic Gregorian).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    pub const fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    pub fn clamp(self) -> Self {
        let month = self.month.clamp(1, 12);
        let dim = days_in_month(self.year, month);
        Self {
            year: self.year,
            month,
            day: self.day.clamp(1, dim),
        }
    }

    pub fn display(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn add_months(self, delta: i32) -> Self {
        let mut y = self.year;
        let mut m = self.month as i32 + delta;
        while m < 1 {
            m += 12;
            y -= 1;
        }
        while m > 12 {
            m -= 12;
            y += 1;
        }
        Date::new(y, m as u8, 1).clamp()
    }
}

impl Default for Date {
    fn default() -> Self {
        Self::new(2026, 10, 22)
    }
}

/// Clock time, 24h.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

impl Time {
    pub const fn new(hour: u8, minute: u8) -> Self {
        Self { hour, minute }
    }

    pub fn clamp(self) -> Self {
        Self {
            hour: self.hour.min(23),
            minute: self.minute.min(59),
        }
    }

    pub fn display(self) -> String {
        format!("{:02}:{:02}", self.hour.min(23), self.minute.min(59))
    }
}

impl Default for Time {
    fn default() -> Self {
        Self::new(9, 30)
    }
}

pub fn is_leap(year: i32) -> bool {
    let y = year;
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Sakamoto: 0 = Sunday.
pub fn weekday(year: i32, month: u8, day: u8) -> u8 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = year;
    if month < 3 {
        y -= 1;
    }
    let w = (y + y / 4 - y / 100 + y / 400 + T[(month as usize) - 1] + day as i32) % 7;
    ((w + 7) % 7) as u8
}

/// Month grid that selects a day.
pub struct Calendar {
    theme: Theme,
    view: Date,
    selected: Date,
    on_select: Option<DateHandler>,
    on_view: Option<DateHandler>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            view: Date::default(),
            selected: Date::default(),
            on_select: None,
            on_view: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn view(mut self, view: Date) -> Self {
        self.view = Date::new(view.year, view.month.clamp(1, 12), 1);
        self
    }

    pub fn selected(mut self, selected: Date) -> Self {
        self.selected = selected.clamp();
        self
    }

    pub fn on_select(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_view(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_view = Some(Arc::new(f));
        self
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Calendar {
    type Element = Div;

    fn into_element(self) -> Div {
        let view = self.view.clamp();
        let title = format!("{} {}", MONTHS[(view.month as usize) - 1], view.year);
        let prev = {
            let on = self.on_view.clone();
            let next_view = view.add_months(-1);
            Button::new("cal-prev")
                .themed(&self.theme)
                .ghost()
                .label("<")
                .on_click(move || {
                    if let Some(h) = &on {
                        h(next_view);
                    }
                })
        };
        let next = {
            let on = self.on_view.clone();
            let next_view = view.add_months(1);
            Button::new("cal-next")
                .themed(&self.theme)
                .ghost()
                .label(">")
                .on_click(move || {
                    if let Some(h) = &on {
                        h(next_view);
                    }
                })
        };
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .w(px(252))
            .child(prev)
            .child(
                text(title)
                    .color(self.theme.colors.foreground)
                    .size(self.theme.typo.body),
            )
            .child(next);

        let mut days = div().flex().gap_1().w(px(252));
        for name in WEEKDAYS {
            days = days.child(
                div()
                    .w(px(32))
                    .h(px(20))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        text(name)
                            .color(self.theme.colors.muted_foreground)
                            .size(12),
                    ),
            );
        }

        let first_wd = weekday(view.year, view.month, 1) as i32;
        let dim = days_in_month(view.year, view.month) as i32;
        let mut grid = div().flex_col().gap_1();
        let mut d = 1i32 - first_wd;
        for _row in 0..6 {
            let mut row = div().flex().gap_1();
            for _col in 0..7 {
                if d < 1 || d > dim {
                    row = row.child(div().w(px(32)).h(px(28)));
                } else {
                    let date = Date::new(view.year, view.month, d as u8);
                    let selected = date == self.selected;
                    let on = self.on_select.clone();
                    row = row.child(
                        Button::new(format!("cal-{d}"))
                            .themed(&self.theme)
                            .ghost()
                            .small()
                            .compact()
                            .label(format!("{d}"))
                            .w(px(32))
                            .selected(selected)
                            .on_click(move || {
                                if let Some(h) = &on {
                                    h(date);
                                }
                            }),
                    );
                }
                d += 1;
            }
            grid = grid.child(row);
        }

        div()
            .flex_col()
            .gap_1()
            .child(header)
            .child(days)
            .child(grid)
    }
}

/// Input-like trigger plus a calendar (kit `DatePicker`).
pub struct DatePicker {
    theme: Theme,
    selected: Date,
    view: Date,
    open: bool,
    on_select: Option<DateHandler>,
    on_view: Option<DateHandler>,
    on_toggle: Option<ToggleHandler>,
}

impl DatePicker {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            selected: Date::default(),
            view: Date::default(),
            open: false,
            on_select: None,
            on_view: None,
            on_toggle: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn selected(mut self, selected: Date) -> Self {
        self.selected = selected.clamp();
        self
    }

    pub fn view(mut self, view: Date) -> Self {
        self.view = view;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_select(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_view(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_view = Some(Arc::new(f));
        self
    }

    pub fn on_toggle(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }
}

impl Default for DatePicker {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for DatePicker {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut trigger = Button::new("date-trig")
            .themed(&self.theme)
            .secondary()
            .label(self.selected.display())
            .min_w(px(120));
        if let Some(toggle) = self.on_toggle {
            trigger = trigger.on_click(move || toggle());
        }
        let mut root = div().flex_col().child(trigger);
        if self.open {
            let mut cal = Calendar::new()
                .themed(&self.theme)
                .view(self.view)
                .selected(self.selected);
            if let Some(on) = self.on_select {
                cal = cal.on_select(move |d| on(d));
            }
            if let Some(on) = self.on_view {
                cal = cal.on_view(move |d| on(d));
            }
            root = root.child(cal);
        }
        root
    }
}

/// Hour / minute segments (kit `TimeField`).
pub struct TimeField {
    theme: Theme,
    value: Time,
    /// 0 = hour, 1 = minute.
    segment: u8,
    disabled: bool,
    on_change: Option<TimeHandler>,
    on_segment: Option<Arc<dyn Fn(u8) + Send + Sync>>,
    computed: Bounds,
    hitbox_id: Option<HitboxId>,
}

impl TimeField {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            value: Time::default(),
            segment: 0,
            disabled: false,
            on_change: None,
            on_segment: None,
            computed: Bounds::ZERO,
            hitbox_id: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn value(mut self, value: Time) -> Self {
        self.value = value.clamp();
        self
    }

    pub fn segment(mut self, segment: u8) -> Self {
        self.segment = segment.min(1);
        self
    }

    pub fn on_change(mut self, f: impl Fn(Time) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn on_segment(mut self, f: impl Fn(u8) + Send + Sync + 'static) -> Self {
        self.on_segment = Some(Arc::new(f));
        self
    }

    pub fn value_of(&self) -> Time {
        self.value.clamp()
    }

    fn emit_time(&mut self, time: Time) {
        let time = time.clamp();
        self.value = time;
        if let Some(h) = &self.on_change {
            h(time);
        }
    }

    fn emit_seg(&mut self, seg: u8) {
        self.segment = seg.min(1);
        if let Some(h) = &self.on_segment {
            h(self.segment);
        }
    }
}

impl Default for TimeField {
    fn default() -> Self {
        Self::new()
    }
}

impl Disableable for TimeField {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Element for TimeField {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        self.computed = Bounds::from_xywh(origin.x.0, origin.y.0, 88, 32);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::default()
    }

    fn prepaint(&mut self, _bounds: Bounds, cx: &mut WindowInput) {
        self.hitbox_id = Some(cx.insert_hitbox_ex(
            self.computed,
            CursorStyle::IBeam,
            !self.disabled,
            !self.disabled,
        ));
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        let focused = input.focused() == self.hitbox_id && self.hitbox_id.is_some();
        if focused {
            scene.push_quad(
                Quad::new(bounds.dilate(px(2)), ring_color()).rounded(self.theme.radius.md),
            );
        }
        let mut quad = Quad::new(bounds, self.theme.colors.input).rounded(self.theme.radius.md);
        quad.border_widths = knox_gpui::Edges::all(px(1));
        quad.border_color = self.theme.colors.border;
        scene.push_quad(quad);
        let t = self.value.clamp();
        let hour_bg = if focused && self.segment == 0 {
            self.theme.colors.accent
        } else {
            knox_gpui::Hsla::TRANSPARENT
        };
        let min_bg = if focused && self.segment == 1 {
            self.theme.colors.accent
        } else {
            knox_gpui::Hsla::TRANSPARENT
        };
        scene.push_quad(
            Quad::new(
                Bounds::from_xywh(bounds.x() + 6, bounds.y() + 4, 28, 24),
                hour_bg,
            )
            .rounded(px(4)),
        );
        scene.push_quad(
            Quad::new(
                Bounds::from_xywh(bounds.x() + 48, bounds.y() + 4, 28, 24),
                min_bg,
            )
            .rounded(px(4)),
        );
        let mut hh = text(format!("{:02}", t.hour))
            .color(self.theme.colors.foreground)
            .size(13);
        let hb = hh.request_layout(Point::xy(bounds.x() + 10, bounds.y() + 8), Size::wh(28, 20));
        hh.paint(hb, scene, input, env);
        let mut colon = text(":").color(self.theme.colors.muted_foreground).size(13);
        let cb = colon.request_layout(Point::xy(bounds.x() + 38, bounds.y() + 8), Size::wh(10, 20));
        colon.paint(cb, scene, input, env);
        let mut mm = text(format!("{:02}", t.minute))
            .color(self.theme.colors.foreground)
            .size(13);
        let mb = mm.request_layout(Point::xy(bounds.x() + 52, bounds.y() + 8), Size::wh(28, 20));
        mm.paint(mb, scene, input, env);
    }

    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        if self.disabled || self.hitbox_id != cx.dispatch_target() {
            return;
        }
        let local = event.position.x.0 - self.computed.x();
        self.emit_seg(if local < 44 { 0 } else { 1 });
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        if self.disabled || self.hitbox_id != cx.focused() {
            return false;
        }
        let mut t = self.value.clamp();
        match event.key {
            Key::Left => {
                self.emit_seg(0);
                true
            }
            Key::Right => {
                self.emit_seg(1);
                true
            }
            Key::Up => {
                if self.segment == 0 {
                    t.hour = (t.hour + 1) % 24;
                } else {
                    t.minute = (t.minute + 1) % 60;
                }
                self.emit_time(t);
                true
            }
            Key::Down => {
                if self.segment == 0 {
                    t.hour = (t.hour + 23) % 24;
                } else {
                    t.minute = (t.minute + 59) % 60;
                }
                self.emit_time(t);
                true
            }
            Key::Char(c) if c.is_ascii_digit() => {
                let d = c.to_digit(10).unwrap() as u8;
                if self.segment == 0 {
                    t.hour = ((t.hour % 10) * 10 + d).min(23);
                } else {
                    t.minute = ((t.minute % 10) * 10 + d).min(59);
                }
                self.emit_time(t);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Calendar, Date, DatePicker, Time, TimeField, days_in_month, weekday};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU32, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        dispatch_mouse_down, dispatch_mouse_up, prepare_element,
    };

    #[test]
    fn leap_and_weekday() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        // 2026-10-22 is Thursday.
        assert_eq!(weekday(2026, 10, 22), 4);
    }

    #[test]
    fn calendar_selects_day() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let mut el = Calendar::new()
            .themed(&Theme::aurora_dark())
            .view(Date::new(2026, 10, 1))
            .selected(Date::new(2026, 10, 1))
            .on_select(move |d| {
                c.store(d.day as u32, Ordering::SeqCst);
            })
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(280, 260), &mut input);
        let day = input
            .hitboxes()
            .iter()
            .find(|h| h.bounds.width() >= 28 && h.bounds.y() > 40)
            .copied()
            .expect("day cell");
        let p = Point::xy(day.bounds.x() + 8, day.bounds.y() + 8);
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(n.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn time_field_edits_segments() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let mut el = TimeField::new()
            .value(Time::new(9, 30))
            .on_change(move |t| {
                c.store(t.hour as u32 * 100 + t.minute as u32, Ordering::SeqCst);
            });
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(88, 32), &mut input);
        input.focus(input.hitboxes()[0].id);
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Up, Modifiers::NONE),
        );
        assert_eq!(el.value_of().hour, 10);
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Right, Modifiers::NONE),
        );
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Up, Modifiers::NONE),
        );
        assert_eq!(el.value_of().minute, 31);
        let _ = DatePicker::new().themed(&Theme::aurora_dark());
    }
}
