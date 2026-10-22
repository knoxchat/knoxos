// SPDX-License-Identifier: MIT
//! D16.2 keyboard cheat-sheet as a GPUI Dialog. Lists D6.3 actions.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::gui::framebuffer::{FrameBuffer, Rect};
use knox_gpui::{
    Bounds, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas, WindowId,
    WindowInput, dispatch_mouse_down_button, dispatch_mouse_up, paint_element_with_input,
    prepare_element, px,
};
use knox_ui::{ShortcutsOverlay, Theme};

use super::platform::APP;
use super::proof::app_theme;

static WINDOW: AtomicU64 = AtomicU64::new(0);

pub fn is_open() -> bool {
    crate::gui::shortcuts_overlay::is_visible()
}

fn hid() -> Rect {
    Rect::new(0, 0, 1, 1)
}

pub fn ensure_window(app: &mut knox_gpui::App, sw: u32, sh: u32) -> WindowId {
    let dest = if is_open() {
        Rect::new(0, 0, sw, sh)
    } else {
        hid()
    };
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        WINDOW.store(id.as_u64(), Ordering::SeqCst);
        id
    } else {
        let id = WindowId::from_u64(raw);
        if let Some(win) = app.window_mut(id) {
            win.set_bounds(bounds);
        }
        id
    }
}

pub fn window_id() -> Option<WindowId> {
    let raw = WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

pub fn is_shortcuts_window(id: WindowId) -> bool {
    window_id() == Some(id)
}

fn tree(theme: &Theme, sw: i32, sh: i32) -> knox_gpui::Div {
    ShortcutsOverlay::new()
        .themed(theme)
        .open(is_open())
        .viewport(px(sw), px(sh))
        .on_dismiss(|| {
            crate::gui::shortcuts_overlay::hide();
        })
        .into_element()
}

pub fn paint_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    let mut scene = Scene::new();
    if !is_open() {
        return scene;
    }
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = tree(theme, width, height);
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

pub fn present(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    if !is_open() {
        return;
    }
    let Some(id) = window_id() else {
        return;
    };
    let slot = APP.lock();
    let Some(app) = slot.as_ref() else {
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

pub fn pointer_moved(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = window_id() else {
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

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if !is_open() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = window_id() else {
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

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = window_id() else {
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
        let _ = dispatch_mouse_up(&mut root, input, local);
    }
    win.request_layout();
    true
}
