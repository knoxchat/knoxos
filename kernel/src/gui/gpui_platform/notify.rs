// SPDX-License-Identifier: MIT
//! D9.4 session notification center + toasts as Knox GPUI views.
//! Replaces `notifications.rs` paint. Marker `GATE_GPUI notify`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::notifications::{self, Notification, NotificationIcon};
use crate::gui::taskbar::TaskbarPosition;
use knox_gpui::{
    Bounds, IntoElement, Key, KeyDownEvent, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_key_down, dispatch_mouse_down_button, dispatch_mouse_move,
    dispatch_mouse_up, dispatch_scroll as dispatch_scroll_element, paint_element_with_input,
    prepare_element, px,
};
use knox_ui::{NotificationCenter, NotifyAction, NotifyItem, NotifyKind, Theme, ToastStack};

use super::keys;
use super::platform::APP;
use super::proof::app_theme;

pub const GATE_GPUI_NOTIFY: &str = "GATE_GPUI notify";

const GAP: i32 = 8;
const MARGIN: i32 = 16;

static TOAST_WINDOW: AtomicU64 = AtomicU64::new(0);
static CENTER_WINDOW: AtomicU64 = AtomicU64::new(0);
static NOTIFY_GATE: AtomicBool = AtomicBool::new(false);
static SCROLL: AtomicI32 = AtomicI32::new(0);

pub fn is_live() -> bool {
    TOAST_WINDOW.load(Ordering::SeqCst) != 0 && CENTER_WINDOW.load(Ordering::SeqCst) != 0
}

pub fn toast_window_id() -> Option<WindowId> {
    let raw = TOAST_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

pub fn center_window_id() -> Option<WindowId> {
    let raw = CENTER_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

pub fn is_center_open() -> bool {
    notifications::is_panel_open()
}

pub fn has_toasts() -> bool {
    notifications::has_visible_toasts()
}

fn hidden_rect() -> Rect {
    Rect::new(0, 0, 1, 1)
}

fn map_kind(icon: NotificationIcon) -> NotifyKind {
    match icon {
        NotificationIcon::Info => NotifyKind::Info,
        NotificationIcon::Success => NotifyKind::Success,
        NotificationIcon::Warning => NotifyKind::Warning,
        NotificationIcon::Error => NotifyKind::Error,
        NotificationIcon::System => NotifyKind::System,
        NotificationIcon::App => NotifyKind::App,
        NotificationIcon::Network => NotifyKind::Network,
        NotificationIcon::Battery => NotifyKind::Battery,
        NotificationIcon::Volume => NotifyKind::Volume,
        NotificationIcon::Update => NotifyKind::Update,
    }
}

fn age_label(created_tick: u64) -> String {
    let elapsed = crate::interrupts::get_ticks().saturating_sub(created_tick) / 18;
    if elapsed < 60 {
        format!("{}s", elapsed)
    } else if elapsed < 3600 {
        format!("{}m", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{}h", elapsed / 3600)
    } else {
        format!("{}d", elapsed / 86400)
    }
}

fn to_item(n: &Notification) -> NotifyItem {
    let mut item = NotifyItem::new(n.id, n.title.clone(), n.body.clone())
        .app(n.app_name.clone())
        .age(age_label(n.created_tick))
        .kind(map_kind(n.icon))
        .unread(!n.read);
    if n.has_progress {
        item = item.progress(n.progress);
    }
    if !n.actions.is_empty() {
        item = item.actions(
            n.actions
                .iter()
                .map(|a| NotifyAction::new(a.label.clone(), a.action_id.clone(), a.primary)),
        );
    }
    item
}

fn toast_items() -> Vec<NotifyItem> {
    let nc = notifications::NOTIFICATIONS.lock();
    nc.active_toasts().into_iter().map(to_item).collect()
}

fn center_items() -> Vec<NotifyItem> {
    let nc = notifications::NOTIFICATIONS.lock();
    nc.all_for_panel().into_iter().map(to_item).collect()
}

pub fn toast_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    let items = toast_items();
    if items.is_empty() {
        return hidden_rect();
    }
    let (w, h) = ToastStack::stack_size(&items);
    let panel = super::panel::panel_dest_rect(screen_w, screen_h);
    let x = match crate::gui::taskbar::position() {
        TaskbarPosition::Right => (panel.x - w - GAP).max(8),
        _ => (screen_w as i32 - w - MARGIN).max(8),
    };
    let y = match crate::gui::taskbar::position() {
        TaskbarPosition::Top => panel.y + panel.height as i32 + GAP,
        _ => MARGIN,
    };
    let _ = screen_h;
    Rect::new(x, y, w as u32, h as u32)
}

pub fn center_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    if !is_center_open() {
        return hidden_rect();
    }
    let w = NotificationCenter::WIDTH;
    let h = NotificationCenter::HEIGHT;
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
    let _ = screen_h;
    Rect::new(x, y, w as u32, h as u32)
}

fn damage_rect(screen_w: u32, screen_h: u32) -> Rect {
    let toast_max = Rect::new(
        (screen_w as i32 - ToastStack::WIDTH - MARGIN).max(8),
        MARGIN,
        ToastStack::WIDTH as u32,
        (ToastStack::CARD_H * ToastStack::MAX as i32
            + ToastStack::GAP * (ToastStack::MAX as i32 - 1)
            + 80) as u32,
    );
    let center = {
        let panel = super::panel::panel_dest_rect(screen_w, screen_h);
        let w = NotificationCenter::WIDTH;
        let h = NotificationCenter::HEIGHT;
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
    };
    crate::gui::rect_union(toast_max, center)
}

fn dismiss_id(id: u32) {
    notifications::NOTIFICATIONS.lock().dismiss(id);
    damage();
}

fn dismiss_from_panel(id: u32) {
    notifications::NOTIFICATIONS.lock().dismiss_from_panel(id);
    damage();
}

fn mark_read(id: u32) {
    if let Some(n) = notifications::NOTIFICATIONS
        .lock()
        .notifications
        .iter_mut()
        .find(|n| n.id == id)
    {
        n.read = true;
    }
    damage();
}

fn run_action(id: u32, action_id: &str) {
    crate::serial_println!("[KnoxOS] Notification action: {} (id={})", action_id, id);
    notifications::NOTIFICATIONS.lock().dismiss(id);
    damage();
}

pub(super) fn toast_tree(theme: &Theme) -> knox_gpui::Div {
    let items = toast_items();
    if items.is_empty() {
        return knox_gpui::div();
    }
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        toast_dest_rect(sw as u32, sh as u32)
    };
    ToastStack::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .items(items)
        .on_dismiss(dismiss_id)
        .on_action(run_action)
        .into_element()
}

pub(super) fn center_tree(theme: &Theme) -> knox_gpui::Div {
    if !is_center_open() {
        return knox_gpui::div();
    }
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        center_dest_rect(sw as u32, sh as u32)
    };
    let (unread, dnd, group, collapsed) = {
        let nc = notifications::NOTIFICATIONS.lock();
        (
            nc.unread_count,
            nc.dnd_mode,
            nc.group_by_app,
            nc.collapsed_groups.clone(),
        )
    };
    NotificationCenter::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .items(center_items())
        .unread(unread)
        .dnd(dnd)
        .group_by_app(group)
        .collapsed(collapsed)
        .scroll(SCROLL.load(Ordering::SeqCst))
        .on_dismiss(dismiss_from_panel)
        .on_read(mark_read)
        .on_action(run_action)
        .on_clear(|| {
            notifications::NOTIFICATIONS.lock().clear_all();
            SCROLL.store(0, Ordering::SeqCst);
            damage();
        })
        .on_dnd(|on| {
            notifications::NOTIFICATIONS.lock().dnd_mode = on;
            damage();
        })
        .on_group(|| {
            let mut nc = notifications::NOTIFICATIONS.lock();
            nc.group_by_app = !nc.group_by_app;
            nc.scroll_offset = 0;
            drop(nc);
            SCROLL.store(0, Ordering::SeqCst);
            damage();
        })
        .on_toggle_app(|name| {
            let mut nc = notifications::NOTIFICATIONS.lock();
            if nc.collapsed_groups.iter().any(|g| g == name) {
                nc.collapsed_groups.retain(|g| g != name);
            } else {
                nc.collapsed_groups.push(String::from(name));
            }
            drop(nc);
            damage();
        })
        .on_scroll(|y| {
            SCROLL.store(y, Ordering::SeqCst);
        })
        .on_dismiss_panel(close_center)
        .into_element()
}

pub fn paint_toast_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    let mut scene = Scene::new();
    if !has_toasts() {
        return scene;
    }
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = toast_tree(theme);
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

pub fn paint_center_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    let mut scene = Scene::new();
    if !is_center_open() {
        return scene;
    }
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = center_tree(theme);
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

pub fn ensure_toast_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    let dest = toast_dest_rect(screen_w, screen_h);
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = TOAST_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        TOAST_WINDOW.store(id.as_u64(), Ordering::SeqCst);
        id
    } else {
        let id = WindowId::from_u64(raw);
        if let Some(win) = app.window_mut(id) {
            win.set_bounds(bounds);
        }
        id
    }
}

pub fn ensure_center_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    let dest = center_dest_rect(screen_w, screen_h);
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = CENTER_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        CENTER_WINDOW.store(id.as_u64(), Ordering::SeqCst);
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
    if NOTIFY_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_NOTIFY);
}

pub fn toast_contains(x: i32, y: i32) -> bool {
    if !has_toasts() {
        return false;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    toast_dest_rect(sw as u32, sh as u32).contains(x, y)
}

pub fn center_contains(x: i32, y: i32) -> bool {
    if !is_center_open() {
        return false;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    center_dest_rect(sw as u32, sh as u32).contains(x, y)
}

pub fn contains_screen(x: i32, y: i32) -> bool {
    toast_contains(x, y) || center_contains(x, y)
}

fn local_point(bounds: Bounds, x: i32, y: i32) -> Point {
    Point::xy(x - bounds.x(), y - bounds.y())
}

pub fn close_center() {
    notifications::close_panel();
}

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if center_contains(x, y) {
        return dispatch_down(center_window_id(), true, x, y, button);
    }
    if toast_contains(x, y) {
        return dispatch_down(toast_window_id(), false, x, y, button);
    }
    false
}

fn dispatch_down(id: Option<WindowId>, center: bool, x: i32, y: i32, button: MouseButton) -> bool {
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
    let local = local_point(bounds, x, y);
    let size = Size::wh(bounds.width(), bounds.height());
    win.input_mut()
        .set_modifiers(crate::task::keyboard::modifiers());
    {
        let input = win.input_mut();
        let mut root = if center {
            center_tree(&theme)
        } else {
            toast_tree(&theme)
        };
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    if center {
        keys::claim_key_focus();
    }
    win.request_layout();
    true
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    dispatch_up(center_window_id(), true, x, y) || dispatch_up(toast_window_id(), false, x, y)
}

fn dispatch_up(id: Option<WindowId>, center: bool, x: i32, y: i32) -> bool {
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
    let local = local_point(bounds, x, y);
    let size = Size::wh(bounds.width(), bounds.height());
    {
        let input = win.input_mut();
        let mut root = if center {
            center_tree(&theme)
        } else {
            toast_tree(&theme)
        };
        prepare_element(&mut root, Point::ZERO, size, input);
        let _ = dispatch_mouse_up(&mut root, input, local);
    }
    win.request_layout();
    true
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    move_on(center_window_id(), true, x, y) || move_on(toast_window_id(), false, x, y)
}

fn move_on(id: Option<WindowId>, center: bool, x: i32, y: i32) -> bool {
    let open = if center {
        is_center_open()
    } else {
        has_toasts()
    };
    if !open {
        return false;
    }
    let hit = if center {
        center_contains(x, y)
    } else {
        toast_contains(x, y)
    };
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
    let armed = win.input().is_mouse_down() || win.input().mouse_down_hitbox().is_some();
    if !hit && !armed {
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
            let mut root = if center {
                center_tree(&theme)
            } else {
                toast_tree(&theme)
            };
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

pub fn dispatch_scroll(x: i32, y: i32, delta_y: i32) -> bool {
    if !center_contains(x, y) {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = center_window_id() else {
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
        let mut root = center_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_scroll_element(&mut root, input, Point::xy(0, delta_y));
    }
    win.request_layout();
    true
}

pub fn key_down(key: Key, modifiers: knox_gpui::Modifiers) -> bool {
    if !is_center_open() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = center_window_id() else {
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
        let mut root = center_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_key_down(&mut root, input, &KeyDownEvent::new(key, modifiers))
    };
    if key == Key::Escape {
        drop(slot);
        close_center();
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
    let id = if center_contains(x, y) {
        center_window_id()?
    } else {
        toast_window_id()?
    };
    let win = app.window(id)?;
    let local = local_point(win.bounds(), x, y);
    Some(win.input().cursor_at(local))
}

fn present_id(fb: &mut FrameBuffer, damage: Rect, is_full: bool, id: Option<WindowId>, open: bool) {
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

pub fn present(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    present_id(fb, damage, is_full, toast_window_id(), has_toasts());
    present_id(fb, damage, is_full, center_window_id(), is_center_open());
}

pub fn damage() {
    let (sw, sh) = crate::gui::cached_screen_size();
    crate::gui::request_window_redraw(damage_rect(sw as u32, sh as u32));
}
