// SPDX-License-Identifier: MIT
//! D9.1 session panel as a Knox GPUI view. Replaces `taskbar.rs` paint.
//! D9.5 live tray icons (network / volume / clock) as panel children.
//! Marker `GATE_GPUI panel` / `GATE_GPUI tray`.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::gui::framebuffer::Rect;
use crate::gui::taskbar::{self, TaskbarPosition};
use knox_gpui::{
    Bounds, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas, WindowId,
    WindowInput, dispatch_mouse_down_button, dispatch_mouse_up, paint_element_with_input,
    prepare_element, px,
};
use knox_ui::{DOCK_BAR_H, FlyoutKind, Panel, PanelEntry, Theme, TrayStatus};

use super::platform::APP;
use super::proof::app_theme;

pub const GATE_GPUI_PANEL: &str = "GATE_GPUI panel";
pub const GATE_GPUI_TRAY: &str = "GATE_GPUI tray";

const BAR_H: i32 = DOCK_BAR_H;
const FLOAT: i32 = 10;

static PANEL_WINDOW: AtomicU64 = AtomicU64::new(0);
static PANEL_GATE: AtomicBool = AtomicBool::new(false);

pub fn is_live() -> bool {
    PANEL_WINDOW.load(Ordering::SeqCst) != 0
}

pub fn panel_window_id() -> Option<WindowId> {
    let raw = PANEL_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

pub fn panel_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    let w = (screen_w as i32 - 120).clamp(520, 840);
    let h = BAR_H;
    match taskbar::position() {
        TaskbarPosition::Top => Rect::new((screen_w as i32 - w) / 2, FLOAT, w as u32, h as u32),
        TaskbarPosition::Left => Rect::new(FLOAT, (screen_h as i32 - w) / 2, h as u32, w as u32),
        TaskbarPosition::Right => Rect::new(
            screen_w as i32 - h - FLOAT,
            (screen_h as i32 - w) / 2,
            h as u32,
            w as u32,
        ),
        TaskbarPosition::Bottom => Rect::new(
            (screen_w as i32 - w) / 2,
            screen_h as i32 - h - FLOAT,
            w as u32,
            h as u32,
        ),
    }
}

fn tray_status() -> TrayStatus {
    let qs_brightness = crate::gui::popups::QUICK_SETTINGS.lock().brightness;
    TrayStatus {
        wifi: crate::gui::system_tray::is_wifi_enabled(),
        wifi_strength: crate::gui::system_tray::get_wifi_strength(),
        volume: crate::gui::system_tray::get_volume(),
        muted: crate::gui::system_tray::is_muted(),
        brightness: qs_brightness,
        battery: crate::gui::system_tray::get_battery(),
        charging: crate::gui::system_tray::is_charging(),
        bluetooth: crate::gui::system_tray::is_bluetooth_enabled(),
        unread: crate::gui::notifications::unread_count(),
    }
}

fn panel_tree(theme: &Theme) -> knox_gpui::Div {
    let (entries, clock) = {
        let tb = taskbar::TASKBAR.lock();
        let entries: Vec<PanelEntry> = tb
            .entries
            .iter()
            .take(12)
            .map(|e| {
                PanelEntry::new(
                    e.title.clone(),
                    crate::gui::icon_theme::for_content(e.content_type),
                )
                .active(e.active)
            })
            .collect();
        (entries, tb.clock_text.clone())
    };
    let start_open = crate::gui::startmenu::is_visible();
    let osd = super::osd::kind();
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        panel_dest_rect(sw as u32, sh as u32)
    };
    Panel::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .entries(entries)
        .clock(clock)
        .start_open(start_open)
        .tray(tray_status())
        .volume_open(osd == Some(FlyoutKind::Volume))
        .brightness_open(osd == Some(FlyoutKind::Brightness))
        .qs_open(osd == Some(FlyoutKind::QuickSettings))
        .calendar_open(osd == Some(FlyoutKind::Calendar))
        .notify_open(crate::gui::notifications::is_panel_open())
        .on_start(|| {
            crate::gui::popups::close_all_popups();
            crate::gui::notifications::close_panel();
            super::tray::close();
            crate::gui::startmenu::toggle();
        })
        .on_app(|i| {
            super::tray::close();
            taskbar::activate_dock_index(i);
        })
        .on_clock(|| {
            super::tray::close();
            super::osd::toggle(FlyoutKind::Calendar);
        })
        .on_network(|| {
            super::tray::close();
            super::osd::toggle(FlyoutKind::QuickSettings);
        })
        .on_volume(|| {
            super::tray::close();
            super::osd::toggle(FlyoutKind::Volume);
        })
        .on_brightness(|| {
            super::tray::close();
            super::osd::toggle(FlyoutKind::Brightness);
        })
        .on_battery(|| {
            super::tray::close();
            super::osd::toggle(FlyoutKind::QuickSettings);
        })
        .on_bluetooth(|| {
            super::tray::close();
            super::osd::toggle(FlyoutKind::QuickSettings);
        })
        .on_notify(|| {
            super::tray::close();
            crate::gui::popups::close_all_popups();
            crate::gui::startmenu::close();
            crate::gui::notifications::toggle_panel();
        })
        .on_tray_right(|icon| {
            super::tray::toggle(icon);
        })
        .into_element()
}

pub fn paint_panel_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    let mut scene = Scene::new();
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = panel_tree(theme);
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

pub fn ensure_panel_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    let dest = panel_dest_rect(screen_w, screen_h);
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = PANEL_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        PANEL_WINDOW.store(id.as_u64(), Ordering::SeqCst);
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
    if PANEL_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_PANEL);
    crate::serial_println!("[gpui] {}", GATE_GPUI_TRAY);
}

pub fn contains_screen(x: i32, y: i32) -> bool {
    let (sw, sh) = crate::gui::cached_screen_size();
    panel_dest_rect(sw as u32, sh as u32).contains(x, y)
}

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if !contains_screen(x, y) {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = panel_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    let local = Point::xy(x - bounds.x(), y - bounds.y());
    let size = Size::wh(bounds.width(), bounds.height());
    {
        let input = win.input_mut();
        let mut root = panel_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    win.request_layout();
    true
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = panel_window_id() else {
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
    let local = Point::xy(x - bounds.x(), y - bounds.y());
    let size = Size::wh(bounds.width(), bounds.height());
    {
        let input = win.input_mut();
        let mut root = panel_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        let _ = dispatch_mouse_up(&mut root, input, local);
    }
    win.request_layout();
    true
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    if !contains_screen(x, y) {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = panel_window_id() else {
        return false;
    };
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let local = Point::xy(x - win.bounds().x(), y - win.bounds().y());
    let old = win.input().hover_mask();
    win.input_mut().set_pointer_position(local);
    let new = win.input().hover_mask();
    if old != new {
        win.request_layout();
        true
    } else {
        false
    }
}

pub fn cursor_style_at(x: i32, y: i32) -> Option<knox_gpui::CursorStyle> {
    if !contains_screen(x, y) {
        return None;
    }
    let mut slot = APP.lock();
    let app = slot.as_mut()?;
    let id = panel_window_id()?;
    let win = app.window(id)?;
    let local = Point::xy(x - win.bounds().x(), y - win.bounds().y());
    Some(win.input().cursor_at(local))
}

pub fn damage() {
    let (sw, sh) = crate::gui::cached_screen_size();
    crate::gui::request_window_redraw(panel_dest_rect(sw as u32, sh as u32));
}
