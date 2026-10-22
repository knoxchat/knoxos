// SPDX-License-Identifier: MIT
//! D11.1 Calculator as a Knox GPUI view. Replaces `calculator::draw_content`.
//! Marker `GATE_GPUI app calc`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::gui::calculator;
use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::window::{self, ViewKind, WindowContentType, WindowId as WmId, WindowState};
use knox_gpui::{
    Bounds, ChromeKind, IntoElement, Key, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_mouse_down_button, dispatch_mouse_up, paint_element_with_input,
    prepare_element, px,
};
use knox_ui::{Calculator, Theme, TitleBar, WindowBorder};
use spin::Mutex;

use super::platform::APP;
use super::proof::app_theme;

pub const GATE_GPUI_APP_CALC: &str = "GATE_GPUI app calc";

static CALC_GATE: AtomicBool = AtomicBool::new(false);
static VIEWS: Mutex<BTreeMap<WmId, u64>> = Mutex::new(BTreeMap::new());

struct CalcSnap {
    wm_id: WmId,
    rect: Rect,
    maximized: bool,
}

pub fn is_live() -> bool {
    super::is_ready()
}

pub fn gpui_id_for(wm: WmId) -> Option<WindowId> {
    let raw = *VIEWS.lock().get(&wm)?;
    Some(WindowId::from_u64(raw))
}

pub fn wm_id_for(gpui: WindowId) -> Option<WmId> {
    let raw = gpui.as_u64();
    VIEWS
        .lock()
        .iter()
        .find(|(_, id)| **id == raw)
        .map(|(wm, _)| *wm)
}

pub fn is_calc_window(id: WindowId) -> bool {
    wm_id_for(id).is_some()
}

fn visible_snaps() -> Vec<CalcSnap> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| {
            w.content_type == WindowContentType::Gpui(ViewKind::Calculator) && w.is_visible()
        })
        .map(|w| CalcSnap {
            wm_id: w.id,
            rect: w.rect,
            maximized: w.state == WindowState::Maximized,
        })
        .collect()
}

fn known_calc_ids() -> Vec<WmId> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Calculator))
        .map(|w| w.id)
        .collect()
}

pub fn ensure_calc_windows(app: &mut knox_gpui::App) -> Vec<WindowId> {
    let snaps = visible_snaps();
    let known = known_calc_ids();
    let mut ids = Vec::new();
    let mut map = VIEWS.lock();
    for snap in &snaps {
        let bounds = Bounds::from_xywh(
            snap.rect.x,
            snap.rect.y,
            snap.rect.width as i32,
            snap.rect.height as i32,
        );
        let gpui = if let Some(&raw) = map.get(&snap.wm_id) {
            WindowId::from_u64(raw)
        } else {
            let id = app.open_window(bounds);
            map.insert(snap.wm_id, id.as_u64());
            id
        };
        if let Some(win) = app.window_mut(gpui) {
            win.set_title("Calculator");
            win.set_chrome(ChromeKind::Csd);
            win.set_min_size(Size::wh(280, 360));
            win.set_bounds(bounds);
        }
        ids.push(gpui);
    }
    let stale: Vec<WmId> = map
        .keys()
        .copied()
        .filter(|id| !known.contains(id))
        .collect();
    for wm_id in stale {
        if let Some(raw) = map.remove(&wm_id) {
            app.close_window(WindowId::from_u64(raw));
            crate::gui::backing_store::STORE_MANAGER
                .lock()
                .remove(wm_id);
        }
    }
    ids
}

fn calc_tree(
    theme: &Theme,
    wm_id: WmId,
    width: i32,
    height: i32,
    maximized: bool,
) -> knox_gpui::Div {
    let (display, history) = calculator::snapshot(wm_id).unwrap_or_else(|| {
        (
            alloc::string::String::from("0"),
            alloc::string::String::new(),
        )
    });
    let titlebar = TitleBar::new("Calculator")
        .themed(theme)
        .maximized(maximized)
        .on_drag(move |_| start_drag(wm_id))
        .on_min(move || minimize(wm_id))
        .on_max(move || maximize(wm_id))
        .on_close(move || close(wm_id));
    WindowBorder::new("Calculator")
        .themed(theme)
        .size(px(width), px(height))
        .titlebar(titlebar)
        .body(
            Calculator::new()
                .themed(theme)
                .size(px(width), px((height - 32).max(1)))
                .display(display)
                .history(history)
                .on_key(move |code| {
                    calculator::apply_key(wm_id, code);
                    crate::gui::sounds::click();
                    crate::gui::damage_window(wm_id);
                }),
        )
        .into_element()
}

pub fn paint_calc_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
    wm_id: WmId,
) -> Scene {
    let maximized = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wm_id)
            .map(|w| w.state == WindowState::Maximized)
            .unwrap_or(false)
    };
    let mut scene = Scene::new();
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = calc_tree(theme, wm_id, width, height, maximized);
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

pub fn present_calc(fb: &mut FrameBuffer, wid: WmId, area: Rect) {
    let _ = crate::gui::backing_store::STORE_MANAGER
        .lock()
        .blit(wid, fb, area.x, area.y);
}

pub fn emit_gate() {
    if CALC_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_APP_CALC);
}

fn mouse_pos() -> (i32, i32) {
    let mouse = crate::gui::input::MOUSE.lock();
    (mouse.x, mouse.y)
}

fn start_drag(wid: WmId) {
    let (x, y) = mouse_pos();
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.state == WindowState::Maximized {
            win.state = WindowState::Normal;
            win.rect = win.saved_rect;
            win.rect.x = x - win.rect.width as i32 / 2;
            win.rect.y = y.max(0);
        }
        win.dragging = true;
        win.drag_offset_x = x - win.rect.x;
        win.drag_offset_y = y - win.rect.y;
    }
}

fn close(wid: WmId) {
    let old = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.rect)
            .unwrap_or_else(|| Rect::new(0, 0, 1, 1))
    };
    window::WINDOW_MANAGER.lock().close_window(wid);
    crate::gui::taskbar::remove_entry(wid);
    crate::gui::request_window_redraw(old);
}

fn minimize(wid: WmId) {
    let old = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.rect)
            .unwrap_or_else(|| Rect::new(0, 0, 1, 1))
    };
    crate::gui::window_events::push_event(wid, crate::gui::window_events::WindowEvent::Minimized);
    window::WINDOW_MANAGER.lock().minimize_window(wid);
    crate::gui::request_window_redraw(old);
}

fn maximize(wid: WmId) {
    let (sw, sh) = crate::gui::cached_screen_size();
    let old = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.rect)
            .unwrap_or_else(|| Rect::new(0, 0, 1, 1))
    };
    crate::gui::request_window_redraw(old);
    window::WINDOW_MANAGER
        .lock()
        .toggle_maximize(wid, sw as u32, sh as u32);
}

fn hit_visible(x: i32, y: i32) -> Option<(WmId, WindowId)> {
    let wm = window::WINDOW_MANAGER.lock();
    let wid = wm.window_at(x, y)?;
    let is_calc = wm
        .windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| {
            w.is_visible()
                && (w.content_type == WindowContentType::Gpui(ViewKind::Calculator)
                    || (w.content_type == WindowContentType::ShmEmpty
                        && calculator::is_attached(wid)))
        })
        .unwrap_or(false);
    drop(wm);
    if !is_calc {
        return None;
    }
    let gpui = gpui_id_for(wid)?;
    Some((wid, gpui))
}

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    let Some((wm_id, id)) = hit_visible(x, y) else {
        return false;
    };
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    let local = Point::xy(x - bounds.x(), y - bounds.y());
    let size = Size::wh(bounds.width(), bounds.height());
    let maximized = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wm_id)
            .map(|w| w.state == WindowState::Maximized)
            .unwrap_or(false)
    };
    {
        let input = win.input_mut();
        let mut root = calc_tree(&theme, wm_id, size.width.0, size.height.0, maximized);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    win.request_layout();
    true
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    let map: Vec<(WmId, WindowId)> = VIEWS
        .lock()
        .iter()
        .map(|(wm, raw)| (*wm, WindowId::from_u64(*raw)))
        .collect();
    if map.is_empty() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let theme = app_theme(app);
    let mut handled = false;
    for (wm_id, id) in map {
        let Some(win) = app.window_mut(id) else {
            continue;
        };
        if !win.input().is_mouse_down() && win.input().mouse_down_hitbox().is_none() {
            continue;
        }
        let bounds = win.bounds();
        let local = Point::xy(x - bounds.x(), y - bounds.y());
        let size = Size::wh(bounds.width(), bounds.height());
        let maximized = {
            let wm = window::WINDOW_MANAGER.lock();
            wm.windows
                .iter()
                .find(|w| w.id == wm_id)
                .map(|w| w.state == WindowState::Maximized)
                .unwrap_or(false)
        };
        {
            let input = win.input_mut();
            let mut root = calc_tree(&theme, wm_id, size.width.0, size.height.0, maximized);
            prepare_element(&mut root, Point::ZERO, size, input);
            let _ = dispatch_mouse_up(&mut root, input, local);
        }
        win.request_layout();
        handled = true;
    }
    handled
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    let Some((wm_id, id)) = hit_visible(x, y) else {
        return false;
    };
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
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
        let _ = wm_id;
        false
    }
}

pub fn key_to_code(key: Key) -> Option<char> {
    match key {
        Key::Char(c) if c.is_ascii_digit() => Some(c),
        Key::Char('.') => Some('.'),
        Key::Char('+') => Some('+'),
        Key::Char('-') => Some('-'),
        Key::Char('*') | Key::Char('x') | Key::Char('X') => Some('*'),
        Key::Char('/') => Some('/'),
        Key::Char('%') => Some('%'),
        Key::Char('c') | Key::Char('C') => Some('C'),
        Key::Enter => Some('='),
        Key::Backspace => Some('B'),
        Key::Escape => Some('C'),
        _ => None,
    }
}

pub fn focused_wm() -> Option<WmId> {
    let wm = window::WINDOW_MANAGER.lock();
    let id = wm.focused_window?;
    let ok = wm
        .windows
        .iter()
        .find(|w| w.id == id)
        .map(|w| {
            if !w.is_visible() {
                return false;
            }
            w.content_type == WindowContentType::Gpui(ViewKind::Calculator)
                || (w.content_type == WindowContentType::ShmEmpty && calculator::is_attached(id))
        })
        .unwrap_or(false);
    if ok { Some(id) } else { None }
}

fn mark_dirty(wid: WmId) {
    let Some(id) = gpui_id_for(wid) else {
        return;
    };
    let mut slot = APP.lock();
    if let Some(app) = slot.as_mut() {
        if let Some(win) = app.window_mut(id) {
            win.request_layout();
        }
    }
}

pub fn key_down(key: Key) -> bool {
    let Some(wid) = focused_wm() else {
        return false;
    };
    let Some(code) = key_to_code(key) else {
        return false;
    };
    calculator::apply_key(wid, code);
    mark_dirty(wid);
    crate::gui::damage_window(wid);
    true
}

pub fn char_input(c: char) -> bool {
    let Some(wid) = focused_wm() else {
        return false;
    };
    let code = if c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | '*' | '/' | '%') {
        c
    } else if c == '=' {
        '='
    } else {
        return false;
    };
    calculator::apply_key(wid, code);
    mark_dirty(wid);
    crate::gui::damage_window(wid);
    true
}
