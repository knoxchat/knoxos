// SPDX-License-Identifier: MIT
//! D11.9 Terminal CSD + tab chrome. PTY still rasterizes via `crate::terminal`.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::window::{self, WindowContentType, WindowId as WmId, WindowState};
use knox_gpui::{
    Bounds, ChromeKind, Hsla, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    Styled, WindowId, WindowInput, dispatch_mouse_down_button, dispatch_mouse_up, div,
    paint_element_with_input, prepare_element, px,
};
use knox_ui::{TERM_TAB_H, TerminalChrome, Theme, TitleBar, WindowBorder};
use spin::Mutex;

use super::platform::APP;
use super::proof::app_theme;

static VIEWS: Mutex<BTreeMap<WmId, u64>> = Mutex::new(BTreeMap::new());

/// CSD titlebar (~32) + tab strip.
pub const CHROME_H: i32 = 32 + TERM_TAB_H;

struct Snap {
    wm_id: WmId,
    rect: Rect,
    maximized: bool,
}

pub fn is_live() -> bool {
    super::is_ready()
}

pub fn wm_id_for(gpui: WindowId) -> Option<WmId> {
    let raw = gpui.as_u64();
    VIEWS
        .lock()
        .iter()
        .find(|(_, id)| **id == raw)
        .map(|(wm, _)| *wm)
}

pub fn gpui_id_for(wm: WmId) -> Option<WindowId> {
    let raw = *VIEWS.lock().get(&wm)?;
    Some(WindowId::from_u64(raw))
}

pub fn is_term_window(id: WindowId) -> bool {
    wm_id_for(id).is_some()
}

fn visible_snaps() -> Vec<Snap> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::TerminalSurface && w.is_visible())
        .map(|w| Snap {
            wm_id: w.id,
            rect: w.rect,
            maximized: w.state == WindowState::Maximized,
        })
        .collect()
}

fn known_ids() -> Vec<WmId> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::TerminalSurface)
        .map(|w| w.id)
        .collect()
}

pub fn ensure_term_windows(app: &mut knox_gpui::App) -> Vec<WindowId> {
    let snaps = visible_snaps();
    let known = known_ids();
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
            win.set_title("Terminal");
            win.set_chrome(ChromeKind::Csd);
            win.set_min_size(Size::wh(400, 240));
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

fn tab_meta(wid: WmId) -> (Vec<String>, usize) {
    let wm = window::WINDOW_MANAGER.lock();
    let Some(win) = wm.windows.iter().find(|w| w.id == wid) else {
        return (alloc::vec![String::from("Shell 1")], 0);
    };
    let labels: Vec<String> = if win.terminal_tabs.is_empty() {
        alloc::vec![String::from("Shell 1")]
    } else {
        win.terminal_tabs
            .iter()
            .enumerate()
            .map(|(i, _)| format!("Shell {}", i + 1))
            .collect()
    };
    (labels, win.terminal_active_tab)
}

fn switch_tab(wid: WmId, i: usize) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if i < win.terminal_tabs.len() || win.terminal_tabs.is_empty() {
            win.terminal_active_tab = i;
        }
    }
}

fn add_tab(wid: WmId) {
    let n = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.terminal_tabs.len())
            .unwrap_or(0)
    };
    let tab_id = crate::terminal::create_tab(wid, n);
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.terminal_tabs.push(tab_id);
        win.terminal_active_tab = n;
    }
}

fn term_tree(
    theme: &Theme,
    wm_id: WmId,
    width: i32,
    height: i32,
    maximized: bool,
) -> knox_gpui::Div {
    let (tabs, active) = tab_meta(wm_id);
    let titlebar = TitleBar::new("Terminal")
        .themed(theme)
        .maximized(maximized)
        .on_drag(move |_| start_drag(wm_id))
        .on_min(move || minimize(wm_id))
        .on_max(move || maximize(wm_id))
        .on_close(move || close(wm_id));
    WindowBorder::new("Terminal")
        .themed(theme)
        .size(px(width), px(height))
        .titlebar(titlebar)
        .body(
            div()
                .flex_col()
                .w(px(width))
                .h(px((height - 32).max(1)))
                .bg(Hsla::from_rgb(12, 12, 12))
                .child(
                    TerminalChrome::new()
                        .themed(theme)
                        .width(px(width))
                        .tabs(tabs)
                        .active(active)
                        .on_tab(move |i| {
                            switch_tab(wm_id, i);
                            crate::gui::damage_window(wm_id);
                        })
                        .on_new(move || {
                            add_tab(wm_id);
                            crate::gui::damage_window(wm_id);
                        }),
                ),
        )
        .into_element()
}

fn start_drag(wid: WmId) {
    let mouse = crate::gui::input::MOUSE.lock();
    let (x, y) = (mouse.x, mouse.y);
    drop(mouse);
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
    let (old, tabs) = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| (w.rect, w.terminal_tabs.clone()))
            .unwrap_or_else(|| (Rect::new(0, 0, 1, 1), alloc::vec::Vec::new()))
    };
    window::WINDOW_MANAGER.lock().close_window(wid);
    crate::gui::taskbar::remove_entry(wid);
    crate::terminal::destroy_for_window(wid);
    for tab_id in tabs {
        if tab_id != wid {
            crate::terminal::destroy_tab(tab_id);
        }
    }
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

pub fn paint_term_scene(
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
    let mut root = term_tree(theme, wm_id, width, height, maximized);
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

pub fn present_term(fb: &mut FrameBuffer, wid: WmId, area: Rect) {
    let _ = crate::gui::backing_store::STORE_MANAGER
        .lock()
        .blit(wid, fb, area.x, area.y);
}

fn hit_visible(x: i32, y: i32) -> Option<(WmId, WindowId)> {
    let wm = window::WINDOW_MANAGER.lock();
    let wid = wm.window_at(x, y)?;
    let ok = wm
        .windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.content_type == WindowContentType::TerminalSurface && w.is_visible())
        .unwrap_or(false);
    drop(wm);
    if !ok {
        return None;
    }
    Some((wid, gpui_id_for(wid)?))
}

fn maximized(wm_id: WmId) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .find(|w| w.id == wm_id)
        .map(|w| w.state == WindowState::Maximized)
        .unwrap_or(false)
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
    let maxed = maximized(wm_id);
    {
        let input = win.input_mut();
        let mut root = term_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
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
        let maxed = maximized(wm_id);
        {
            let input = win.input_mut();
            let mut root = term_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
            prepare_element(&mut root, Point::ZERO, size, input);
            let _ = dispatch_mouse_up(&mut root, input, local);
        }
        win.request_layout();
        handled = true;
    }
    handled
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    let Some((_wm, id)) = hit_visible(x, y) else {
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
    win.input_mut().set_pointer_position(local);
    false
}
