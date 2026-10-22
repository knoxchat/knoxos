// SPDX-License-Identifier: MIT
//! D12.1 / D12.2 Login and lock screens as GPUI Dialog overlays.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::gui::framebuffer::{FrameBuffer, Rect};
use knox_gpui::{
    Bounds, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas, WindowId,
    WindowInput, dispatch_mouse_down_button, dispatch_mouse_up, paint_element_with_input,
    prepare_element, px,
};
use knox_ui::{LockView, LoginView, Theme};

use super::platform::APP;
use super::proof::app_theme;

/// Serial marker grepped by `tests/run_integration.sh`.
pub const GATE_GPUI_LOGIN: &str = "GATE_GPUI login";

static LOGIN_WINDOW: AtomicU64 = AtomicU64::new(0);
static LOCK_WINDOW: AtomicU64 = AtomicU64::new(0);
static LOGIN_GATE: AtomicBool = AtomicBool::new(false);

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

pub fn login_open() -> bool {
    !crate::gui::login::is_logged_in()
}

pub fn lock_open() -> bool {
    crate::gui::login::is_logged_in() && crate::gui::lock_screen::is_locked()
}

pub fn ensure_login_window(app: &mut knox_gpui::App, sw: u32, sh: u32) -> WindowId {
    let dest = if login_open() {
        Rect::new(0, 0, sw, sh)
    } else {
        hid()
    };
    ensure_overlay(app, &LOGIN_WINDOW, dest)
}

pub fn ensure_lock_window(app: &mut knox_gpui::App, sw: u32, sh: u32) -> WindowId {
    let dest = if lock_open() {
        Rect::new(0, 0, sw, sh)
    } else {
        hid()
    };
    ensure_overlay(app, &LOCK_WINDOW, dest)
}

pub fn login_window_id() -> Option<WindowId> {
    raw_id(&LOGIN_WINDOW)
}

pub fn lock_window_id() -> Option<WindowId> {
    raw_id(&LOCK_WINDOW)
}

pub fn is_login_window(id: WindowId) -> bool {
    login_window_id() == Some(id)
}

pub fn is_lock_window(id: WindowId) -> bool {
    lock_window_id() == Some(id)
}

fn login_tree(theme: &Theme, sw: i32, sh: i32) -> knox_gpui::Div {
    let snap = crate::gui::login::snapshot();
    LoginView::new()
        .themed(theme)
        .viewport(px(sw), px(sh))
        .username(snap.username)
        .password_mask(snap.password_mask)
        .focused(snap.focused)
        .error(snap.error)
        .authenticating(snap.authenticating)
        .on_focus_user(|| {
            crate::gui::login::set_focused_field(0);
        })
        .on_focus_pass(|| {
            crate::gui::login::set_focused_field(1);
        })
        .on_submit(|| {
            crate::gui::login::submit();
        })
        .into_element()
}

fn lock_tree(theme: &Theme, sw: i32, sh: i32) -> knox_gpui::Div {
    let snap = crate::gui::lock_screen::snapshot();
    LockView::new()
        .themed(theme)
        .viewport(px(sw), px(sh))
        .username(snap.username)
        .password_mask(snap.password_mask)
        .error(snap.error)
        .on_submit(|| {
            crate::gui::lock_screen::handle_enter();
        })
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

pub fn paint_login_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    w: i32,
    h: i32,
) -> Scene {
    if !login_open() {
        return Scene::new();
    }
    paint_tree(input, atlas, login_tree(theme, w, h), w, h)
}

pub fn paint_lock_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    w: i32,
    h: i32,
) -> Scene {
    if !lock_open() {
        return Scene::new();
    }
    paint_tree(input, atlas, lock_tree(theme, w, h), w, h)
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

pub fn present_login(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    present_overlay(fb, damage, is_full, login_window_id(), login_open());
}

pub fn present_lock(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    present_overlay(fb, damage, is_full, lock_window_id(), lock_open());
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
    if overlay_down(login_open(), login_window_id(), x, y, button, login_tree) {
        return true;
    }
    overlay_down(lock_open(), lock_window_id(), x, y, button, lock_tree)
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    if overlay_up(login_open(), login_window_id(), x, y, login_tree) {
        return true;
    }
    overlay_up(lock_open(), lock_window_id(), x, y, lock_tree)
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    overlay_hover(login_open(), login_window_id(), x, y)
        || overlay_hover(lock_open(), lock_window_id(), x, y)
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

pub fn emit_gate() {
    if LOGIN_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_LOGIN);
}
