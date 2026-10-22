// SPDX-License-Identifier: MIT
//! D11.10 File picker / screenshot / installer / OOBE as Dialog/Sheet.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::gui::file_picker::{self, FilePickerMode};
use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::screenshot::{self, CaptureMode};
use crate::gui::window::{self, ViewKind, WindowContentType, WindowId as WmId, WindowState};
use knox_gpui::{
    Bounds, ChromeKind, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    Styled, WindowId, WindowInput, dispatch_mouse_down_button, dispatch_mouse_up,
    dispatch_scroll as dispatch_scroll_element, div, paint_element_with_input, prepare_element, px,
};
use knox_ui::{FilePickerDialog, ScreenshotDialog, Theme, TitleBar, WindowBorder, WizardFlow};
use spin::Mutex;

use super::platform::APP;
use super::proof::app_theme;

static PICKER_WINDOW: AtomicU64 = AtomicU64::new(0);
static SHOT_WINDOW: AtomicU64 = AtomicU64::new(0);
static INSTALL_WINDOW: AtomicU64 = AtomicU64::new(0);
static VIEWS: Mutex<BTreeMap<WmId, u64>> = Mutex::new(BTreeMap::new());

pub fn is_live() -> bool {
    super::is_ready()
}

fn hid() -> Rect {
    Rect::new(0, 0, 1, 1)
}

fn raw_id(slot: &AtomicU64) -> Option<WindowId> {
    let raw = slot.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

fn ensure_overlay(app: &mut knox_gpui::App, slot: &AtomicU64, dest: Rect) -> WindowId {
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = slot.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        slot.store(id.as_u64(), Ordering::SeqCst);
        id
    } else {
        let id = WindowId::from_u64(raw);
        if let Some(win) = app.window_mut(id) {
            win.set_bounds(bounds);
        }
        id
    }
}

pub fn ensure_picker_window(app: &mut knox_gpui::App, sw: u32, sh: u32) -> WindowId {
    let dest = if file_picker::is_visible() {
        Rect::new(0, 0, sw, sh)
    } else {
        hid()
    };
    ensure_overlay(app, &PICKER_WINDOW, dest)
}

pub fn ensure_shot_window(app: &mut knox_gpui::App, sw: u32, sh: u32) -> WindowId {
    let dest = if screenshot::is_dialog_open() {
        Rect::new(0, 0, sw, sh)
    } else {
        hid()
    };
    ensure_overlay(app, &SHOT_WINDOW, dest)
}

pub fn ensure_install_window(app: &mut knox_gpui::App, sw: u32, sh: u32) -> WindowId {
    let dest = if crate::gui::installer::is_active() {
        Rect::new(0, 0, sw, sh)
    } else {
        hid()
    };
    ensure_overlay(app, &INSTALL_WINDOW, dest)
}

pub fn picker_window_id() -> Option<WindowId> {
    raw_id(&PICKER_WINDOW)
}
pub fn shot_window_id() -> Option<WindowId> {
    raw_id(&SHOT_WINDOW)
}
pub fn install_window_id() -> Option<WindowId> {
    raw_id(&INSTALL_WINDOW)
}

pub fn is_picker_window(id: WindowId) -> bool {
    picker_window_id() == Some(id)
}
pub fn is_shot_window(id: WindowId) -> bool {
    shot_window_id() == Some(id)
}
pub fn is_install_window(id: WindowId) -> bool {
    install_window_id() == Some(id)
}

pub fn wm_id_for(gpui: WindowId) -> Option<WmId> {
    let raw = gpui.as_u64();
    VIEWS
        .lock()
        .iter()
        .find(|(_, id)| **id == raw)
        .map(|(wm, _)| *wm)
}

pub fn is_wizard_window(id: WindowId) -> bool {
    wm_id_for(id).is_some()
}

fn known_wizard() -> Vec<WmId> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Wizard))
        .map(|w| w.id)
        .collect()
}

pub fn ensure_wizard_windows(app: &mut knox_gpui::App) -> Vec<WindowId> {
    let snaps: Vec<(WmId, Rect)> = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .filter(|w| {
                w.content_type == WindowContentType::Gpui(ViewKind::Wizard) && w.is_visible()
            })
            .map(|w| (w.id, w.rect))
            .collect()
    };
    let known = known_wizard();
    let mut ids = Vec::new();
    let mut map = VIEWS.lock();
    for (wm_id, rect) in &snaps {
        let bounds = Bounds::from_xywh(rect.x, rect.y, rect.width as i32, rect.height as i32);
        let gpui = if let Some(&raw) = map.get(wm_id) {
            WindowId::from_u64(raw)
        } else {
            let id = app.open_window(bounds);
            map.insert(*wm_id, id.as_u64());
            id
        };
        if let Some(win) = app.window_mut(gpui) {
            win.set_title("Setup Wizard");
            win.set_chrome(ChromeKind::Csd);
            win.set_min_size(Size::wh(420, 320));
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

fn picker_tree(theme: &Theme, sw: i32, sh: i32) -> knox_gpui::Div {
    let snap = file_picker::snapshot();
    let title = match snap.mode {
        FilePickerMode::Open => "Open File",
        FilePickerMode::Save => "Save File",
        FilePickerMode::SelectFolder => "Select Folder",
    };
    let confirm = match snap.mode {
        FilePickerMode::Save => "Save",
        FilePickerMode::SelectFolder => "Select",
        FilePickerMode::Open => "Open",
    };
    let labels: Vec<String> = snap
        .entries
        .iter()
        .map(|(n, dir)| if *dir { format!("{n}/") } else { n.clone() })
        .collect();
    let bookmarks: Vec<String> = file_picker::SIDEBAR
        .iter()
        .map(|(n, _)| String::from(*n))
        .collect();
    let bookmark_sel = file_picker::SIDEBAR
        .iter()
        .position(|(_, p)| *p == snap.path);
    FilePickerDialog::new()
        .themed(theme)
        .viewport(px(sw), px(sh))
        .title(title)
        .path(snap.path)
        .bookmarks(bookmarks)
        .bookmark_sel(bookmark_sel)
        .entries(labels)
        .selected(if snap.selected >= 0 {
            Some(snap.selected as usize)
        } else {
            None
        })
        .confirm_label(confirm)
        .on_back(|| {
            file_picker::go_back();
            crate::gui::request_redraw();
        })
        .on_forward(|| {
            file_picker::go_forward();
            crate::gui::request_redraw();
        })
        .on_up(|| {
            file_picker::go_up();
            crate::gui::request_redraw();
        })
        .on_bookmark(|i| {
            if let Some((_, path)) = file_picker::SIDEBAR.get(i) {
                file_picker::go_to(path);
                crate::gui::request_redraw();
            }
        })
        .on_entry(|i| {
            file_picker::select_index(i as i32);
            crate::gui::request_redraw();
        })
        .on_confirm(|| {
            file_picker::confirm();
            crate::gui::request_redraw();
        })
        .on_dismiss(|| {
            file_picker::close();
            crate::gui::request_redraw();
        })
        .into_element()
}

fn shot_tree(theme: &Theme, sw: i32, sh: i32) -> knox_gpui::Div {
    ScreenshotDialog::new()
        .themed(theme)
        .viewport(px(sw), px(sh))
        .on_full(|| {
            screenshot::close_dialog();
            screenshot::take_screenshot(CaptureMode::FullScreen);
        })
        .on_window(|| {
            screenshot::close_dialog();
            screenshot::take_screenshot(CaptureMode::ActiveWindow);
        })
        .on_region(|| {
            screenshot::close_dialog();
            screenshot::take_screenshot(CaptureMode::Region);
        })
        .on_dismiss(screenshot::close_dialog)
        .into_element()
}

fn install_tree(theme: &Theme, sw: i32, sh: i32) -> knox_gpui::Div {
    let snap = crate::gui::installer::snapshot();
    let mut flow = WizardFlow::new()
        .themed(theme)
        .size(px(sw), px(sh))
        .title("KnoxOS Install")
        .steps(crate::gui::installer::STEP_NAMES)
        .step(snap.step)
        .body(if snap.progress > 0 {
            format!("{} ({}%)", snap.body, snap.progress)
        } else {
            snap.body
        })
        .options(snap.options)
        .selected(snap.selected)
        .next_label(snap.next_label)
        .as_sheet(true)
        .on_next(|| {
            crate::gui::installer::next_step();
            crate::gui::request_redraw();
        })
        .on_option(|i| {
            crate::gui::installer::pick(i);
            crate::gui::request_redraw();
        })
        .on_dismiss(|| {
            crate::gui::installer::close_installer();
            crate::gui::request_redraw();
        });
    if snap.can_back {
        flow = flow.on_back(|| {
            crate::gui::installer::back_step();
            crate::gui::request_redraw();
        });
    }
    flow.into_element()
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

fn close_wiz(wid: WmId) {
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

fn minimize_wiz(wid: WmId) {
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

fn maximize_wiz(wid: WmId) {
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

fn wizard_tree(
    theme: &Theme,
    wm_id: WmId,
    width: i32,
    height: i32,
    maximized: bool,
) -> knox_gpui::Div {
    let snap = crate::gui::setup_wizard::snapshot(wm_id);
    let titlebar = TitleBar::new("Setup Wizard")
        .themed(theme)
        .maximized(maximized)
        .on_drag(move |_| start_drag(wm_id))
        .on_min(move || minimize_wiz(wm_id))
        .on_max(move || maximize_wiz(wm_id))
        .on_close(move || close_wiz(wm_id));
    let body = if let Some(s) = snap {
        let mut flow = WizardFlow::new()
            .themed(theme)
            .size(px(width), px((height - 32).max(1)))
            .title(s.title)
            .steps([
                "Welcome", "Language", "Keyboard", "Timezone", "Network", "User", "Privacy",
                "Theme", "Done",
            ])
            .step(s.step as usize)
            .body(s.body)
            .options(s.options)
            .selected(s.selected)
            .next_label(s.next_label)
            .on_next(move || {
                crate::gui::setup_wizard::go_next(wm_id);
                crate::gui::damage_window(wm_id);
            })
            .on_option(move |i| {
                crate::gui::setup_wizard::pick(wm_id, i);
                crate::gui::damage_window(wm_id);
            });
        if s.can_back {
            flow = flow.on_back(move || {
                crate::gui::setup_wizard::go_back(wm_id);
                crate::gui::damage_window(wm_id);
            });
        }
        flow.into_element()
    } else {
        div()
    };
    WindowBorder::new("Setup Wizard")
        .themed(theme)
        .size(px(width), px(height))
        .titlebar(titlebar)
        .body(body)
        .into_element()
}

fn paint_tree(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    mut root: knox_gpui::Div,
    width: i32,
    height: i32,
) -> Scene {
    let mut scene = Scene::new();
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
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

pub fn paint_picker_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    w: i32,
    h: i32,
) -> Scene {
    if !file_picker::is_visible() {
        return Scene::new();
    }
    paint_tree(input, atlas, picker_tree(theme, w, h), w, h)
}

pub fn paint_shot_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    w: i32,
    h: i32,
) -> Scene {
    if !screenshot::is_dialog_open() {
        return Scene::new();
    }
    paint_tree(input, atlas, shot_tree(theme, w, h), w, h)
}

pub fn paint_install_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    w: i32,
    h: i32,
) -> Scene {
    if !crate::gui::installer::is_active() {
        return Scene::new();
    }
    paint_tree(input, atlas, install_tree(theme, w, h), w, h)
}

pub fn paint_wizard_scene(
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
    paint_tree(
        input,
        atlas,
        wizard_tree(theme, wm_id, width, height, maximized),
        width,
        height,
    )
}

pub fn present_picker(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    present_overlay(
        fb,
        damage,
        is_full,
        picker_window_id(),
        file_picker::is_visible(),
    );
}
pub fn present_shot(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    present_overlay(
        fb,
        damage,
        is_full,
        shot_window_id(),
        screenshot::is_dialog_open(),
    );
}
pub fn present_install(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    present_overlay(
        fb,
        damage,
        is_full,
        install_window_id(),
        crate::gui::installer::is_active(),
    );
}

fn present_overlay(
    fb: &mut FrameBuffer,
    damage: Rect,
    is_full: bool,
    id: Option<WindowId>,
    open: bool,
) {
    if !open {
        return;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return;
    };
    let Some(id) = id else {
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

pub fn present_wizard(fb: &mut FrameBuffer, wid: WmId, area: Rect) {
    let _ = crate::gui::backing_store::STORE_MANAGER
        .lock()
        .blit(wid, fb, area.x, area.y);
}

fn overlay_down(
    open: bool,
    id: Option<WindowId>,
    x: i32,
    y: i32,
    button: MouseButton,
    tree: fn(&Theme, i32, i32) -> knox_gpui::Div,
) -> bool {
    if !open {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = id else {
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
        let mut root = tree(&theme, size.width.0, size.height.0);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    win.request_layout();
    true
}

fn overlay_up(
    open: bool,
    id: Option<WindowId>,
    x: i32,
    y: i32,
    tree: fn(&Theme, i32, i32) -> knox_gpui::Div,
) -> bool {
    if !open {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = id else {
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
        let mut root = tree(&theme, size.width.0, size.height.0);
        prepare_element(&mut root, Point::ZERO, size, input);
        let _ = dispatch_mouse_up(&mut root, input, local);
    }
    win.request_layout();
    true
}

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if overlay_down(
        crate::gui::installer::is_active(),
        install_window_id(),
        x,
        y,
        button,
        install_tree,
    ) {
        return true;
    }
    if overlay_down(
        screenshot::is_dialog_open(),
        shot_window_id(),
        x,
        y,
        button,
        shot_tree,
    ) {
        return true;
    }
    if overlay_down(
        file_picker::is_visible(),
        picker_window_id(),
        x,
        y,
        button,
        picker_tree,
    ) {
        return true;
    }
    wizard_pointer_down(x, y, button)
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    if overlay_up(
        crate::gui::installer::is_active(),
        install_window_id(),
        x,
        y,
        install_tree,
    ) {
        return true;
    }
    if overlay_up(
        screenshot::is_dialog_open(),
        shot_window_id(),
        x,
        y,
        shot_tree,
    ) {
        return true;
    }
    if overlay_up(
        file_picker::is_visible(),
        picker_window_id(),
        x,
        y,
        picker_tree,
    ) {
        return true;
    }
    wizard_pointer_up(x, y)
}

fn wizard_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    let Some(wm_id) = hit_wizard(x, y) else {
        return false;
    };
    let Some(id) = gpui_for(wm_id) else {
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
    let maxed = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wm_id)
            .map(|w| w.state == WindowState::Maximized)
            .unwrap_or(false)
    };
    {
        let input = win.input_mut();
        let mut root = wizard_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    win.request_layout();
    true
}

fn wizard_pointer_up(x: i32, y: i32) -> bool {
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
        let maxed = {
            let wm = window::WINDOW_MANAGER.lock();
            wm.windows
                .iter()
                .find(|w| w.id == wm_id)
                .map(|w| w.state == WindowState::Maximized)
                .unwrap_or(false)
        };
        {
            let input = win.input_mut();
            let mut root = wizard_tree(&theme, wm_id, size.width.0, size.height.0, maxed);
            prepare_element(&mut root, Point::ZERO, size, input);
            let _ = dispatch_mouse_up(&mut root, input, local);
        }
        win.request_layout();
        handled = true;
    }
    handled
}

fn gpui_for(wm: WmId) -> Option<WindowId> {
    let raw = *VIEWS.lock().get(&wm)?;
    Some(WindowId::from_u64(raw))
}

fn hit_wizard(x: i32, y: i32) -> Option<WmId> {
    let wm = window::WINDOW_MANAGER.lock();
    let wid = wm.window_at(x, y)?;
    let ok = wm
        .windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.content_type == WindowContentType::Gpui(ViewKind::Wizard) && w.is_visible())
        .unwrap_or(false);
    drop(wm);
    if ok { Some(wid) } else { None }
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    overlay_hover(file_picker::is_visible(), picker_window_id(), x, y)
        || overlay_hover(screenshot::is_dialog_open(), shot_window_id(), x, y)
        || overlay_hover(
            crate::gui::installer::is_active(),
            install_window_id(),
            x,
            y,
        )
        || wizard_hover(x, y)
}

fn overlay_hover(open: bool, id: Option<WindowId>, x: i32, y: i32) -> bool {
    if !open {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = id else {
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

fn wizard_hover(x: i32, y: i32) -> bool {
    let Some(wm_id) = hit_wizard(x, y) else {
        return false;
    };
    let Some(id) = gpui_for(wm_id) else {
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
    if !file_picker::is_visible() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = picker_window_id() else {
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
        input.set_pointer_position(local);
        let mut root = picker_tree(&theme, size.width.0, size.height.0);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_scroll_element(&mut root, input, Point::xy(0, delta_y));
    }
    win.request_layout();
    true
}
