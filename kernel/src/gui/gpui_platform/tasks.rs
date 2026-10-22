// SPDX-License-Identifier: MIT
//! D11.4 Task Manager as a Knox GPUI view (virtual DataTable).
//! Keeps `gui/task_manager.rs` domain; replaces `draw_content` paint.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::task_manager::{self, TaskManagerTab};
use crate::gui::window::{self, ViewKind, WindowContentType, WindowId as WmId, WindowState};
use knox_gpui::{
    Bounds, ChromeKind, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_mouse_down_button, dispatch_mouse_up,
    dispatch_scroll as dispatch_scroll_element, paint_element_with_input, prepare_element, px,
};
use knox_ui::{TableColumn, TaskManagerView, Theme, TitleBar, WindowBorder};
use spin::Mutex;

use super::platform::APP;
use super::proof::app_theme;

static VIEWS: Mutex<BTreeMap<WmId, u64>> = Mutex::new(BTreeMap::new());

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

pub fn is_tasks_window(id: WindowId) -> bool {
    wm_id_for(id).is_some()
}

fn visible_snaps() -> Vec<Snap> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Tasks) && w.is_visible())
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
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Tasks))
        .map(|w| w.id)
        .collect()
}

pub fn ensure_tasks_windows(app: &mut knox_gpui::App) -> Vec<WindowId> {
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
            win.set_title("Task Manager");
            win.set_chrome(ChromeKind::Csd);
            win.set_min_size(Size::wh(480, 320));
            win.set_bounds(bounds);
            win.request_layout();
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

fn proc_columns() -> [TableColumn; 5] {
    [
        TableColumn::new("PID", 60),
        TableColumn::new("Name", 220),
        TableColumn::new("State", 90),
        TableColumn::new("Priority", 80),
        TableColumn::new("UID", 60),
    ]
}

fn win_columns() -> [TableColumn; 4] {
    [
        TableColumn::new("ID", 60),
        TableColumn::new("Title", 240),
        TableColumn::new("Type", 120),
        TableColumn::new("State", 80),
    ]
}

fn tasks_tree(
    theme: &Theme,
    wm_id: WmId,
    width: i32,
    height: i32,
    maximized: bool,
) -> knox_gpui::Div {
    let (tab, selected, scroll, sort_col, sort_asc) = task_manager::view_state(wm_id);
    let tab_i = task_manager::tab_index(tab);
    let sel = if selected >= 0 {
        Some(selected as usize)
    } else {
        None
    };
    let (columns, rows, status, mem_pct, perf) = match tab {
        TaskManagerTab::Processes => {
            let rows = task_manager::process_rows(wm_id);
            let n = rows.len();
            (
                proc_columns().to_vec(),
                rows,
                format!("{n} processes"),
                0,
                Vec::new(),
            )
        }
        TaskManagerTab::Windows => {
            let rows = task_manager::window_rows(wm_id);
            let n = rows.len();
            (
                win_columns().to_vec(),
                rows,
                format!("{n} windows"),
                0,
                Vec::new(),
            )
        }
        TaskManagerTab::Performance => {
            let info = task_manager::performance_info();
            (
                Vec::new(),
                Vec::new(),
                String::from("Performance"),
                info.mem_pct,
                info.rows,
            )
        }
    };
    let titlebar = TitleBar::new("Task Manager")
        .themed(theme)
        .maximized(maximized)
        .on_drag(move |_| start_drag(wm_id))
        .on_min(move || minimize(wm_id))
        .on_max(move || maximize(wm_id))
        .on_close(move || close(wm_id));
    WindowBorder::new("Task Manager")
        .themed(theme)
        .size(px(width), px(height))
        .titlebar(titlebar)
        .body(
            TaskManagerView::new()
                .themed(theme)
                .size(px(width), px((height - 32).max(1)))
                .tab(tab_i)
                .columns(columns)
                .rows(rows)
                .selected(sel)
                .offset(scroll)
                .sort(sort_col, sort_asc)
                .status(status)
                .mem_pct(mem_pct)
                .perf_rows(perf)
                .on_tab(move |i| {
                    task_manager::set_tab(wm_id, task_manager::tab_from_index(i));
                    crate::gui::damage_window(wm_id);
                })
                .on_row(move |i| {
                    task_manager::set_selected(wm_id, i as i32);
                    crate::gui::damage_window(wm_id);
                })
                .on_sort(move |i| {
                    task_manager::set_sort(wm_id, i);
                    crate::gui::damage_window(wm_id);
                })
                .on_offset(move |o| {
                    task_manager::set_scroll(wm_id, o);
                    crate::gui::damage_window(wm_id);
                })
                .on_end(move || {
                    task_manager::end_selected(wm_id);
                    crate::gui::damage_window(wm_id);
                }),
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

pub fn paint_tasks_scene(
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
    let mut root = tasks_tree(theme, wm_id, width, height, maximized);
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

pub fn present_tasks(fb: &mut FrameBuffer, wid: WmId, area: Rect) {
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
        .map(|w| w.content_type == WindowContentType::Gpui(ViewKind::Tasks) && w.is_visible())
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
        let mut root = tasks_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
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
            let mut root = tasks_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
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

pub fn dispatch_scroll(x: i32, y: i32, delta_y: i32) -> bool {
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
        input.set_pointer_position(local);
        let mut root = tasks_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_scroll_element(&mut root, input, Point::xy(0, delta_y));
    }
    win.request_layout();
    true
}
