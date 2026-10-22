// SPDX-License-Identifier: MIT
//! Keyboard, keymap, and IME dispatch into the GPUI proof window (D6).

use core::sync::atomic::{AtomicBool, Ordering};

use knox_gpui::{
    Key, KeyDownEvent, Keystroke, Modifiers, Point, Size, dispatch_ime_commit, dispatch_key_down,
    prepare_element,
};

use super::platform::APP;
use super::pointer;
use super::proof::{self, app_theme, proof_tree, proof_window_id};

static KEY_FOCUS: AtomicBool = AtomicBool::new(false);

pub fn claim_key_focus() {
    KEY_FOCUS.store(true, Ordering::SeqCst);
}

pub fn release_key_focus() {
    KEY_FOCUS.store(false, Ordering::SeqCst);
}

pub fn has_key_focus() -> bool {
    KEY_FOCUS.load(Ordering::SeqCst)
}

pub fn set_modifiers(modifiers: Modifiers) {
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return;
    };
    if let Some(id) = super::switcher::alt_tab_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    if let Some(id) = super::switcher::overview_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    if let Some(id) = super::osd::osd_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    if let Some(id) = super::tray::menu_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    if let Some(id) = super::desktop::menu_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    if let Some(id) = super::notify::center_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    if let Some(id) = super::notify::toast_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    if let Some(id) = super::launcher::launcher_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.input_mut().set_modifiers(modifiers);
        }
    }
    let Some(id) = proof_window_id() else {
        return;
    };
    if let Some(win) = app.window_mut(id) {
        win.input_mut().set_modifiers(modifiers);
    }
}

/// Named / character key (Tab, cmd-k, Backspace, …).
pub fn key_down(key: Key, modifiers: Modifiers) -> bool {
    set_modifiers(modifiers);
    if super::shortcuts::is_open() {
        if key == Key::Escape || (key == Key::Char('?') && !modifiers.control) {
            crate::gui::shortcuts_overlay::hide();
            return true;
        }
        return true;
    }
    if key == Key::Char('?') && !modifiers.control && !modifiers.alt && !modifiers.platform {
        crate::gui::shortcuts_overlay::toggle();
        return true;
    }
    if super::switcher::is_open() {
        return super::switcher::key_down(key, modifiers);
    }
    if super::tray::is_open() {
        return super::tray::key_down(key, modifiers);
    }
    if super::desktop::menu_is_open() {
        return super::desktop::key_down(key, modifiers);
    }
    if super::osd::is_open() {
        return super::osd::key_down(key, modifiers);
    }
    if super::notify::is_center_open() {
        return super::notify::key_down(key, modifiers);
    }
    if super::launcher::is_open() {
        let consumed = super::launcher::key_down(key, modifiers);
        super::launcher::drain_pending();
        return consumed;
    }
    if super::calc::key_down(key) {
        return true;
    }
    let over = pointer_over_proof();
    if key == Key::Tab {
        if !has_key_focus() && !over {
            return false;
        }
    } else if !has_key_focus() {
        return false;
    }

    if key == Key::Backspace && crate::gui::ime::is_composing() {
        crate::gui::ime::cancel_compose();
        pointer::damage_proof();
        return true;
    }
    if key == Key::Escape && crate::gui::ime::is_composing() {
        crate::gui::ime::cancel_compose();
        pointer::damage_proof();
        return true;
    }

    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = proof_window_id() else {
        return false;
    };

    let keystroke = Keystroke::new(key, modifiers);
    let context = app
        .window(id)
        .map(|w| w.input().context())
        .unwrap_or(knox_gpui::WindowInput::DEFAULT_CONTEXT);
    if let Some(action) = app.keymap().action_for(&keystroke, context) {
        handle_action(action);
        if let Some(win) = app.window_mut(id) {
            win.request_layout();
        }
        drop(slot);
        pointer::damage_proof();
        return true;
    }

    // Printable characters without chords go through IME (`char_input`).
    if matches!(key, Key::Char(_) | Key::Space)
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
    {
        return false;
    }

    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let size = Size::wh(proof::CARD_W, proof::CARD_H);
    let consumed = {
        let input = win.input_mut();
        let mut root = proof_tree(&theme, 0.0, false);
        prepare_element(&mut root, Point::xy(proof::PAD, proof::PAD), size, input);
        dispatch_key_down(&mut root, input, &KeyDownEvent::new(key, modifiers))
    };
    if consumed {
        claim_key_focus();
        win.request_layout();
        sync_ime_caret(win);
    }
    drop(slot);
    if consumed {
        pointer::damage_focused();
    }
    consumed
}

/// Printable character → IME → focused field (D6.4).
pub fn char_input(c: char) -> bool {
    if super::launcher::is_open() {
        return super::launcher::char_input(c);
    }
    if super::calc::char_input(c) {
        return true;
    }
    if !has_key_focus() {
        return false;
    }
    if c.is_control() || crate::task::keyboard::is_ctrl_held() {
        return false;
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
    if !win.input().focused_accepts_ime() {
        return false;
    }

    if let Some(text) = crate::gui::ime::process_char(c) {
        let size = Size::wh(proof::CARD_W, proof::CARD_H);
        let input = win.input_mut();
        let mut root = proof_tree(&theme, 0.0, false);
        prepare_element(&mut root, Point::xy(proof::PAD, proof::PAD), size, input);
        dispatch_ime_commit(&mut root, input, &text);
    }
    win.request_layout();
    sync_ime_caret(win);
    drop(slot);
    pointer::damage_focused();
    true
}

fn handle_action(action: &str) {
    crate::serial_println!("[gpui] action {}", action);
    match action {
        "command_palette" => crate::gui::startmenu::toggle(),
        "shortcuts" => crate::gui::shortcuts_overlay::toggle(),
        "dismiss" => crate::gui::shortcuts_overlay::hide(),
        _ => {}
    }
}

fn sync_ime_caret(win: &knox_gpui::Window) {
    if let Some(b) = win.input().focused_bounds() {
        crate::gui::ime::set_cursor_position(
            win.bounds().x() + b.x(),
            win.bounds().y() + b.bottom().0,
        );
    }
}

fn pointer_over_proof() -> bool {
    let mouse = crate::gui::input::MOUSE.lock();
    let (x, y) = (mouse.x, mouse.y);
    drop(mouse);
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = proof_window_id() else {
        return false;
    };
    app.window(id)
        .map(|w| w.bounds().contains(Point::xy(x, y)))
        .unwrap_or(false)
}
