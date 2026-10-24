// SPDX-License-Identifier: MIT
//! D11.3 Files as a Knox GPUI view (Tree + List + path).
//! Keeps `explorer/` actions; replaces `draw_file_explorer_content` paint.

use crate::gui::explorer;
use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::window::{self, ViewKind, WindowContentType, WindowId as WmId, WindowState};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use knox_gpui::{
    Bounds, ChromeKind, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_mouse_down_button, dispatch_mouse_up, paint_element_with_input,
    prepare_element, px,
};
use knox_ui::{FileEntry, FilesView, Theme, TitleBar, WindowBorder};
use spin::Mutex;

use super::platform::APP;
use super::proof::app_theme;

static VIEWS: Mutex<BTreeMap<WmId, u64>> = Mutex::new(BTreeMap::new());

const BOOKMARKS: &[&str] = &[
    "/home/user",
    "/home/user/Desktop",
    "/home/user/Documents",
    "/home/user/Downloads",
    "/home/user/Pictures",
    "/home/user/Music",
    "/",
];

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

pub fn is_files_window(id: WindowId) -> bool {
    wm_id_for(id).is_some()
}

fn visible_snaps() -> Vec<Snap> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Files) && w.is_visible())
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
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Files))
        .map(|w| w.id)
        .collect()
}

pub fn ensure_files_windows(app: &mut knox_gpui::App) -> Vec<WindowId> {
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
        let created = !map.contains_key(&snap.wm_id);
        let gpui = if let Some(&raw) = map.get(&snap.wm_id) {
            WindowId::from_u64(raw)
        } else {
            let id = app.open_window(bounds);
            map.insert(snap.wm_id, id.as_u64());
            id
        };
        if let Some(win) = app.window_mut(gpui) {
            win.set_title("Files");
            win.set_chrome(ChromeKind::Csd);
            win.set_min_size(Size::wh(480, 320));
            win.set_bounds(bounds);
            if created {
                win.request_layout();
            }
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

fn listing(wid: WmId) -> (String, Vec<FileEntry>, Option<usize>, usize) {
    let wm = window::WINDOW_MANAGER.lock();
    let Some(win) = wm.windows.iter().find(|w| w.id == wid) else {
        return (String::from("/"), Vec::new(), None, 0);
    };
    let path = explorer::current_path(&win.title);
    let selected = if win.explorer_selected >= 0 {
        Some(win.explorer_selected as usize)
    } else {
        None
    };
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);
    let mut entries = explorer::read_entries(&path, show_hidden);
    explorer::sort_entries(&mut entries, sort, sort_asc);
    let bookmark = BOOKMARKS
        .iter()
        .position(|p| *p == path.as_str())
        .unwrap_or(0);
    let rows = entries
        .into_iter()
        .map(|e| FileEntry::new(e.name, e.is_dir))
        .collect();
    (path, rows, selected, bookmark)
}

fn select_entry(wid: WmId, index: usize) {
    let prev = {
        let mut wm = window::WINDOW_MANAGER.lock();
        let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) else {
            return;
        };
        let prev = win.explorer_selected;
        win.explorer_selected = index as i32;
        prev
    };
    if prev == index as i32 {
        explorer::activate_selected(wid);
    }
    crate::gui::damage_window(wid);
}

fn files_tree(
    theme: &Theme,
    wm_id: WmId,
    width: i32,
    height: i32,
    maximized: bool,
) -> knox_gpui::Div {
    let (path, entries, selected, bookmark) = listing(wm_id);
    let titlebar = TitleBar::new("Files")
        .themed(theme)
        .maximized(maximized)
        .on_drag(move |_| start_drag(wm_id))
        .on_min(move || minimize(wm_id))
        .on_max(move || maximize(wm_id))
        .on_close(move || close(wm_id));
    WindowBorder::new("Files")
        .themed(theme)
        .size(px(width), px(height))
        .titlebar(titlebar)
        .body(
            FilesView::new()
                .themed(theme)
                .size(px(width), px((height - 32).max(1)))
                .path(path)
                .bookmarks(BOOKMARKS.iter().copied())
                .bookmark_sel(bookmark)
                .entries(entries)
                .selected(selected)
                .on_back(move || {
                    explorer::navigate_back(wm_id);
                    crate::gui::damage_window(wm_id);
                })
                .on_forward(move || {
                    explorer::navigate_forward(wm_id);
                    crate::gui::damage_window(wm_id);
                })
                .on_up(move || {
                    explorer::navigate_up(wm_id);
                    crate::gui::damage_window(wm_id);
                })
                .on_bookmark(move |i| {
                    if let Some(p) = BOOKMARKS.get(i) {
                        explorer::navigate_to(wm_id, p);
                        crate::gui::damage_window(wm_id);
                    }
                })
                .on_entry(move |i| select_entry(wm_id, i)),
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

pub fn paint_files_scene(
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
    let mut root = files_tree(theme, wm_id, width, height, maximized);
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

pub fn present_files(fb: &mut FrameBuffer, wid: WmId, area: Rect) {
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
        .map(|w| w.content_type == WindowContentType::Gpui(ViewKind::Files) && w.is_visible())
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
        let mut root = files_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
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
            let mut root = files_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
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
    // Hover must not relayout: 900×640 Files raster on TCG starves the tablet.
    false
}
