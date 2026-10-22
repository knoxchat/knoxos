// SPDX-License-Identifier: MIT
//! D9.5 tray context menu as a Knox GPUI overlay. Replaces
//! `system_tray.rs` fill_rect menus. Icons themselves live in the panel.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::taskbar::TaskbarPosition;
use knox_gpui::{
    Bounds, IntoElement, Key, KeyDownEvent, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_key_down, dispatch_mouse_down_button, dispatch_mouse_move,
    dispatch_mouse_up, paint_element_with_input, prepare_element, px,
};
use knox_ui::{Theme, TrayIcon, TrayMenu};

use super::keys;
use super::platform::APP;
use super::proof::app_theme;

const KIND_NONE: usize = 0;
const GAP: i32 = 8;

static MENU_WINDOW: AtomicU64 = AtomicU64::new(0);
static KIND: AtomicUsize = AtomicUsize::new(KIND_NONE);

pub fn is_open() -> bool {
    KIND.load(Ordering::SeqCst) != KIND_NONE
}

pub fn icon() -> Option<TrayIcon> {
    let raw = KIND.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        TrayIcon::from_u8((raw - 1) as u8)
    }
}

pub fn menu_window_id() -> Option<WindowId> {
    let raw = MENU_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

fn hidden_rect() -> Rect {
    Rect::new(0, 0, 1, 1)
}

pub fn open(icon: TrayIcon) {
    crate::gui::startmenu::close();
    crate::gui::notifications::close_panel();
    super::osd::close();
    KIND.store(icon.as_u8() as usize + 1, Ordering::SeqCst);
    damage();
}

pub fn close() {
    if KIND.swap(KIND_NONE, Ordering::SeqCst) == KIND_NONE {
        return;
    }
    damage();
}

pub fn toggle(icon: TrayIcon) {
    if self_icon() == Some(icon) {
        close();
    } else {
        open(icon);
    }
}

fn self_icon() -> Option<TrayIcon> {
    icon()
}

pub fn menu_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    let Some(icon) = icon() else {
        return hidden_rect();
    };
    let (w, h) = icon.menu_size();
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

fn pick(item: usize) {
    let Some(icon) = icon() else {
        return;
    };
    crate::gui::system_tray::run_menu_action(icon.as_u8(), item);
    close();
    super::panel::damage();
}

pub(super) fn menu_tree(theme: &Theme) -> knox_gpui::Div {
    let Some(icon) = icon() else {
        return knox_gpui::div();
    };
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        menu_dest_rect(sw as u32, sh as u32)
    };
    TrayMenu::new(icon)
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .on_pick(pick)
        .on_dismiss(close)
        .into_element()
}

pub fn paint_menu_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    let mut scene = Scene::new();
    if icon().is_none() {
        return scene;
    }
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = menu_tree(theme);
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

pub fn ensure_menu_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    let dest = menu_dest_rect(screen_w, screen_h);
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = MENU_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        MENU_WINDOW.store(id.as_u64(), Ordering::SeqCst);
        id
    } else {
        let id = WindowId::from_u64(raw);
        if let Some(win) = app.window_mut(id) {
            win.set_bounds(bounds);
        }
        id
    }
}

pub fn contains_screen(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    menu_dest_rect(sw as u32, sh as u32).contains(x, y)
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
    let Some(id) = menu_window_id() else {
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
        let mut root = menu_tree(&theme);
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
    let Some(id) = menu_window_id() else {
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
        let mut root = menu_tree(&theme);
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
    let Some(id) = menu_window_id() else {
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
            let mut root = menu_tree(&theme);
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
    let Some(id) = menu_window_id() else {
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
        let mut root = menu_tree(&theme);
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
    let id = menu_window_id()?;
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
    let Some(id) = menu_window_id() else {
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
    let dest = menu_dest_rect(sw as u32, sh as u32);
    let panel = super::panel::panel_dest_rect(sw as u32, sh as u32);
    crate::gui::request_window_redraw(crate::gui::rect_union(dest, panel));
}
