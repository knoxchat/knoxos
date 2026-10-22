// SPDX-License-Identifier: MIT
//! D11.6 remaining in-kernel apps as Knox GPUI views.
//! Domain stays in each `gui/{log,calendar,disk,bt,archive,image,software}_*.rs`.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::window::{self, ViewKind, WindowContentType, WindowId as WmId, WindowState};
use knox_gpui::{
    Bounds, ChromeKind, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_mouse_down_button, dispatch_mouse_up,
    dispatch_scroll as dispatch_scroll_element, paint_element_with_input, prepare_element, px,
};
use knox_ui::{Date, Theme, TitleBar, UtilityApp, WindowBorder};
use spin::Mutex;

use super::platform::APP;
use super::proof::app_theme;

static VIEWS: Mutex<BTreeMap<WmId, u64>> = Mutex::new(BTreeMap::new());

struct Snap {
    wm_id: WmId,
    rect: Rect,
    maximized: bool,
    kind: WindowContentType,
    title: String,
    scroll: i32,
}

fn is_utility(kind: WindowContentType) -> bool {
    matches!(
        kind,
        WindowContentType::Gpui(ViewKind::Logs)
            | WindowContentType::Gpui(ViewKind::Calendar)
            | WindowContentType::Gpui(ViewKind::Disks)
            | WindowContentType::Gpui(ViewKind::Bluetooth)
            | WindowContentType::Gpui(ViewKind::Archive)
            | WindowContentType::Gpui(ViewKind::Image)
            | WindowContentType::Gpui(ViewKind::Software)
            | WindowContentType::Gpui(ViewKind::Updater)
    )
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

pub fn is_apps_window(id: WindowId) -> bool {
    wm_id_for(id).is_some()
}

fn visible_snaps() -> Vec<Snap> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| is_utility(w.content_type) && w.is_visible())
        .map(|w| Snap {
            wm_id: w.id,
            rect: w.rect,
            maximized: w.state == WindowState::Maximized,
            kind: w.content_type,
            title: w.title.clone(),
            scroll: w.scroll_y,
        })
        .collect()
}

fn known_ids() -> Vec<WmId> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| is_utility(w.content_type))
        .map(|w| w.id)
        .collect()
}

fn kind_of(wid: WmId) -> Option<WindowContentType> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.content_type)
}

fn title_of(wid: WmId) -> String {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.title.clone())
        .unwrap_or_else(|| String::from("App"))
}

fn scroll_of(wid: WmId) -> i32 {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.scroll_y)
        .unwrap_or(0)
}

pub fn ensure_app_windows(app: &mut knox_gpui::App) -> Vec<WindowId> {
    crate::gui::image_viewer::poll_picker();
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
            win.set_title(&snap.title);
            win.set_chrome(ChromeKind::Csd);
            win.set_min_size(Size::wh(360, 240));
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

fn body_for(
    theme: &Theme,
    wm_id: WmId,
    kind: WindowContentType,
    width: i32,
    height: i32,
    scroll: i32,
) -> knox_gpui::Div {
    let h = px((height - 32).max(1));
    let w = px(width);
    match kind {
        WindowContentType::Gpui(ViewKind::Logs) => UtilityApp::new()
            .themed(theme)
            .size(w, h)
            .actions(["All", "Err", "Warn", "Info"])
            .rows(crate::gui::log_viewer::log_lines())
            .offset(scroll)
            .status("Kernel log")
            .on_action(move |i| {
                let level = [7u8, 3, 4, 6][i.min(3)];
                crate::gui::log_viewer::set_filter(level);
                crate::gui::damage_window(wm_id);
            })
            .on_offset(move |o| set_scroll(wm_id, o))
            .into_element(),
        WindowContentType::Gpui(ViewKind::Calendar) => {
            let (year, month, td, tm, ty) = crate::gui::calendar_app::current_view(wm_id);
            UtilityApp::new()
                .themed(theme)
                .size(w, h)
                .calendar(Date::new(year, month, 1), Date::new(ty as i32, tm, td))
                .status(format!("{year}-{month:02}"))
                .on_view(move |d| {
                    crate::gui::calendar_app::set_view(wm_id, d.year, d.month);
                    crate::gui::damage_window(wm_id);
                })
                .into_element()
        }
        WindowContentType::Gpui(ViewKind::Disks) => {
            let (rows, sel) = crate::gui::disk_utility::disk_rows(wm_id);
            UtilityApp::new()
                .themed(theme)
                .size(w, h)
                .actions(["Refresh"])
                .rows(rows)
                .selected(sel)
                .offset(scroll)
                .status("Filesystems")
                .on_action(move |_| {
                    crate::gui::disk_utility::refresh_disks(wm_id);
                    crate::gui::damage_window(wm_id);
                })
                .on_row(move |i| {
                    crate::gui::disk_utility::select_disk(wm_id, i);
                    crate::gui::damage_window(wm_id);
                })
                .on_offset(move |o| set_scroll(wm_id, o))
                .into_element()
        }
        WindowContentType::Gpui(ViewKind::Bluetooth) => {
            let (enabled, scanning, rows, sel) = crate::gui::bt_manager::bt_snapshot(wm_id);
            let scan = if scanning { "Stop scan" } else { "Scan" };
            let radio = if enabled { "Disable" } else { "Enable" };
            UtilityApp::new()
                .themed(theme)
                .size(w, h)
                .actions([radio, scan])
                .rows(rows)
                .selected(sel)
                .offset(scroll)
                .status(if enabled {
                    String::from("Bluetooth on")
                } else {
                    String::from("Bluetooth off")
                })
                .on_action(move |i| {
                    if i == 0 {
                        crate::gui::bt_manager::toggle_radio();
                    } else {
                        crate::gui::bt_manager::toggle_scan(wm_id);
                    }
                    crate::gui::damage_window(wm_id);
                })
                .on_row(move |i| {
                    crate::gui::bt_manager::select_device(wm_id, i);
                    crate::gui::damage_window(wm_id);
                })
                .on_offset(move |o| set_scroll(wm_id, o))
                .into_element()
        }
        WindowContentType::Gpui(ViewKind::Archive) => {
            let (path, rows, sel, extracted) = crate::gui::archive_manager::archive_snapshot(wm_id);
            let extract = if extracted { "Extracted" } else { "Extract" };
            UtilityApp::new()
                .themed(theme)
                .size(w, h)
                .actions([extract])
                .rows(rows)
                .selected(sel)
                .offset(scroll)
                .status(path)
                .on_action(move |_| {
                    crate::gui::archive_manager::extract_current(wm_id);
                    crate::gui::damage_window(wm_id);
                })
                .on_row(move |i| {
                    crate::gui::archive_manager::select_entry(wm_id, i);
                    crate::gui::damage_window(wm_id);
                })
                .on_offset(move |o| set_scroll(wm_id, o))
                .into_element()
        }
        WindowContentType::Gpui(ViewKind::Image) => {
            let (path, iw, ih, pixels, status) = crate::gui::image_viewer::image_snapshot(wm_id);
            let mut view = UtilityApp::new()
                .themed(theme)
                .size(w, h)
                .actions(["Open", "+", "−", "Fit", "100%"])
                .status(if status.is_empty() { path } else { status })
                .on_action(move |i| {
                    match i {
                        0 => crate::gui::image_viewer::request_open(wm_id),
                        1 => crate::gui::image_viewer::zoom_in_pub(wm_id),
                        2 => crate::gui::image_viewer::zoom_out_pub(wm_id),
                        3 => crate::gui::image_viewer::zoom_fit_pub(wm_id),
                        _ => crate::gui::image_viewer::zoom_reset_pub(wm_id),
                    }
                    crate::gui::damage_window(wm_id);
                });
            if iw > 0 && ih > 0 && !pixels.is_empty() {
                view = view.image(iw, ih, pixels);
            }
            view.into_element()
        }
        WindowContentType::Gpui(ViewKind::Software) => {
            let (tab, rows, status) = crate::gui::software_center::software_snapshot(wm_id);
            UtilityApp::new()
                .themed(theme)
                .size(w, h)
                .tabs(["Featured", "Categories", "Installed", "Updates"])
                .tab(tab)
                .rows(rows)
                .offset(scroll)
                .status(status)
                .on_tab(move |i| {
                    crate::gui::software_center::set_tab(wm_id, i);
                    crate::gui::damage_window(wm_id);
                })
                .on_row(move |i| {
                    crate::gui::software_center::select_app(wm_id, i);
                    crate::gui::damage_window(wm_id);
                })
                .on_offset(move |o| set_scroll(wm_id, o))
                .into_element()
        }
        WindowContentType::Gpui(ViewKind::Updater) => {
            let (rows, progress, status) = crate::gui::software_updater::updater_snapshot(wm_id);
            UtilityApp::new()
                .themed(theme)
                .size(w, h)
                .actions(["Check", "Apply"])
                .rows(rows)
                .offset(scroll)
                .progress(progress as i32)
                .status(status)
                .on_action(move |i| {
                    if i == 0 {
                        crate::gui::software_updater::check_updates(wm_id);
                    } else {
                        crate::gui::software_updater::apply_updates(wm_id);
                    }
                    crate::gui::damage_window(wm_id);
                })
                .on_offset(move |o| set_scroll(wm_id, o))
                .into_element()
        }
        _ => UtilityApp::new().themed(theme).size(w, h).into_element(),
    }
}

fn set_scroll(wid: WmId, offset: i32) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.scroll_y = offset.max(0);
    }
}

fn app_tree(
    theme: &Theme,
    wm_id: WmId,
    width: i32,
    height: i32,
    maximized: bool,
) -> knox_gpui::Div {
    let kind = kind_of(wm_id).unwrap_or(WindowContentType::ShmEmpty);
    let title = title_of(wm_id);
    let scroll = scroll_of(wm_id);
    let titlebar = TitleBar::new(title.as_str())
        .themed(theme)
        .maximized(maximized)
        .on_drag(move |_| start_drag(wm_id))
        .on_min(move || minimize(wm_id))
        .on_max(move || maximize(wm_id))
        .on_close(move || close(wm_id));
    WindowBorder::new(title.as_str())
        .themed(theme)
        .size(px(width), px(height))
        .titlebar(titlebar)
        .body(body_for(theme, wm_id, kind, width, height, scroll))
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

pub fn paint_apps_scene(
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
    let mut root = app_tree(theme, wm_id, width, height, maximized);
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

pub fn present_apps(fb: &mut FrameBuffer, wid: WmId, area: Rect) {
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
        .map(|w| is_utility(w.content_type) && w.is_visible())
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
        let mut root = app_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
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
            let mut root = app_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
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
        let mut root = app_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_scroll_element(&mut root, input, Point::xy(0, delta_y));
    }
    win.request_layout();
    true
}
