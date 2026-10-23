// SPDX-License-Identifier: MIT
//! D1.7 proof card + D3.6 button + D6 two buttons, Tab ring, Input + D7 theme.
//! D8.1–D8.3: knox-ui catalog rows (gallery stories).

use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use knox_gpui::{
    Bounds, Div, PaintEnv, Point, Scene, Shadow, Size, SpriteAtlas, WindowId, WindowInput,
    paint_element_with_input,
};
use knox_ui::{
    D8ButtonsAction, D8ButtonsSnapshot, D8DateAction, D8DateSnapshot, D8FeedbackAction,
    D8FeedbackSnapshot, D8FormsAction, D8FormsSnapshot, D8InputAction, D8InputSnapshot,
    D8OverlayAction, D8OverlaySnapshot, D8RestAction, D8RestSnapshot, D8SelectAction,
    D8SelectSnapshot, FieldState, KitGalleryProps, Theme, apply_d8_rest, kit_gallery,
};
use spin::Mutex;

use crate::gui::framebuffer::Rect;

/// Serial marker grepped by `tests/run_integration.sh`.
pub const GATE_GPUI_SCENE_RASTER: &str = "GATE_GPUI scene raster";
pub const GATE_GPUI_INTERACT: &str = "GATE_GPUI interact";

pub const CARD_W: i32 = 1280;
pub const CARD_H: i32 = 860;
pub const PAD: i32 = 24;
pub const GATE_GPUI_KIT_GALLERY: &str = "GATE_GPUI kit gallery";
pub const GATE_GPUI_CHROME: &str = "GATE_GPUI chrome";

pub(super) static PROOF_WINDOW: AtomicU64 = AtomicU64::new(0);
pub(super) static ACCENT_ON: AtomicBool = AtomicBool::new(true);
static GALLERY_HIDDEN: AtomicBool = AtomicBool::new(true);
static GALLERY_MAXIMIZED: AtomicBool = AtomicBool::new(false);
static GALLERY_MINIMIZING: AtomicBool = AtomicBool::new(false);
static D8: Mutex<D8ButtonsSnapshot> = Mutex::new(D8ButtonsSnapshot {
    switch_on: true,
    checked: true,
    radio: 1,
    group: 0,
    toggle: false,
    toggle_a: true,
    toggle_b: false,
    dropdown_open: false,
    dropdown_choice: 0,
});
static D8F: Mutex<D8FeedbackSnapshot> = Mutex::new(D8FeedbackSnapshot {
    alert_open: true,
    copied: false,
    phase: 0.0,
    reduced_motion: false,
});
static D8I: Mutex<D8InputSnapshot> = Mutex::new(D8InputSnapshot {
    line: knox_ui::FieldState::empty(),
    area: knox_ui::FieldState::empty(),
    group: knox_ui::FieldState::empty(),
});
static D8M: Mutex<D8FormsSnapshot> = Mutex::new(D8FormsSnapshot {
    number: 4,
    number_text: knox_ui::FieldState::empty(),
    otp: String::new(),
    slider: 40.0,
    color: knox_gpui::Hsla::RED,
    name: knox_ui::FieldState::empty(),
});
static D8S: Mutex<D8SelectSnapshot> = Mutex::new(D8SelectSnapshot {
    select_open: false,
    select_idx: 0,
    combo_open: true,
    combo_query: knox_ui::FieldState::empty(),
    combo_idx: None,
    combo_hi: 0,
    list_query: knox_ui::FieldState::empty(),
    list_idx: Some(0),
    list_hi: 0,
});
static D8D: Mutex<D8DateSnapshot> = Mutex::new(D8DateSnapshot {
    view: knox_ui::Date::new(2026, 10, 1),
    selected: knox_ui::Date::new(2026, 10, 22),
    open: true,
    time: knox_ui::Time::new(9, 30),
    segment: 0,
});
static D8O: Mutex<D8OverlaySnapshot> = Mutex::new(D8OverlaySnapshot {
    dialog: false,
    alert: false,
    sheet: false,
    popover: false,
    hover: false,
});
static D8R: Mutex<D8RestSnapshot> = Mutex::new(D8RestSnapshot::INIT);
static D8CMD: Mutex<FieldState> = Mutex::new(FieldState::empty());
static D8SEARCH: Mutex<FieldState> = Mutex::new(FieldState::empty());
static DRAG_GRAB: Mutex<Option<(i32, i32)>> = Mutex::new(None);
static USER_ORIGIN: Mutex<Option<(i32, i32)>> = Mutex::new(None);

pub fn buffer_size() -> (i32, i32) {
    (CARD_W + PAD * 2, CARD_H + PAD * 2)
}

pub fn proof_dest_rect(screen_w: u32, screen_h: u32) -> Rect {
    if GALLERY_HIDDEN.load(Ordering::SeqCst) {
        return Rect::new(-64, -64, 1, 1);
    }
    if GALLERY_MAXIMIZED.load(Ordering::SeqCst) {
        let panel = crate::gui::scale::taskbar_height() as i32;
        let w = (screen_w as i32 - 24).max(buffer_size().0);
        let h = (screen_h as i32 - panel - 24).max(buffer_size().1);
        return Rect::new(12, 12, w as u32, h as u32);
    }
    let (w, h) = buffer_size();
    let x = (screen_w as i32 - w) / 2;
    let y = (screen_h as i32 - h) / 2;
    Rect::new(x, y, w as u32, h as u32)
}

pub fn gallery_hidden() -> bool {
    GALLERY_HIDDEN.load(Ordering::SeqCst)
}

pub fn gallery_minimizing() -> bool {
    GALLERY_MINIMIZING.load(Ordering::SeqCst)
}

pub fn gallery_maximized() -> bool {
    GALLERY_MAXIMIZED.load(Ordering::SeqCst)
}

fn hide_gallery() {
    GALLERY_HIDDEN.store(true, Ordering::SeqCst);
    GALLERY_MINIMIZING.store(false, Ordering::SeqCst);
    crate::gui::request_redraw();
}

/// Hide the Kit Gallery overlay (start-menu / Escape).
pub fn hide_kit_gallery() {
    hide_gallery();
}

fn begin_gallery_minimize() {
    if crate::gui::accessibility::is_reduced_motion() {
        hide_gallery();
        return;
    }
    GALLERY_MINIMIZING.store(true, Ordering::SeqCst);
    crate::gui::request_redraw();
}

fn toggle_gallery_maximized() {
    GALLERY_MAXIMIZED.fetch_xor(true, Ordering::SeqCst);
    crate::gui::request_redraw();
}

pub fn finish_gallery_minimize() {
    hide_gallery();
}

pub(super) fn proof_window_id() -> Option<WindowId> {
    let raw = PROOF_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(WindowId::from_u64(raw))
    }
}

pub(super) fn theme_from_kernel() -> Theme {
    let base = match crate::gui::theme::active_theme() {
        crate::gui::theme::ThemeId::ArcticLight => Theme::arctic_light(),
        crate::gui::theme::ThemeId::SunsetWarm => Theme::sunset_warm(),
        _ => Theme::aurora_dark(),
    };
    base.with_high_contrast(crate::gui::accessibility::is_high_contrast())
        .with_reduced_motion(crate::gui::accessibility::is_reduced_motion())
}

pub(super) fn app_theme(app: &knox_gpui::App) -> Theme {
    app.global::<Theme>()
        .copied()
        .unwrap_or_else(theme_from_kernel)
}

/// Store-local scene: rounded card + shadow + kit gallery tree.
pub fn paint_proof_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    _accent_on: bool,
    theme: &Theme,
    phase: f32,
    reduced_motion: bool,
) -> Scene {
    let card = Bounds::from_xywh(PAD, PAD, CARD_W, CARD_H);
    let mut scene = Scene::new();
    let sh = theme.shadow.md;
    scene.push_shadow(
        Shadow::new(card, sh.color)
            .rounded(theme.radius.xl)
            .blur(sh.blur)
            .offset(sh.offset),
    );

    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = proof_tree(theme, phase, reduced_motion);
    paint_element_with_input(
        &mut root,
        Point::xy(PAD, PAD),
        Size::wh(CARD_W, CARD_H),
        &mut scene,
        input,
        &mut env,
    );
    scene
}

fn apply_d8(action: D8ButtonsAction) {
    let mut s = D8.lock();
    match action {
        D8ButtonsAction::Switch(v) => s.switch_on = v,
        D8ButtonsAction::Check(v) => s.checked = v,
        D8ButtonsAction::Radio(v) => s.radio = v,
        D8ButtonsAction::Group(v) => s.group = v,
        D8ButtonsAction::Toggle(v) => s.toggle = v,
        D8ButtonsAction::ToggleGroup { index, on } => match index {
            0 => s.toggle_a = on,
            _ => s.toggle_b = on,
        },
        D8ButtonsAction::DropdownToggle => s.dropdown_open = !s.dropdown_open,
        D8ButtonsAction::DropdownPick(v) => {
            s.dropdown_choice = v;
            s.dropdown_open = false;
        }
        D8ButtonsAction::Link => {}
    }
}

fn apply_d8_feedback(action: D8FeedbackAction) {
    match action {
        D8FeedbackAction::DismissAlert => D8F.lock().alert_open = false,
        D8FeedbackAction::Copy => {
            crate::gui::clipboard::copy_text("knox-gpui", None);
            D8F.lock().copied = true;
        }
    }
}

fn apply_d8_input(action: D8InputAction) {
    let mut s = D8I.lock();
    match action {
        D8InputAction::Line(v) => s.line = v,
        D8InputAction::Area(v) => s.area = v,
        D8InputAction::Group(v) => s.group = v,
        D8InputAction::Copy(text) => crate::gui::clipboard::copy_text(&text, None),
    }
}

fn paste_from_clipboard() -> Option<String> {
    crate::gui::clipboard::paste_text(crate::gui::clipboard::Selection::Clipboard)
}

fn apply_d8_forms(action: D8FormsAction) {
    let mut s = D8M.lock();
    match action {
        D8FormsAction::Number(v) => {
            s.number = v;
            s.number_text = knox_ui::FieldState::from(alloc::format!("{v}"));
        }
        D8FormsAction::NumberText(v) => s.number_text = v,
        D8FormsAction::Otp(v) => s.otp = v,
        D8FormsAction::Slider(v) => s.slider = v,
        D8FormsAction::Color(v) => s.color = v,
        D8FormsAction::Name(v) => s.name = v,
    }
}

fn apply_d8_select(action: D8SelectAction) {
    let mut s = D8S.lock();
    match action {
        D8SelectAction::SelectToggle => s.select_open = !s.select_open,
        D8SelectAction::Select(i) => {
            s.select_idx = i;
            s.select_open = false;
        }
        D8SelectAction::ComboToggle => s.combo_open = !s.combo_open,
        D8SelectAction::ComboQuery(v) => {
            s.combo_query = v;
            s.combo_open = true;
        }
        D8SelectAction::Combo(i) => {
            s.combo_idx = Some(i);
            s.combo_open = false;
        }
        D8SelectAction::ComboHighlight(i) => s.combo_hi = i,
        D8SelectAction::ListQuery(v) => s.list_query = v,
        D8SelectAction::List(i) => s.list_idx = Some(i),
        D8SelectAction::ListHighlight(i) => s.list_hi = i,
    }
}

fn apply_d8_date(action: D8DateAction) {
    let mut s = D8D.lock();
    match action {
        D8DateAction::View(d) => s.view = d,
        D8DateAction::Select(d) => {
            s.selected = d;
            s.view = knox_ui::Date::new(d.year, d.month, 1);
            s.open = false;
        }
        D8DateAction::Toggle => s.open = !s.open,
        D8DateAction::Time(t) => s.time = t,
        D8DateAction::Segment(v) => s.segment = v,
    }
}

fn apply_d8_overlay(action: D8OverlayAction) {
    let mut s = D8O.lock();
    match action {
        D8OverlayAction::Dialog(v) => s.dialog = v,
        D8OverlayAction::Alert(v) => s.alert = v,
        D8OverlayAction::Sheet(v) => s.sheet = v,
        D8OverlayAction::Popover(v) => s.popover = v,
        D8OverlayAction::Hover(v) => s.hover = v,
    }
}

fn apply_rest(action: D8RestAction) {
    apply_d8_rest(&mut D8R.lock(), action);
}

pub(super) fn begin_title_drag(local: Point) {
    *DRAG_GRAB.lock() = Some((local.x.0, local.y.0));
}

pub(super) fn end_title_drag() {
    *DRAG_GRAB.lock() = None;
}

pub(super) fn drag_grab() -> Option<(i32, i32)> {
    *DRAG_GRAB.lock()
}

pub(super) fn set_user_origin(x: i32, y: i32) {
    *USER_ORIGIN.lock() = Some((x, y));
}

pub fn gallery_origin(screen_w: u32, screen_h: u32) -> (i32, i32) {
    if let Some(o) = *USER_ORIGIN.lock() {
        return o;
    }
    let dest = proof_dest_rect(screen_w, screen_h);
    (dest.x, dest.y)
}

pub fn focus_kit_gallery() {
    *USER_ORIGIN.lock() = None;
    GALLERY_HIDDEN.store(false, Ordering::SeqCst);
    GALLERY_MINIMIZING.store(false, Ordering::SeqCst);
    D8R.lock().page = 0;
    super::pump::begin_gallery_enter();
    crate::serial_println!("[gpui] {}", GATE_GPUI_KIT_GALLERY);
    crate::gui::request_redraw();
}

pub(super) fn proof_tree(theme: &Theme, phase: f32, reduced_motion: bool) -> Div {
    let snap = *D8.lock();
    let mut feedback = *D8F.lock();
    feedback.phase = phase;
    feedback.reduced_motion = reduced_motion;
    let inputs = D8I.lock().clone();
    let forms = D8M.lock().clone();
    let selects = D8S.lock().clone();
    let dates = *D8D.lock();
    let overlays = *D8O.lock();
    let rest = *D8R.lock();
    let cmd_query = D8CMD.lock().clone();
    let search = D8SEARCH.lock().clone();

    kit_gallery(&KitGalleryProps {
        theme,
        rest,
        buttons: snap,
        feedback,
        inputs,
        forms,
        selects,
        dates,
        overlays,
        search,
        cmd_query,
        maximized: GALLERY_MAXIMIZED.load(Ordering::SeqCst),
        card_w: CARD_W,
        card_h: CARD_H,
        on_rest: Arc::new(apply_rest),
        on_buttons: Arc::new(apply_d8),
        on_feedback: Arc::new(apply_d8_feedback),
        on_input: Arc::new(apply_d8_input),
        on_forms: Arc::new(apply_d8_forms),
        on_select: Arc::new(apply_d8_select),
        on_date: Arc::new(apply_d8_date),
        on_overlay: Arc::new(apply_d8_overlay),
        on_search: Arc::new(|s| {
            *D8SEARCH.lock() = s;
        }),
        on_cmd: Arc::new(|s| {
            *D8CMD.lock() = s;
        }),
        paste: Arc::new(paste_from_clipboard),
        on_min: Arc::new(begin_gallery_minimize),
        on_max: Arc::new(toggle_gallery_maximized),
        on_close: Arc::new(hide_gallery),
        on_drag: Arc::new(begin_title_drag),
    })
}
