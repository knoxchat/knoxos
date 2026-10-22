// SPDX-License-Identifier: MIT
//! D9.6 Alt-Tab and overview as Knox GPUI overlays.
//! Replaces `alt_tab.rs` / `expose.rs` paint. Marker `GATE_GPUI alt-tab`.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::gui::alt_tab;
use crate::gui::expose;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::window::{WINDOW_MANAGER, WindowContentType, WindowId as WmWindowId};
use knox_gpui::{
    Bounds, IntoElement, Key, KeyDownEvent, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_key_down, dispatch_mouse_down_button, dispatch_mouse_move,
    dispatch_mouse_up, paint_element_with_input, prepare_element, px,
};
use knox_ui::{AltTab, IconName, Overview, SwitcherEntry, Theme};

use super::keys;
use super::platform::APP;
use super::proof::app_theme;

pub const GATE_GPUI_ALT_TAB: &str = "GATE_GPUI alt-tab";

static ALT_TAB_WINDOW: AtomicU64 = AtomicU64::new(0);
static OVERVIEW_WINDOW: AtomicU64 = AtomicU64::new(0);
static SWITCHER_GATE: AtomicBool = AtomicBool::new(false);
static PENDING_FOCUS: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    AltTab,
    Overview,
}

pub fn is_live() -> bool {
    ALT_TAB_WINDOW.load(Ordering::SeqCst) != 0 && OVERVIEW_WINDOW.load(Ordering::SeqCst) != 0
}

pub fn is_open() -> bool {
    alt_tab::is_visible() || expose::is_visible()
}

fn kind() -> Option<Kind> {
    if alt_tab::is_visible() {
        Some(Kind::AltTab)
    } else if expose::is_visible() {
        Some(Kind::Overview)
    } else {
        None
    }
}

pub fn alt_tab_window_id() -> Option<WindowId> {
    let raw = ALT_TAB_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

pub fn overview_window_id() -> Option<WindowId> {
    let raw = OVERVIEW_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

fn hidden_rect() -> Rect {
    Rect::new(0, 0, 1, 1)
}

fn icon_for(ct: WindowContentType) -> IconName {
    crate::gui::icon_theme::for_content(ct)
}

fn map_entry(id: WmWindowId, title: &str, ct: WindowContentType) -> SwitcherEntry {
    SwitcherEntry::new(id, title, icon_for(ct))
}

fn alt_tab_entries() -> (usize, Vec<SwitcherEntry>) {
    let state = alt_tab::ALT_TAB.lock();
    let selected = state.selected_index;
    let entries = state
        .entries
        .iter()
        .take(AltTab::MAX)
        .map(|e| map_entry(e.window_id, &e.title, e.content_type))
        .collect();
    (selected, entries)
}

fn overview_entries() -> (Option<usize>, Vec<SwitcherEntry>) {
    let state = expose::EXPOSE_STATE.lock();
    let selected = state.selected;
    let entries = state
        .entries
        .iter()
        .map(|e| map_entry(e.window_id, &e.title, e.content_type))
        .collect();
    (selected, entries)
}

fn centered(screen_w: u32, screen_h: u32, w: i32, h: i32) -> Rect {
    let w = w.min(screen_w as i32 - 40).max(1);
    let h = h.min(screen_h as i32 - 80).max(1);
    let x = ((screen_w as i32 - w) / 2).max(8);
    let y = ((screen_h as i32 - h) / 2).max(8);
    Rect::new(x, y, w as u32, h as u32)
}

pub fn alt_tab_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    if !alt_tab::is_visible() {
        return hidden_rect();
    }
    let n = alt_tab::ALT_TAB.lock().entries.len();
    if n == 0 {
        return hidden_rect();
    }
    let (w, h) = AltTab::panel_size(n);
    centered(screen_w, screen_h, w, h)
}

pub fn overview_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    if !expose::is_visible() {
        return hidden_rect();
    }
    let n = expose::EXPOSE_STATE.lock().entries.len();
    if n == 0 {
        return hidden_rect();
    }
    let (w, h) = Overview::panel_size(n);
    centered(screen_w, screen_h, w, h)
}

fn queue_focus(id: u32) {
    PENDING_FOCUS.store(id, Ordering::SeqCst);
    alt_tab::dismiss();
    expose::dismiss();
}

pub fn dismiss() {
    alt_tab::dismiss();
    expose::dismiss();
    PENDING_FOCUS.store(0, Ordering::SeqCst);
    damage();
}

pub fn drain_pending() {
    let id = PENDING_FOCUS.swap(0, Ordering::SeqCst);
    if id == 0 {
        return;
    }
    let mut wm = WINDOW_MANAGER.lock();
    wm.focus_window(id);
    drop(wm);
    crate::gui::taskbar::set_active(id);
}

pub(super) fn alt_tab_tree(theme: &Theme) -> knox_gpui::Div {
    if !alt_tab::is_visible() {
        return knox_gpui::div();
    }
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        alt_tab_dest_rect(sw as u32, sh as u32)
    };
    let (selected, entries) = alt_tab_entries();
    AltTab::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .entries(entries)
        .selected(selected)
        .on_select(queue_focus)
        .on_dismiss(dismiss)
        .into_element()
}

pub(super) fn overview_tree(theme: &Theme) -> knox_gpui::Div {
    if !expose::is_visible() {
        return knox_gpui::div();
    }
    let dest = {
        let (sw, sh) = crate::gui::cached_screen_size();
        overview_dest_rect(sw as u32, sh as u32)
    };
    let (selected, entries) = overview_entries();
    Overview::new()
        .themed(theme)
        .size(px(dest.width as i32), px(dest.height as i32))
        .entries(entries)
        .selected(selected)
        .on_select(queue_focus)
        .on_highlight(|i| {
            let mut state = expose::EXPOSE_STATE.lock();
            if state.selected != Some(i) {
                state.selected = Some(i);
            }
        })
        .on_dismiss(dismiss)
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

pub fn paint_alt_tab_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    if !alt_tab::is_visible() {
        return Scene::new();
    }
    paint_tree(alt_tab_tree(theme), input, atlas, width, height)
}

pub fn paint_overview_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
) -> Scene {
    if !expose::is_visible() {
        return Scene::new();
    }
    paint_tree(overview_tree(theme), input, atlas, width, height)
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

pub fn ensure_alt_tab_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    ensure_window(&ALT_TAB_WINDOW, alt_tab_dest_rect(screen_w, screen_h), app)
}

pub fn ensure_overview_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    ensure_window(
        &OVERVIEW_WINDOW,
        overview_dest_rect(screen_w, screen_h),
        app,
    )
}

pub fn emit_gate() {
    if SWITCHER_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", GATE_GPUI_ALT_TAB);
}

pub fn contains_screen(x: i32, y: i32) -> bool {
    if !is_open() {
        return false;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    if alt_tab::is_visible() {
        return alt_tab_dest_rect(sw as u32, sh as u32).contains(x, y);
    }
    if expose::is_visible() {
        return overview_dest_rect(sw as u32, sh as u32).contains(x, y);
    }
    false
}

fn local_point(bounds: Bounds, x: i32, y: i32) -> Point {
    Point::xy(x - bounds.x(), y - bounds.y())
}

fn active_window_id() -> Option<WindowId> {
    match kind()? {
        Kind::AltTab => alt_tab_window_id(),
        Kind::Overview => overview_window_id(),
    }
}

fn active_tree(theme: &Theme) -> knox_gpui::Div {
    match kind() {
        Some(Kind::AltTab) => alt_tab_tree(theme),
        Some(Kind::Overview) => overview_tree(theme),
        None => knox_gpui::div(),
    }
}

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    if !is_open() {
        return false;
    }
    if !contains_screen(x, y) {
        dismiss();
        return true;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = active_window_id() else {
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
        let mut root = active_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    keys::claim_key_focus();
    win.request_layout();
    true
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    if !is_open() && PENDING_FOCUS.load(Ordering::SeqCst) == 0 {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(id) = active_window_id().or_else(|| alt_tab_window_id().or_else(overview_window_id))
    else {
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
        let mut root = if alt_tab::is_visible() {
            alt_tab_tree(&theme)
        } else if expose::is_visible() {
            overview_tree(&theme)
        } else {
            // Overlay may already be dismissed by on_click; still complete the click.
            knox_gpui::div()
        };
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
    let Some(id) = active_window_id() else {
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
            let mut root = active_tree(&theme);
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
    let Some(id) = active_window_id() else {
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
        let mut root = active_tree(&theme);
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_key_down(&mut root, input, &KeyDownEvent::new(key, modifiers))
    };
    if key == Key::Escape {
        drop(slot);
        dismiss();
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
    let id = active_window_id()?;
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
    if !is_open() {
        return;
    }
    let (sw, sh) = crate::gui::cached_screen_size();
    let screen = Rect::new(0, 0, sw as u32, sh as u32);
    if is_full || damage.intersects(&screen) {
        fb.fill_rect(screen, Pixel::new(0, 0, 0, 120));
    }
    present_id(
        fb,
        damage,
        is_full,
        alt_tab_window_id(),
        alt_tab::is_visible(),
    );
    present_id(
        fb,
        damage,
        is_full,
        overview_window_id(),
        expose::is_visible(),
    );
}

pub fn damage() {
    crate::gui::request_redraw();
}
