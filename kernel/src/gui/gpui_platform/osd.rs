// SPDX-License-Identifier: MIT
//! D9.3 session OSD / flyouts as a Knox GPUI view. Replaces `popups.rs` paint.
//! Marker `GATE_GPUI osd`.

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::popups;
use crate::gui::taskbar::TaskbarPosition;
use knox_gpui::{
    Bounds, IntoElement, Key, KeyDownEvent, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_key_down, dispatch_mouse_down_button, dispatch_mouse_move,
    dispatch_mouse_up, paint_element_with_input, prepare_element, px,
};
pub use knox_ui::FlyoutKind;
use knox_ui::{Date, OsdFlyout, Theme};

use super::keys;
use super::platform::APP;
use super::proof::app_theme;

pub const GATE_GPUI_OSD: &str = "GATE_GPUI osd";

const KIND_NONE: usize = 0;
const KIND_VOLUME: usize = 1;
const KIND_BRIGHTNESS: usize = 2;
const KIND_CALENDAR: usize = 3;
const KIND_QS: usize = 4;

const GAP: i32 = 8;
const DAMAGE_W: i32 = 360;
const DAMAGE_H: i32 = 420;

static OSD_WINDOW: AtomicU64 = AtomicU64::new(0);
static OSD_GATE: AtomicBool = AtomicBool::new(false);
static KIND: AtomicUsize = AtomicUsize::new(KIND_NONE);

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
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

pub fn is_live() -> bool {
    OSD_WINDOW.load(Ordering::SeqCst) != 0
}

pub fn is_open() -> bool {
    KIND.load(Ordering::SeqCst) != KIND_NONE
}

pub fn kind() -> Option<FlyoutKind> {
    unpack(KIND.load(Ordering::SeqCst))
}

pub fn osd_window_id() -> Option<WindowId> {
    let raw = OSD_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

fn pack(kind: Option<FlyoutKind>) -> usize {
    match kind {
        None => KIND_NONE,
        Some(FlyoutKind::Volume) => KIND_VOLUME,
        Some(FlyoutKind::Brightness) => KIND_BRIGHTNESS,
        Some(FlyoutKind::Calendar) => KIND_CALENDAR,
        Some(FlyoutKind::QuickSettings) => KIND_QS,
    }
}

fn unpack(raw: usize) -> Option<FlyoutKind> {
    match raw {
        KIND_VOLUME => Some(FlyoutKind::Volume),
        KIND_BRIGHTNESS => Some(FlyoutKind::Brightness),
        KIND_CALENDAR => Some(FlyoutKind::Calendar),
        KIND_QS => Some(FlyoutKind::QuickSettings),
        _ => None,
    }
}

fn hidden_rect() -> Rect {
    Rect::new(0, 0, 1, 1)
}

fn sync_popup_flags(kind: Option<FlyoutKind>) {
    popups::CALENDAR.lock().visible = matches!(kind, Some(FlyoutKind::Calendar));
    popups::VOLUME.lock().visible = matches!(kind, Some(FlyoutKind::Volume));
    popups::QUICK_SETTINGS.lock().visible = matches!(kind, Some(FlyoutKind::QuickSettings));
}

fn refresh_calendar_clock() {
    let mut cal = popups::CALENDAR.lock();
    let ticks = crate::interrupts::get_ticks();
    let total_secs = ticks / 18;
    cal.hour = ((total_secs / 3600) % 24) as u32;
    cal.minute = ((total_secs / 60) % 60) as u32;
    cal.second = (total_secs % 60) as u32;
}

/// Toggle a flyout. Opening one kind replaces another. Atomic-only (safe under APP lock).
pub fn toggle(next: FlyoutKind) {
    let cur = unpack(KIND.load(Ordering::SeqCst));
    let kind = if cur == Some(next) { None } else { Some(next) };
    if kind.is_some() {
        crate::gui::startmenu::close();
        crate::gui::notifications::close_panel();
        if kind == Some(FlyoutKind::Calendar) {
            refresh_calendar_clock();
        }
    }
    KIND.store(pack(kind), Ordering::SeqCst);
    sync_popup_flags(kind);
    damage();
}

pub fn close() {
    if KIND.swap(KIND_NONE, Ordering::SeqCst) == KIND_NONE {
        return;
    }
    sync_popup_flags(None);
    damage();
}

pub fn osd_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    let Some(kind) = kind() else {
        return hidden_rect();
    };
    let (w, h) = kind.size();
    let panel = super::panel::panel_dest_rect(screen_w, screen_h);
    let x = match crate::gui::taskbar::position() {
        TaskbarPosition::Left => panel.x + panel.width as i32 + GAP,
        TaskbarPosition::Right => (panel.x - w - GAP).max(8),
        TaskbarPosition::Top | TaskbarPosition::Bottom => {
            (panel.x + panel.width as i32 - w - GAP).max(8)
        }
    };
    let y = match crate::gui::taskbar::position() {
        TaskbarPosition::Top => panel.y + panel.height as i32 + GAP,
        TaskbarPosition::Bottom => (panel.y - h - GAP).max(8),
        TaskbarPosition::Left | TaskbarPosition::Right => {
            (panel.y + panel.height as i32 - h).max(8)
        }
    };
    Rect::new(x, y, w as u32, h as u32)
}

fn damage_rect(screen_w: u32, screen_h: u32) -> Rect {
    let panel = super::panel::panel_dest_rect(screen_w, screen_h);
    let x = match crate::gui::taskbar::position() {
        TaskbarPosition::Left => panel.x + panel.width as i32 + GAP,
        TaskbarPosition::Right => (panel.x - DAMAGE_W - GAP).max(8),
        TaskbarPosition::Top | TaskbarPosition::Bottom => {
            (panel.x + panel.width as i32 - DAMAGE_W - GAP).max(8)
        }
    };
    let y = match crate::gui::taskbar::position() {
        TaskbarPosition::Top => panel.y + panel.height as i32 + GAP,
        TaskbarPosition::Bottom => (panel.y - DAMAGE_H - GAP).max(8),
        TaskbarPosition::Left | TaskbarPosition::Right => {
            (panel.y + panel.height as i32 - DAMAGE_H).max(8)
        }
    };
    Rect::new(x, y, DAMAGE_W as u32, DAMAGE_H as u32)
}

fn time_label(hour: u32, minute: u32) -> String {
    let h = if hour == 0 {
        12
    } else if hour > 12 {
        hour - 12
    } else {
        hour
    };
    let ampm = if hour < 12 { "AM" } else { "PM" };
    format!("{}:{:02} {}", h, minute, ampm)
}

fn date_label(year: u32, month: u32, day: u32) -> String {
    let month = month.clamp(1, 12);
    let wd = knox_ui::weekday(year as i32, month as u8, day as u8) as usize;
    format!(
        "{}, {} {}, {}",
        WEEKDAYS[wd.min(6)],
        MONTHS[(month as usize) - 1],
        day,
        year
    )
}

fn apply_volume(value: f32) {
    let n = libm::roundf(value).clamp(0.0, 100.0) as u8;
    {
        let mut vol = popups::VOLUME.lock();
        vol.volume = n;
        vol.muted = false;
    }
    crate::gui::system_tray::set_volume(n);
    crate::gui::system_tray::set_volume_muted(false);
    super::panel::damage();
}

fn apply_mute() {
    let muted = {
        let mut vol = popups::VOLUME.lock();
        vol.muted = !vol.muted;
        vol.muted
    };
    crate::gui::system_tray::set_volume_muted(muted);
    super::panel::damage();
}

fn apply_brightness(value: f32) {
    let n = libm::roundf(value).clamp(0.0, 100.0) as u8;
    popups::QUICK_SETTINGS.lock().brightness = n;
    super::panel::damage();
}

pub(super) fn osd_tree(theme: &Theme) -> knox_gpui::Div {
    let Some(kind) = kind() else {
        return knox_gpui::div();
    };
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        osd_dest_rect(sw as u32, sh as u32)
    };
    let (volume, muted) = {
        let vol = popups::VOLUME.lock();
        (vol.volume as f32, vol.muted)
    };
    let (brightness, wifi, bluetooth, night_light, airplane) = {
        let qs = popups::QUICK_SETTINGS.lock();
        (
            qs.brightness as f32,
            qs.wifi_enabled,
            qs.bluetooth_enabled,
            qs.night_light,
            qs.airplane_mode,
        )
    };
    let (time, date, view, selected) = {
        let cal = popups::CALENDAR.lock();
        let month = cal.month.clamp(1, 12) as u8;
        let day = cal.day.clamp(1, 31) as u8;
        (
            time_label(cal.hour, cal.minute),
            date_label(cal.year, cal.month, cal.day),
            Date::new(cal.year as i32, month, 1),
            Date::new(cal.year as i32, month, day),
        )
    };
    let battery = crate::gui::system_tray::get_battery();
    OsdFlyout::new(kind)
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .volume(volume)
        .muted(muted)
        .brightness(brightness)
        .wifi(wifi)
        .bluetooth(bluetooth)
        .night_light(night_light)
        .airplane(airplane)
        .battery(battery)
        .time_label(time)
        .date_label(date)
        .cal_view(view)
        .cal_selected(selected)
        .on_volume(apply_volume)
        .on_mute(apply_mute)
        .on_brightness(apply_brightness)
        .on_wifi(|on| {
            popups::QUICK_SETTINGS.lock().wifi_enabled = on;
            crate::gui::system_tray::set_wifi_enabled(on);
            crate::gui::system_tray::set_wifi_strength(if on { 3 } else { 0 });
            super::panel::damage();
        })
        .on_bluetooth(|on| {
            popups::QUICK_SETTINGS.lock().bluetooth_enabled = on;
            crate::gui::system_tray::set_bluetooth_enabled(on);
            super::panel::damage();
        })
        .on_night_light(|on| {
            popups::QUICK_SETTINGS.lock().night_light = on;
            crate::gui::request_redraw();
        })
        .on_airplane(|on| {
            {
                let mut qs = popups::QUICK_SETTINGS.lock();
                qs.airplane_mode = on;
                if on {
                    qs.wifi_enabled = false;
                }
            }
            if on {
                crate::gui::system_tray::set_wifi_enabled(false);
            }
            super::panel::damage();
        })
        .on_cal_select(|d| {
            let mut cal = popups::CALENDAR.lock();
            cal.year = d.year.max(0) as u32;
            cal.month = d.month as u32;
            cal.day = d.day as u32;
        })
        .on_cal_view(|d| {
            let mut cal = popups::CALENDAR.lock();
            cal.year = d.year.max(0) as u32;
            cal.month = d.month as u32;
        })
        .on_dismiss(close)
        .into_element()
}

pub fn paint_osd_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    let mut scene = Scene::new();
    if kind().is_none() {
        return scene;
    }
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = osd_tree(theme);
    paint_element_with_input(
        &mut root,
        Point::ZERO,
        Size::wh(width, height),
        &mut scene,
        input,
        &mut env,
    );
    scene
}

pub fn ensure_osd_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    let dest = osd_dest_rect(screen_w, screen_h);
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = OSD_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        OSD_WINDOW.store(id.as_u64(), Ordering::SeqCst);
        id
    } else {
        let id = WindowId::from_u64(raw);
        if let Some(win) = app.window_mut(id) {
            win.set_bounds(bounds);
        }
        id
    }
}

pub fn emit_gate() {
    if OSD_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_OSD);
}

pub fn contains_screen(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    osd_dest_rect(sw as u32, sh as u32).contains(x, y)
}

fn local_point(bounds: Bounds, x: i32, y: i32) -> Point {
    Point::xy(x - bounds.x(), y - bounds.y())
}

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if !contains_screen(x, y) {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = osd_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    let local = local_point(bounds, x, y);
    let size = Size::wh(bounds.width(), bounds.height());
    win.input_mut()
        .set_modifiers(crate::task::keyboard::modifiers());
    {
        let input = win.input_mut();
        let mut root = osd_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    keys::claim_key_focus();
    win.request_layout();
    true
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = osd_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    if !win.input().is_mouse_down() && win.input().mouse_down_hitbox().is_none() {
        return false;
    }
    let bounds = win.bounds();
    let local = local_point(bounds, x, y);
    let size = Size::wh(bounds.width(), bounds.height());
    {
        let input = win.input_mut();
        let mut root = osd_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        let _ = dispatch_mouse_up(&mut root, input, local);
    }
    win.request_layout();
    true
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = osd_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let armed = win.input().is_mouse_down() || win.input().mouse_down_hitbox().is_some();
    if !contains_screen(x, y) && !armed {
        return false;
    }
    let bounds = win.bounds();
    let local = local_point(bounds, x, y);
    let old = win.input().hover_mask();
    if armed {
        let size = Size::wh(bounds.width(), bounds.height());
        {
            let input = win.input_mut();
            input.set_pointer_position(local);
            let mut root = osd_tree(&theme);
            prepare_element(&mut root, Point::ZERO, size, input);
            dispatch_mouse_move(&mut root, input, local);
        }
        win.request_layout();
        return true;
    }
    win.input_mut().set_pointer_position(local);
    let new = win.input().hover_mask();
    if old != new {
        win.request_layout();
        true
    } else {
        false
    }
}

pub fn key_down(key: Key, modifiers: knox_gpui::Modifiers) -> bool {
    if !is_open() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = osd_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let size = Size::wh(win.bounds().width(), win.bounds().height());
    win.input_mut().set_modifiers(modifiers);
    let consumed = {
        let input = win.input_mut();
        let mut root = osd_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_key_down(&mut root, input, &KeyDownEvent::new(key, modifiers))
    };
    if key == Key::Escape {
        drop(slot);
        close();
        return true;
    }
    if consumed {
        keys::claim_key_focus();
        win.request_layout();
    }
    drop(slot);
    if consumed {
        damage();
    }
    consumed
}

pub fn cursor_style_at(x: i32, y: i32) -> Option<knox_gpui::CursorStyle> {
    if !contains_screen(x, y) {
        return None;
    }
    let mut slot = APP.lock();
    let app = slot.as_mut()?;
    let id = osd_window_id()?;
    let win = app.window(id)?;
    let local = local_point(win.bounds(), x, y);
    Some(win.input().cursor_at(local))
}

pub fn present(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    if !is_open() {
        return;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return;
    };
    let Some(id) = osd_window_id() else {
        return;
    };
    let Some(win) = app.window(id) else {
        return;
    };
    let b = win.bounds();
    let dest = Rect::new(
        b.x(),
        b.y(),
        b.width().max(0) as u32,
        b.height().max(0) as u32,
    );
    if is_full || damage.intersects(&dest) {
        fb.blit_bgra(
            dest.x,
            dest.y,
            dest.width,
            dest.height,
            win.store().as_bytes(),
        );
    }
}

pub fn damage() {
    let (sw, sh) = crate::gui::cached_screen_size();
    crate::gui::request_window_redraw(damage_rect(sw as u32, sh as u32));
}
