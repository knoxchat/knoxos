// SPDX-License-Identifier: MIT
//! Pointer dispatch into the GPUI proof window (D3.6 / D6.1 / D6.5).
//!
//! VirtIO-tablet absolute coordinates take the same path as PS/2:
//! `set_absolute_mouse` → `handle_click` / `handle_mouse_up` → here.
//! Do not require relative motion.

use knox_gpui::{
    CursorStyle, MouseButton, Point, Size, dispatch_mouse_down_button, dispatch_mouse_up,
    dispatch_scroll, prepare_element,
};

use super::keys;
use super::platform::APP;
use super::proof::{self, app_theme, proof_dest_rect, proof_tree, proof_window_id};

fn screen_to_local(origin_x: i32, origin_y: i32, x: i32, y: i32) -> Point {
    Point::xy(x - origin_x, y - origin_y)
}

/// Damage the centered proof card (with chrome padding).
pub fn damage_proof() {
    let (sw, sh) = crate::gui::cached_screen_size();
    crate::gui::request_window_redraw(proof_dest_rect(sw as u32, sh as u32));
}

/// D13.3: typing in an Input damages the focused field (caret neighborhood),
/// not 1920×1080.
pub fn damage_focused() {
    let mut slot = super::platform::APP.lock();
    let Some(app) = slot.as_mut() else {
        drop(slot);
        damage_proof();
        return;
    };
    let Some(id) = proof_window_id() else {
        return;
    };
    let Some(win) = app.window(id) else {
        return;
    };
    let origin_x = win.bounds().x();
    let origin_y = win.bounds().y();
    if let Some(b) = win.input().focused_bounds() {
        let pad = 8i32;
        drop(slot);
        crate::gui::push_damage(crate::gui::framebuffer::Rect::new(
            origin_x + b.x() - pad,
            origin_y + b.y() - pad,
            (b.width().max(0) as u32)
                .saturating_add((pad * 2) as u32)
                .max(12),
            (b.height().max(0) as u32)
                .saturating_add((pad * 2) as u32)
                .max(12),
        ));
        return;
    }
    drop(slot);
    damage_proof();
}

/// Hover tracking. Returns true when a pseudo-state changed and needs paint.
pub fn pointer_moved(x: i32, y: i32) -> bool {
    if super::session::pointer_moved(x, y) {
        return true;
    }
    if super::shortcuts::pointer_moved(x, y) {
        return true;
    }
    if super::dialogs::pointer_moved(x, y) {
        return true;
    }
    if super::switcher::pointer_moved(x, y) {
        super::switcher::damage();
        return true;
    }
    if super::notify::pointer_moved(x, y) {
        super::notify::damage();
        return true;
    }
    if super::tray::pointer_moved(x, y) {
        super::tray::damage();
        return true;
    }
    if super::osd::pointer_moved(x, y) {
        super::osd::damage();
        return true;
    }
    if super::launcher::pointer_moved(x, y) {
        super::launcher::damage();
        return true;
    }
    if super::desktop::menu_is_open() && super::desktop::pointer_moved(x, y) {
        super::desktop::damage();
        return true;
    }
    if super::panel::pointer_moved(x, y) {
        super::panel::damage();
        return true;
    }
    {
        let wm = crate::gui::window::WINDOW_MANAGER.lock();
        if wm.window_at(x, y).is_some() {
            return false;
        }
    }
    if super::desktop::pointer_moved(x, y) {
        super::desktop::damage();
        return true;
    }

    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = proof_window_id() else {
        return false;
    };
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    if let Some((gx, gy)) = super::proof::drag_grab() {
        super::proof::set_user_origin(x - gx, y - gy);
        win.set_origin(knox_gpui::Point::xy(x - gx, y - gy));
        drop(slot);
        damage_proof();
        return true;
    }
    let local = screen_to_local(win.bounds().x(), win.bounds().y(), x, y);
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

/// Mouse down on the proof window. Consumes the click so desktop icons do not.
pub fn pointer_down(x: i32, y: i32) -> bool {
    pointer_down_button(x, y, MouseButton::Left)
}

pub fn pointer_down_button(x: i32, y: i32, button: MouseButton) -> bool {
    if super::session::dispatch_pointer_down(x, y, button) {
        return true;
    }
    if super::shortcuts::dispatch_pointer_down(x, y, button) {
        return true;
    }
    if super::dialogs::dispatch_pointer_down(x, y, button) {
        return true;
    }
    if super::switcher::dispatch_pointer_down(x, y, button) {
        super::switcher::damage();
        return true;
    }
    if super::notify::dispatch_pointer_down(x, y, button) {
        super::notify::damage();
        return true;
    }
    if super::tray::dispatch_pointer_down(x, y, button) {
        super::tray::damage();
        return true;
    }
    if super::osd::dispatch_pointer_down(x, y, button) {
        super::osd::damage();
        return true;
    }
    if super::desktop::dispatch_menu_pointer_down(x, y, button) {
        super::desktop::damage_menu();
        return true;
    }
    if super::launcher::dispatch_pointer_down(x, y, button) {
        super::launcher::damage();
        return true;
    }
    if super::launcher::is_open() && !super::panel::contains_screen(x, y) {
        crate::gui::startmenu::close();
        super::launcher::damage();
    }
    if super::panel::dispatch_pointer_down(x, y, button) {
        super::panel::damage();
        return true;
    }
    if super::tray::is_open() {
        super::tray::close();
    }
    if super::notify::is_center_open() {
        super::notify::close_center();
        super::notify::damage();
    }
    if super::osd::is_open() {
        super::osd::close();
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = proof_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    if !bounds.contains(Point::xy(x, y)) {
        let had_focus = win.input().focused().is_some();
        if had_focus {
            win.input_mut().blur();
            win.request_layout();
        }
        keys::release_key_focus();
        drop(slot);
        if had_focus {
            damage_proof();
        }
        return false;
    }
    let local = screen_to_local(bounds.x(), bounds.y(), x, y);
    let size = Size::wh(proof::CARD_W, proof::CARD_H);
    win.input_mut()
        .set_modifiers(crate::task::keyboard::modifiers());
    {
        let input = win.input_mut();
        let mut root = proof_tree(&theme, 0.0, false);
        prepare_element(&mut root, Point::xy(proof::PAD, proof::PAD), size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    keys::claim_key_focus();
    win.request_layout();
    true
}

/// Mouse up completing a press that started on the proof window.
pub fn pointer_up(x: i32, y: i32) -> bool {
    if super::session::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::shortcuts::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::dialogs::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::switcher::dispatch_pointer_up(x, y) {
        super::switcher::drain_pending();
        super::switcher::damage();
        return true;
    }
    if super::notify::dispatch_pointer_up(x, y) {
        super::notify::damage();
        return true;
    }
    if super::tray::dispatch_pointer_up(x, y) {
        super::tray::damage();
        return true;
    }
    if super::osd::dispatch_pointer_up(x, y) {
        super::osd::damage();
        return true;
    }
    if super::desktop::dispatch_menu_pointer_up(x, y) {
        super::desktop::damage_menu();
        return true;
    }
    if super::launcher::dispatch_pointer_up(x, y) {
        super::launcher::damage();
        super::launcher::drain_pending();
        return true;
    }
    if super::panel::dispatch_pointer_up(x, y) {
        super::panel::damage();
        return true;
    }
    if super::calc::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::settings::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::files::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::tasks::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::editor::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::apps::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::browse::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::chat::dispatch_pointer_up(x, y) {
        return true;
    }
    if super::term::dispatch_pointer_up(x, y) {
        return true;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = proof_window_id() else {
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
    let local = screen_to_local(bounds.x(), bounds.y(), x, y);
    let size = Size::wh(proof::CARD_W, proof::CARD_H);
    {
        let input = win.input_mut();
        let mut root = proof_tree(&theme, 0.0, false);
        prepare_element(&mut root, Point::xy(proof::PAD, proof::PAD), size, input);
        let _ = dispatch_mouse_up(&mut root, input, local);
    }
    super::proof::end_title_drag();
    win.request_layout();
    true
}

/// Scroll over the proof window (D6.1).
pub fn scroll(x: i32, y: i32, delta_y: i32) -> bool {
    if super::dialogs::dispatch_scroll(x, y, delta_y) {
        return true;
    }
    if super::notify::dispatch_scroll(x, y, delta_y) {
        super::notify::damage();
        return true;
    }
    if super::launcher::dispatch_scroll(x, y, delta_y) {
        super::launcher::damage();
        return true;
    }
    if super::settings::dispatch_scroll(x, y, delta_y) {
        return true;
    }
    if super::tasks::dispatch_scroll(x, y, delta_y) {
        return true;
    }
    if super::editor::dispatch_scroll(x, y, delta_y) {
        return true;
    }
    if super::apps::dispatch_scroll(x, y, delta_y) {
        return true;
    }
    if super::browse::dispatch_scroll(x, y, delta_y) {
        return true;
    }
    if super::chat::dispatch_scroll(x, y, delta_y) {
        return true;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = proof_window_id() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    if !bounds.contains(Point::xy(x, y)) {
        return false;
    }
    let local = screen_to_local(bounds.x(), bounds.y(), x, y);
    let size = Size::wh(proof::CARD_W, proof::CARD_H);
    {
        let input = win.input_mut();
        input.set_pointer_position(local);
        input.set_modifiers(crate::task::keyboard::modifiers());
        let mut root = proof_tree(&theme, 0.0, false);
        prepare_element(&mut root, Point::xy(proof::PAD, proof::PAD), size, input);
        dispatch_scroll(&mut root, input, Point::xy(0, delta_y));
    }
    win.request_layout();
    true
}

/// Cursor for the proof surface, or `None` if the pointer is not over it.
pub fn cursor_style_at(x: i32, y: i32) -> Option<CursorStyle> {
    if let Some(style) = super::switcher::cursor_style_at(x, y) {
        return Some(style);
    }
    if let Some(style) = super::notify::cursor_style_at(x, y) {
        return Some(style);
    }
    if let Some(style) = super::tray::cursor_style_at(x, y) {
        return Some(style);
    }
    if let Some(style) = super::osd::cursor_style_at(x, y) {
        return Some(style);
    }
    if let Some(style) = super::launcher::cursor_style_at(x, y) {
        return Some(style);
    }
    if let Some(style) = super::desktop::cursor_style_at(x, y) {
        return Some(style);
    }
    if let Some(style) = super::panel::cursor_style_at(x, y) {
        return Some(style);
    }
    let mut slot = APP.lock();
    let app = slot.as_mut()?;
    let id = proof_window_id()?;
    let win = app.window(id)?;
    if !win.bounds().contains(Point::xy(x, y)) {
        return None;
    }
    let local = screen_to_local(win.bounds().x(), win.bounds().y(), x, y);
    Some(win.input().cursor_at(local))
}
