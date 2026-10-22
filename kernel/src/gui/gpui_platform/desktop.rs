// SPDX-License-Identifier: MIT
//! D9.7 desktop icons + rubber-band + context menu as Knox GPUI views.
//! Replaces `desktop/icons.rs` / `context_menu.rs` paint.
//!
//! D9.9: cursor and icon-drag previews stay compositor-owned. This layer
//! never covers the full framebuffer.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::gui::desktop::{
    CONTEXT_MENU, ContextAction, DESKTOP, IconType, begin_rubber_band, close_context_menu,
    execute_context_action, press_icon,
};
use crate::gui::framebuffer::{FrameBuffer, Rect};
use knox_gpui::{
    Bounds, IntoElement, Key, KeyDownEvent, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_key_down, dispatch_mouse_down_button, dispatch_mouse_move,
    dispatch_mouse_up, paint_element_with_input, prepare_element, px,
};
use knox_ui::{
    DesktopIconEntry, DesktopIcons, DesktopMenu, DesktopRubberBand, ICON_CELL_H, ICON_CELL_W,
    IconName, MenuItem, Theme,
};

use super::keys;
use super::platform::APP;
use super::proof::app_theme;

pub const GATE_GPUI_DESKTOP: &str = "GATE_GPUI desktop";

static ICONS_WINDOW: AtomicU64 = AtomicU64::new(0);
static RUBBER_WINDOW: AtomicU64 = AtomicU64::new(0);
static MENU_WINDOW: AtomicU64 = AtomicU64::new(0);
static DESKTOP_GATE: AtomicBool = AtomicBool::new(false);

pub fn is_live() -> bool {
    ICONS_WINDOW.load(Ordering::SeqCst) != 0
}

pub fn icons_window_id() -> Option<WindowId> {
    id_from(&ICONS_WINDOW)
}

pub fn rubber_window_id() -> Option<WindowId> {
    id_from(&RUBBER_WINDOW)
}

pub fn menu_window_id() -> Option<WindowId> {
    id_from(&MENU_WINDOW)
}

fn id_from(slot: &AtomicU64) -> Option<WindowId> {
    let raw = slot.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

fn hidden_rect() -> Rect {
    Rect::new(0, 0, 1, 1)
}

fn mouse_pos() -> (i32, i32) {
    let mouse = crate::gui::input::MOUSE.lock();
    (mouse.x, mouse.y)
}

fn map_icon(t: IconType) -> IconName {
    crate::gui::icon_theme::for_desktop(t)
}

pub fn icons_dest_rect(_screen_w: u32, _screen_h: u32) -> Rect {
    let desktop = DESKTOP.lock();
    if desktop.icons.is_empty() {
        return Rect::new(24, 16, (ICON_CELL_W + 8) as u32, (ICON_CELL_H + 8) as u32);
    }
    let mut x0 = i32::MAX;
    let mut y0 = i32::MAX;
    let mut x1 = i32::MIN;
    let mut y1 = i32::MIN;
    for icon in &desktop.icons {
        x0 = x0.min(icon.x);
        y0 = y0.min(icon.y);
        x1 = x1.max(icon.x + ICON_CELL_W);
        y1 = y1.max(icon.y + ICON_CELL_H);
    }
    Rect::new(
        x0 - 16,
        y0 - 8,
        (x1 - x0 + 32).max(1) as u32,
        (y1 - y0 + 16).max(1) as u32,
    )
}

pub fn rubber_dest_rect(_screen_w: u32, _screen_h: u32) -> Rect {
    let desktop = DESKTOP.lock();
    match desktop.rubber_band {
        Some(rb) => {
            let r = rb.to_rect();
            Rect::new(r.x, r.y, r.width.max(2), r.height.max(2))
        }
        None => hidden_rect(),
    }
}

pub fn menu_is_open() -> bool {
    CONTEXT_MENU.lock().visible
}

fn menu_items() -> Vec<MenuItem> {
    let menu = CONTEXT_MENU.lock();
    menu.items
        .iter()
        .map(|item| {
            if item.separator {
                MenuItem::separator()
            } else {
                let mut row = MenuItem::new(item.label.clone());
                if let Some(name) = crate::gui::icon_theme::for_action(item.action) {
                    row = row.icon(name);
                }
                row
            }
        })
        .collect()
}

pub fn menu_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    let menu = CONTEXT_MENU.lock();
    if !menu.visible {
        return hidden_rect();
    }
    let items: Vec<MenuItem> = menu
        .items
        .iter()
        .map(|item| {
            if item.separator {
                MenuItem::separator()
            } else {
                let mut row = MenuItem::new(item.label.clone());
                if let Some(name) = crate::gui::icon_theme::for_action(item.action) {
                    row = row.icon(name);
                }
                row
            }
        })
        .collect();
    let (w, h) = DesktopMenu::panel_size(&items);
    let mut x = menu.x;
    let mut y = menu.y;
    if x + w > screen_w as i32 {
        x = (screen_w as i32 - w - 4).max(0);
    }
    let taskbar = crate::gui::scale::taskbar_height() as i32;
    if y + h > screen_h as i32 - taskbar {
        y = (screen_h as i32 - taskbar - h - 4).max(0);
    }
    Rect::new(x, y, w as u32, h as u32)
}

fn pick_menu(index: usize) {
    let action = {
        let menu = CONTEXT_MENU.lock();
        menu.items.get(index).map(|i| i.action)
    };
    close_context_menu();
    if let Some(action) = action {
        if action != ContextAction::Separator && action != ContextAction::None {
            execute_context_action(action);
        }
    }
}

pub(super) fn icons_tree(theme: &Theme) -> knox_gpui::Div {
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        icons_dest_rect(sw as u32, sh as u32)
    };
    let entries = {
        let desktop = DESKTOP.lock();
        let dragging = desktop.dragging_icon;
        desktop
            .icons
            .iter()
            .enumerate()
            .map(|(i, icon)| {
                DesktopIconEntry::new(icon.name.clone(), map_icon(icon.icon_type), icon.x, icon.y)
                    .selected(icon.selected)
                    .ghost(dragging == Some(i))
            })
            .collect::<Vec<_>>()
    };
    DesktopIcons::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .origin(dest.x, dest.y)
        .icons(entries)
        .on_icon(|i| {
            let (x, y) = mouse_pos();
            press_icon(i, x, y);
        })
        .on_empty(|| {
            let (x, y) = mouse_pos();
            begin_rubber_band(x, y);
        })
        .into_element()
}

pub(super) fn rubber_tree(theme: &Theme) -> knox_gpui::Div {
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        rubber_dest_rect(sw as u32, sh as u32)
    };
    if dest.width <= 1 && dest.height <= 1 {
        return knox_gpui::div();
    }
    DesktopRubberBand::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .into_element()
}

pub(super) fn menu_tree(theme: &Theme) -> knox_gpui::Div {
    if !menu_is_open() {
        return knox_gpui::div();
    }
    DesktopMenu::new()
        .themed(theme)
        .items(menu_items())
        .on_pick(pick_menu)
        .on_dismiss(|| {
            close_context_menu();
        })
        .into_element()
}

fn paint_tree(
    mut root: knox_gpui::Div,
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
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

pub fn paint_icons_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    paint_tree(icons_tree(theme), input, atlas, width, height)
}

pub fn paint_rubber_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    if !crate::gui::desktop::is_rubber_band_active() {
        return Scene::new();
    }
    paint_tree(rubber_tree(theme), input, atlas, width, height)
}

pub fn paint_menu_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    if !menu_is_open() {
        return Scene::new();
    }
    paint_tree(menu_tree(theme), input, atlas, width, height)
}

fn ensure_window(slot: &AtomicU64, dest: Rect, app: &mut knox_gpui::App) -> WindowId {
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

pub fn ensure_icons_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    ensure_window(&ICONS_WINDOW, icons_dest_rect(screen_w, screen_h), app)
}

pub fn ensure_rubber_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    ensure_window(&RUBBER_WINDOW, rubber_dest_rect(screen_w, screen_h), app)
}

pub fn ensure_menu_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    ensure_window(&MENU_WINDOW, menu_dest_rect(screen_w, screen_h), app)
}

pub fn emit_gate() {
    if DESKTOP_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_DESKTOP);
}

pub fn contains_icons(x: i32, y: i32) -> bool {
    let (sw, sh) = crate::gui::cached_screen_size();
    icons_dest_rect(sw as u32, sh as u32).contains(x, y)
}

pub fn contains_menu(x: i32, y: i32) -> bool {
    if !menu_is_open() {
        return false;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    menu_dest_rect(sw as u32, sh as u32).contains(x, y)
}

fn local_point(bounds: Bounds, x: i32, y: i32) -> Point {
    Point::xy(x - bounds.x(), y - bounds.y())
}

pub fn dispatch_menu_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if !menu_is_open() {
        return false;
    }
    if !contains_menu(x, y) {
        close_context_menu();
        damage_menu();
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

pub fn dispatch_menu_pointer_up(x: i32, y: i32) -> bool {
    if !menu_is_open() {
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

pub fn dispatch_icons_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if !contains_icons(x, y) {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = icons_window_id() else {
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
        let mut root = icons_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    win.request_layout();
    true
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    if menu_is_open() {
        if contains_menu(x, y) || menu_armed() {
            return hover_window(menu_window_id(), x, y, |theme| menu_tree(theme));
        }
    }
    if !contains_icons(x, y) {
        return false;
    }
    hover_window(icons_window_id(), x, y, |theme| icons_tree(theme))
}

fn menu_armed() -> bool {
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = menu_window_id() else {
        return false;
    };
    let Some(win) = app.window(id) else {
        return false;
    };
    win.input().is_mouse_down() || win.input().mouse_down_hitbox().is_some()
}

fn hover_window(
    id: Option<WindowId>,
    x: i32,
    y: i32,
    tree: impl FnOnce(&Theme) -> knox_gpui::Div,
) -> bool {
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
    let armed = win.input().is_mouse_down() || win.input().mouse_down_hitbox().is_some();
    let old = win.input().hover_mask();
    if armed {
        let size = Size::wh(bounds.width(), bounds.height());
        {
            let input = win.input_mut();
            input.set_pointer_position(local);
            let mut root = tree(&theme);
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
    if !menu_is_open() {
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
        close_context_menu();
        damage_menu();
        return true;
    }
    if consumed {
        keys::claim_key_focus();
        win.request_layout();
    }
    drop(slot);
    if consumed {
        damage_menu();
    }
    consumed
}

pub fn cursor_style_at(x: i32, y: i32) -> Option<knox_gpui::CursorStyle> {
    if contains_menu(x, y) {
        let mut slot = APP.lock();
        let app = slot.as_mut()?;
        let id = menu_window_id()?;
        let win = app.window(id)?;
        let local = local_point(win.bounds(), x, y);
        return Some(win.input().cursor_at(local));
    }
    if !contains_icons(x, y) {
        return None;
    }
    let mut slot = APP.lock();
    let app = slot.as_mut()?;
    let id = icons_window_id()?;
    let win = app.window(id)?;
    let local = local_point(win.bounds(), x, y);
    Some(win.input().cursor_at(local))
}

pub fn present_menu(fb: &mut FrameBuffer, damage: Rect, is_full: bool) {
    if !menu_is_open() {
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
    crate::gui::request_window_redraw(icons_dest_rect(sw as u32, sh as u32));
    if crate::gui::desktop::is_rubber_band_active() {
        crate::gui::request_window_redraw(rubber_dest_rect(sw as u32, sh as u32));
    }
    damage_menu();
}

pub fn damage_menu() {
    if !menu_is_open() {
        return;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    crate::gui::request_window_redraw(menu_dest_rect(sw as u32, sh as u32));
}
