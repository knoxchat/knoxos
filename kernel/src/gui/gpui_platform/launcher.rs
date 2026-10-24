// SPDX-License-Identifier: MIT
//! D9.2 session launcher as a Knox GPUI view. Replaces `startmenu.rs` paint.
//! Marker `GATE_GPUI launcher`.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};

use crate::gui::desktop::IconType;
use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::startmenu::{self, MenuCategory};
use crate::gui::taskbar::TaskbarPosition;
use knox_gpui::{
    Bounds, IntoElement, Key, KeyDownEvent, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_ime_commit, dispatch_key_down, dispatch_mouse_down_button,
    dispatch_mouse_up, dispatch_scroll as dispatch_scroll_element, paint_element_with_input,
    prepare_element, px,
};
use knox_ui::{FieldState, Launcher, LauncherApp, LauncherCategory, PowerAction, Theme};
use spin::Mutex;

use super::keys;
use super::platform::APP;
use super::proof::app_theme;

pub const GATE_GPUI_LAUNCHER: &str = "GATE_GPUI launcher";

const WIDTH: i32 = 480;
const HEIGHT: i32 = 520;
const GAP: i32 = 12;

static LAUNCHER_WINDOW: AtomicU64 = AtomicU64::new(0);
static LAUNCHER_GATE: AtomicBool = AtomicBool::new(false);
static HIGHLIGHT: AtomicUsize = AtomicUsize::new(0);
static SCROLL: AtomicI32 = AtomicI32::new(0);
static QUERY: Mutex<FieldState> = Mutex::new(FieldState::empty());
static PENDING_LAUNCH: Mutex<Option<String>> = Mutex::new(None);
static PENDING_POWER: AtomicUsize = AtomicUsize::new(0);

const POWER_NONE: usize = 0;
const POWER_LOCK: usize = 1;
const POWER_RESTART: usize = 2;
const POWER_SHUTDOWN: usize = 3;

pub fn is_live() -> bool {
    LAUNCHER_WINDOW.load(Ordering::SeqCst) != 0
}

pub fn is_open() -> bool {
    is_live() && startmenu::is_visible()
}

pub fn launcher_window_id() -> Option<WindowId> {
    let raw = LAUNCHER_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

pub fn reset_ui() {
    *QUERY.lock() = FieldState::empty();
    HIGHLIGHT.store(0, Ordering::SeqCst);
    SCROLL.store(0, Ordering::SeqCst);
}

pub fn launcher_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    let panel = super::panel::panel_dest_rect(screen_w, screen_h);
    let w = WIDTH;
    let h = HEIGHT;
    let x = ((screen_w as i32 - w) / 2).max(8);
    let y = match crate::gui::taskbar::position() {
        TaskbarPosition::Top => panel.y + panel.height as i32 + GAP,
        TaskbarPosition::Bottom => (panel.y - h - GAP).max(8),
        TaskbarPosition::Left | TaskbarPosition::Right => ((screen_h as i32 - h) / 2).max(8),
    };
    Rect::new(x, y, w as u32, h as u32)
}

fn hidden_rect() -> Rect {
    Rect::new(0, 0, 1, 1)
}

fn map_category(cat: MenuCategory) -> LauncherCategory {
    match cat {
        MenuCategory::Pinned => LauncherCategory::Pinned,
        MenuCategory::App => LauncherCategory::Apps,
        MenuCategory::Emulator => LauncherCategory::Emulator,
        MenuCategory::Game => LauncherCategory::Game,
        MenuCategory::System => LauncherCategory::System,
        MenuCategory::AI => LauncherCategory::Ai,
        MenuCategory::Media => LauncherCategory::Media,
        MenuCategory::Development => LauncherCategory::Development,
    }
}

fn apps_from_menu() -> Vec<LauncherApp> {
    let menu = startmenu::START_MENU.lock();
    menu.items
        .iter()
        .map(|item| {
            LauncherApp::new(item.name.clone(), map_category(item.category))
                .pinned(item.pinned)
                .keywords(item.keywords.clone())
                .description(item.description.clone())
                .icon(crate::gui::icon_theme::for_desktop(item.icon_type))
        })
        .collect()
}

fn queue_launch(name: &str) {
    *PENDING_LAUNCH.lock() = Some(String::from(name));
    startmenu::close();
}

fn queue_power(action: PowerAction) {
    let code = match action {
        PowerAction::Lock => POWER_LOCK,
        PowerAction::Restart => POWER_RESTART,
        PowerAction::Shutdown => POWER_SHUTDOWN,
    };
    PENDING_POWER.store(code, Ordering::SeqCst);
    startmenu::close();
}

pub fn drain_pending() {
    let name = PENDING_LAUNCH.lock().take();
    let power = PENDING_POWER.swap(POWER_NONE, Ordering::SeqCst);
    if let Some(name) = name {
        let icon = {
            let menu = startmenu::START_MENU.lock();
            menu.items
                .iter()
                .find(|i| i.name == name)
                .map(|i| i.icon_type)
                .unwrap_or(IconType::Document)
        };
        crate::serial_println!("[KnoxOS] Start menu: Opening {}", name);
        crate::gui::desktop::open_application(&name, icon);
    }
    match power {
        POWER_LOCK => {
            crate::serial_println!("[KnoxOS] Power: Lock screen requested via GUI");
            crate::gui::lock_screen::lock();
        }
        POWER_RESTART => {
            crate::serial_println!("[KnoxOS] Power: Reboot requested via GUI");
            crate::acpi::reboot();
        }
        POWER_SHUTDOWN => {
            crate::serial_println!("[KnoxOS] Power: Shutdown requested via GUI");
            crate::acpi::shutdown();
        }
        _ => {}
    }
}

pub(super) fn launcher_tree(theme: &Theme) -> knox_gpui::Div {
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        launcher_dest_rect(sw as u32, sh as u32)
    };
    let apps = apps_from_menu();
    let query = QUERY.lock().clone();
    let names: Vec<String> = apps.iter().map(|a| a.name.clone()).collect();
    Launcher::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .apps(apps)
        .query(query)
        .highlight(HIGHLIGHT.load(Ordering::SeqCst))
        .scroll(SCROLL.load(Ordering::SeqCst))
        .search_placeholder(crate::gui::locale::tr(crate::gui::locale::UiString::Search))
        .power_labels(
            crate::gui::locale::tr(crate::gui::locale::UiString::LockScreen),
            crate::gui::locale::tr(crate::gui::locale::UiString::Restart),
            crate::gui::locale::tr(crate::gui::locale::UiString::Shutdown),
        )
        .on_query(|s| {
            *QUERY.lock() = s;
            HIGHLIGHT.store(0, Ordering::SeqCst);
            SCROLL.store(0, Ordering::SeqCst);
        })
        .on_select(move |i| {
            if let Some(name) = names.get(i) {
                queue_launch(name);
            }
        })
        .on_highlight(|i| {
            HIGHLIGHT.store(i, Ordering::SeqCst);
        })
        .on_power(queue_power)
        .on_dismiss(|| {
            startmenu::close();
        })
        .on_scroll(|y| {
            SCROLL.store(y, Ordering::SeqCst);
        })
        .into_element()
}

pub fn paint_launcher_scene(
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
    let mut root = launcher_tree(theme);
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

pub fn ensure_launcher_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    let dest = if startmenu::is_visible() {
        launcher_dest_rect(screen_w, screen_h)
    } else {
        hidden_rect()
    };
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = LAUNCHER_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        LAUNCHER_WINDOW.store(id.as_u64(), Ordering::SeqCst);
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
    if LAUNCHER_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_LAUNCHER);
}

pub fn contains_screen(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    launcher_dest_rect(sw as u32, sh as u32).contains(x, y)
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
    let Some(id) = launcher_window_id() else {
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
        input.set_context(WindowInput::LAUNCHER_CONTEXT);
        let mut root = launcher_tree(&theme);
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
    let Some(id) = launcher_window_id() else {
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
        let mut root = launcher_tree(&theme);
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
    if !contains_screen(x, y) {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = launcher_window_id() else {
        return false;
    };
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let local = local_point(win.bounds(), x, y);
    let old = win.input().hover_mask();
    win.input_mut().set_pointer_position(local);
    let new = win.input().hover_mask();
    old != new
}

pub fn dispatch_scroll(x: i32, y: i32, delta_y: i32) -> bool {
    if !contains_screen(x, y) {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = launcher_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    let local = local_point(bounds, x, y);
    let size = Size::wh(bounds.width(), bounds.height());
    {
        let input = win.input_mut();
        input.set_pointer_position(local);
        let mut root = launcher_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_scroll_element(&mut root, input, Point::xy(0, delta_y));
    }
    win.request_layout();
    true
}

pub fn key_down(key: Key, modifiers: knox_gpui::Modifiers) -> bool {
    if !is_open() {
        return false;
    }
    if matches!(key, Key::Char(_) | Key::Space)
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
    {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = launcher_window_id() else {
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
        input.set_context(WindowInput::LAUNCHER_CONTEXT);
        let mut root = launcher_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_key_down(&mut root, input, &KeyDownEvent::new(key, modifiers))
    };
    if key == Key::Escape {
        drop(slot);
        startmenu::close();
        damage();
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

pub fn char_input(c: char) -> bool {
    if !is_open() {
        return false;
    }
    if c.is_control() || crate::task::keyboard::is_ctrl_held() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = launcher_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    if !win.input().focused_accepts_ime() {
        // FocusTrap should have focused the search field; type anyway after layout.
    }
    let size = Size::wh(win.bounds().width(), win.bounds().height());
    if let Some(text) = crate::gui::ime::process_char(c) {
        let input = win.input_mut();
        let mut root = launcher_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_ime_commit(&mut root, input, &text);
    }
    win.request_layout();
    drop(slot);
    damage();
    true
}

pub fn cursor_style_at(x: i32, y: i32) -> Option<knox_gpui::CursorStyle> {
    if !contains_screen(x, y) {
        return None;
    }
    let mut slot = APP.lock();
    let app = slot.as_mut()?;
    let id = launcher_window_id()?;
    let win = app.window(id)?;
    let local = local_point(win.bounds(), x, y);
    Some(win.input().cursor_at(local))
}

pub fn present(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    if !startmenu::is_visible() {
        return;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return;
    };
    let Some(id) = launcher_window_id() else {
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
    if !startmenu::is_visible() {
        return;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    crate::gui::request_window_redraw(launcher_dest_rect(sw as u32, sh as u32));
}
