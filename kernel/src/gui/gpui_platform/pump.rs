// SPDX-License-Identifier: MIT
//! Frame pump: dirty GPUI windows layout+paint into their stores, then blit.
//!
//! D2.5: the D1.7 proof card is a window store blit, not an ad-hoc Scene in
//! the compositor. Marker `GATE_GPUI pump`.
//! D3.6: the card is a `div` tree with a hover/click button. Marker
//! `GATE_GPUI interact`.
//! D7.4: enter animation (opacity+y). Marker `GATE_GPUI motion`.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use core::time::Duration;

use knox_gpui::{Bounds, ChromeKind, Point, Scene, Size, WindowId};
use knox_ui::{Theme, enter_opacity_y, exit_opacity_y};

use crate::gui::framebuffer::{FrameBuffer, Rect};

use super::dispatcher;
use super::platform::APP;
use super::proof::{
    ACCENT_ON, GATE_GPUI_CHROME, GATE_GPUI_INTERACT, GATE_GPUI_KIT_GALLERY, GATE_GPUI_SCENE_RASTER,
    PROOF_WINDOW, app_theme, gallery_hidden, gallery_minimizing, gallery_origin, paint_proof_scene,
    proof_dest_rect, theme_from_kernel,
};
use super::window::draw_scene;

/// Serial marker grepped by `tests/run_integration.sh`.
pub const GATE_GPUI_PUMP: &str = "GATE_GPUI pump";
pub const GATE_GPUI_MOTION: &str = "GATE_GPUI motion";

static PUMP_GATE: AtomicBool = AtomicBool::new(false);
static RASTER_GATE: AtomicBool = AtomicBool::new(false);
static INTERACT_GATE: AtomicBool = AtomicBool::new(false);
static MOTION_GATE: AtomicBool = AtomicBool::new(false);
static KIT_GALLERY_GATE: AtomicBool = AtomicBool::new(false);
static ENTER_START_MS: AtomicU64 = AtomicU64::new(0);
static EXIT_START_MS: AtomicU64 = AtomicU64::new(0);
static LAST_Y_OFFSET: AtomicI32 = AtomicI32::new(0);
static CHROME_GATE: AtomicBool = AtomicBool::new(false);

pub fn pump(fb: &mut FrameBuffer, screen_w: u32, screen_h: u32, damage: Rect, is_full: bool) {
    emit_once(&PUMP_GATE, GATE_GPUI_PUMP);
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return;
    };

    app.dispatcher_mut().set_now(dispatcher::now_ticks());
    app.pump();

    app.set_global(theme_from_kernel());

    let proof_id = ensure_proof_window(app, screen_w, screen_h);
    let panel_id = super::panel::ensure_panel_window(app, screen_w, screen_h);
    let desktop_id = super::desktop::ensure_icons_window(app, screen_w, screen_h);
    let rubber_id = super::desktop::ensure_rubber_window(app, screen_w, screen_h);
    let desktop_menu_id = super::desktop::ensure_menu_window(app, screen_w, screen_h);
    let launcher_id = super::launcher::ensure_launcher_window(app, screen_w, screen_h);
    let osd_id = super::osd::ensure_osd_window(app, screen_w, screen_h);
    let toast_id = super::notify::ensure_toast_window(app, screen_w, screen_h);
    let center_id = super::notify::ensure_center_window(app, screen_w, screen_h);
    let tray_id = super::tray::ensure_menu_window(app, screen_w, screen_h);
    let alt_tab_id = super::switcher::ensure_alt_tab_window(app, screen_w, screen_h);
    let overview_id = super::switcher::ensure_overview_window(app, screen_w, screen_h);
    let _calc_ids = super::calc::ensure_calc_windows(app);
    let _settings_ids = super::settings::ensure_settings_windows(app);
    let _files_ids = super::files::ensure_files_windows(app);
    let _tasks_ids = super::tasks::ensure_tasks_windows(app);
    let _editor_ids = super::editor::ensure_editor_windows(app);
    let _app_ids = super::apps::ensure_app_windows(app);
    let _browse_ids = super::browse::ensure_browse_windows(app);
    let _chat_ids = super::chat::ensure_chat_windows(app);
    let _term_ids = super::term::ensure_term_windows(app);
    let _wizard_ids = super::dialogs::ensure_wizard_windows(app);
    let picker_id = super::dialogs::ensure_picker_window(app, screen_w, screen_h);
    let shot_id = super::dialogs::ensure_shot_window(app, screen_w, screen_h);
    let install_id = super::dialogs::ensure_install_window(app, screen_w, screen_h);
    let login_id = super::session::ensure_login_window(app, screen_w, screen_h);
    let lock_id = super::session::ensure_lock_window(app, screen_w, screen_h);
    let shortcuts_id = super::shortcuts::ensure_window(app, screen_w, screen_h);
    let scale = crate::gui::scale::scale_fp();
    let ids: Vec<WindowId> = app.windows().map(|w| w.id()).collect();
    let accent_on = ACCENT_ON.load(Ordering::SeqCst);
    let theme = app_theme(app);
    let reduced = crate::gui::accessibility::is_reduced_motion();
    let (mut opacity, mut y_offset, enter_done) = sample_enter(&theme, reduced);
    if gallery_minimizing() {
        if EXIT_START_MS.load(Ordering::SeqCst) == 0 {
            EXIT_START_MS.store(now_ms().max(1), Ordering::SeqCst);
        }
        let (exit_op, exit_y, exit_done) = sample_exit(&theme, reduced);
        opacity = exit_op;
        y_offset = exit_y;
        if exit_done {
            super::proof::finish_gallery_minimize();
            EXIT_START_MS.store(0, Ordering::SeqCst);
        }
    }

    let mut painted = false;
    for id in ids {
        let Some(win) = app.window_mut(id) else {
            continue;
        };
        win.set_scale_fp(scale);
        if id == proof_id {
            win.set_title("Kit Gallery");
            win.set_chrome(ChromeKind::Csd);
            if !gallery_hidden() {
                win.set_min_size(Size::wh(
                    super::proof::buffer_size().0,
                    super::proof::buffer_size().1,
                ));
            }
            let dest = proof_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y + y_offset,
                dest.width as i32,
                dest.height as i32,
            ));
            let (x, y) = gallery_origin(screen_w, screen_h);
            if !super::proof::gallery_maximized() && !gallery_hidden() {
                win.set_origin(Point::xy(x, y + y_offset));
            }
        }
        if id == panel_id {
            let dest = super::panel::panel_dest_rect(screen_w, screen_h);
            win.set_origin(Point::xy(dest.x, dest.y));
            if is_full || damage.intersects(&dest) {
                win.request_layout();
            }
        }
        if id == desktop_id {
            let dest = super::desktop::icons_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if is_full || damage.intersects(&dest) {
                win.request_layout();
            }
        }
        if id == rubber_id {
            let dest = super::desktop::rubber_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if crate::gui::desktop::is_rubber_band_active() {
                win.request_layout();
            }
        }
        if id == desktop_menu_id {
            let dest = super::desktop::menu_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if super::desktop::menu_is_open() {
                win.request_layout();
            }
        }
        if id == launcher_id {
            let dest = if crate::gui::startmenu::is_visible() {
                super::launcher::launcher_dest_rect(screen_w, screen_h)
            } else {
                crate::gui::framebuffer::Rect::new(0, 0, 1, 1)
            };
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if crate::gui::startmenu::is_visible() {
                win.request_layout();
            }
        }
        if id == osd_id {
            let dest = super::osd::osd_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if super::osd::is_open() {
                win.request_layout();
            }
        }
        if id == toast_id {
            let dest = super::notify::toast_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if super::notify::has_toasts() {
                win.request_layout();
            }
        }
        if id == center_id {
            let dest = super::notify::center_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if super::notify::is_center_open() {
                win.request_layout();
            }
        }
        if id == tray_id {
            let dest = super::tray::menu_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if super::tray::is_open() {
                win.request_layout();
            }
        }
        if id == alt_tab_id {
            let dest = super::switcher::alt_tab_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if crate::gui::alt_tab::is_visible() {
                win.request_layout();
            }
        }
        if id == overview_id {
            let dest = super::switcher::overview_dest_rect(screen_w, screen_h);
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if crate::gui::expose::is_visible() {
                win.request_layout();
            }
        }
        if id == picker_id {
            let dest = if crate::gui::file_picker::is_visible() {
                crate::gui::framebuffer::Rect::new(0, 0, screen_w, screen_h)
            } else {
                crate::gui::framebuffer::Rect::new(0, 0, 1, 1)
            };
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if crate::gui::file_picker::is_visible() {
                win.request_layout();
            }
        }
        if id == shot_id {
            let dest = if crate::gui::screenshot::is_dialog_open() {
                crate::gui::framebuffer::Rect::new(0, 0, screen_w, screen_h)
            } else {
                crate::gui::framebuffer::Rect::new(0, 0, 1, 1)
            };
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if crate::gui::screenshot::is_dialog_open() {
                win.request_layout();
            }
        }
        if id == install_id {
            let dest = if crate::gui::installer::is_active() {
                crate::gui::framebuffer::Rect::new(0, 0, screen_w, screen_h)
            } else {
                crate::gui::framebuffer::Rect::new(0, 0, 1, 1)
            };
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if crate::gui::installer::is_active() {
                win.request_layout();
            }
        }
        if id == login_id {
            let dest = if super::session::login_open() {
                crate::gui::framebuffer::Rect::new(0, 0, screen_w, screen_h)
            } else {
                crate::gui::framebuffer::Rect::new(0, 0, 1, 1)
            };
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if super::session::login_open() {
                win.request_layout();
            }
        }
        if id == lock_id {
            let dest = if super::session::lock_open() {
                crate::gui::framebuffer::Rect::new(0, 0, screen_w, screen_h)
            } else {
                crate::gui::framebuffer::Rect::new(0, 0, 1, 1)
            };
            win.set_bounds(Bounds::from_xywh(
                dest.x,
                dest.y,
                dest.width as i32,
                dest.height as i32,
            ));
            if super::session::lock_open() {
                win.request_layout();
            }
        }
        if id == proof_id && gallery_hidden() && !gallery_minimizing() {
            continue;
        }
        // Kit Gallery is a static catalog after the enter animation. Painting
        // it every frame (~108 knox-ui components at 1920×1080 on TCG) starved
        // pointer polling so the QEMU window could not move or click.
        let animating_gallery = id == proof_id && (!enter_done || gallery_minimizing());
        if !win.is_dirty() && !animating_gallery {
            continue;
        }
        let scene = if id == proof_id {
            let phase = if reduced {
                0.0
            } else {
                (now_ms() % 2000) as f32 / 2000.0
            };
            let (input, atlas) = win.paint_parts_mut();
            paint_proof_scene(input, atlas, accent_on, &theme, phase, reduced)
        } else if id == panel_id {
            let pw = win.bounds().width();
            let ph = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::panel::paint_panel_scene(input, atlas, &theme, pw, ph)
        } else if id == desktop_id {
            let dw = win.bounds().width();
            let dh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::desktop::paint_icons_scene(input, atlas, &theme, dw, dh)
        } else if id == rubber_id {
            if !crate::gui::desktop::is_rubber_band_active() {
                Scene::new()
            } else {
                let rw = win.bounds().width();
                let rh = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::desktop::paint_rubber_scene(input, atlas, &theme, rw, rh)
            }
        } else if id == desktop_menu_id {
            if !super::desktop::menu_is_open() {
                Scene::new()
            } else {
                let mw = win.bounds().width();
                let mh = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::desktop::paint_menu_scene(input, atlas, &theme, mw, mh)
            }
        } else if id == launcher_id {
            if !crate::gui::startmenu::is_visible() {
                Scene::new()
            } else {
                let lw = win.bounds().width();
                let lh = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::launcher::paint_launcher_scene(input, atlas, &theme, lw, lh)
            }
        } else if id == osd_id {
            if !super::osd::is_open() {
                Scene::new()
            } else {
                let ow = win.bounds().width();
                let oh = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::osd::paint_osd_scene(input, atlas, &theme, ow, oh)
            }
        } else if id == toast_id {
            if !super::notify::has_toasts() {
                Scene::new()
            } else {
                let tw = win.bounds().width();
                let th = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::notify::paint_toast_scene(input, atlas, &theme, tw, th)
            }
        } else if id == center_id {
            if !super::notify::is_center_open() {
                Scene::new()
            } else {
                let cw = win.bounds().width();
                let ch = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::notify::paint_center_scene(input, atlas, &theme, cw, ch)
            }
        } else if id == tray_id {
            if !super::tray::is_open() {
                Scene::new()
            } else {
                let mw = win.bounds().width();
                let mh = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::tray::paint_menu_scene(input, atlas, &theme, mw, mh)
            }
        } else if id == alt_tab_id {
            if !crate::gui::alt_tab::is_visible() {
                Scene::new()
            } else {
                let aw = win.bounds().width();
                let ah = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::switcher::paint_alt_tab_scene(input, atlas, &theme, aw, ah)
            }
        } else if id == overview_id {
            if !crate::gui::expose::is_visible() {
                Scene::new()
            } else {
                let ow = win.bounds().width();
                let oh = win.bounds().height();
                let (input, atlas) = win.paint_parts_mut();
                super::switcher::paint_overview_scene(input, atlas, &theme, ow, oh)
            }
        } else if let Some(wm_id) = super::calc::wm_id_for(id) {
            let cw = win.bounds().width();
            let ch = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::calc::paint_calc_scene(input, atlas, &theme, cw, ch, wm_id)
        } else if let Some(wm_id) = super::settings::wm_id_for(id) {
            let sw = win.bounds().width();
            let sh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::settings::paint_settings_scene(input, atlas, &theme, sw, sh, wm_id)
        } else if let Some(wm_id) = super::files::wm_id_for(id) {
            let fw = win.bounds().width();
            let fh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::files::paint_files_scene(input, atlas, &theme, fw, fh, wm_id)
        } else if let Some(wm_id) = super::tasks::wm_id_for(id) {
            let tw = win.bounds().width();
            let th = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::tasks::paint_tasks_scene(input, atlas, &theme, tw, th, wm_id)
        } else if let Some(wm_id) = super::editor::wm_id_for(id) {
            let ew = win.bounds().width();
            let eh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::editor::paint_editor_scene(input, atlas, &theme, ew, eh, wm_id)
        } else if let Some(wm_id) = super::apps::wm_id_for(id) {
            let aw = win.bounds().width();
            let ah = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::apps::paint_apps_scene(input, atlas, &theme, aw, ah, wm_id)
        } else if let Some(wm_id) = super::browse::wm_id_for(id) {
            let bw = win.bounds().width();
            let bh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::browse::paint_browse_scene(input, atlas, &theme, bw, bh, wm_id)
        } else if let Some(wm_id) = super::chat::wm_id_for(id) {
            let chw = win.bounds().width();
            let chh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::chat::paint_chat_scene(input, atlas, &theme, chw, chh, wm_id)
        } else if let Some(wm_id) = super::term::wm_id_for(id) {
            let tw = win.bounds().width();
            let th = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::term::paint_term_scene(input, atlas, &theme, tw, th, wm_id)
        } else if let Some(wm_id) = super::dialogs::wm_id_for(id) {
            let ww = win.bounds().width();
            let wh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::dialogs::paint_wizard_scene(input, atlas, &theme, ww, wh, wm_id)
        } else if id == picker_id {
            let pw = win.bounds().width();
            let ph = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::dialogs::paint_picker_scene(input, atlas, &theme, pw, ph)
        } else if id == shot_id {
            let sw = win.bounds().width();
            let sh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::dialogs::paint_shot_scene(input, atlas, &theme, sw, sh)
        } else if id == install_id {
            let iw = win.bounds().width();
            let ih = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::dialogs::paint_install_scene(input, atlas, &theme, iw, ih)
        } else if id == login_id {
            let lw = win.bounds().width();
            let lh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::session::paint_login_scene(input, atlas, &theme, lw, lh)
        } else if id == lock_id {
            let kw = win.bounds().width();
            let kh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::session::paint_lock_scene(input, atlas, &theme, kw, kh)
        } else if id == shortcuts_id {
            let sw = win.bounds().width();
            let sh = win.bounds().height();
            let (input, atlas) = win.paint_parts_mut();
            super::shortcuts::paint_scene(input, atlas, &theme, sw, sh)
        } else {
            Scene::new()
        };
        draw_scene(win, &scene);
        if id == proof_id && opacity < 1.0 {
            win.store_mut().modulate_opacity(opacity);
        }
        let store_id = super::calc::wm_id_for(id)
            .or_else(|| super::settings::wm_id_for(id))
            .or_else(|| super::files::wm_id_for(id))
            .or_else(|| super::tasks::wm_id_for(id))
            .or_else(|| super::editor::wm_id_for(id))
            .or_else(|| super::apps::wm_id_for(id))
            .or_else(|| super::browse::wm_id_for(id))
            .or_else(|| super::chat::wm_id_for(id))
            .or_else(|| super::term::wm_id_for(id))
            .or_else(|| super::dialogs::wm_id_for(id))
            .unwrap_or(id.as_u32());
        crate::gui::backing_store::STORE_MANAGER.lock().sync_packed(
            store_id,
            win.store().width() as u32,
            win.store().height() as u32,
            win.store().as_bytes(),
        );
        if id == proof_id && (!enter_done || gallery_minimizing()) {
            win.request_layout();
        }
        painted = true;
    }

    for win in app.windows() {
        if gallery_hidden() && Some(win.id()) == super::proof::proof_window_id() {
            continue;
        }
        if Some(win.id()) == super::launcher::launcher_window_id()
            || Some(win.id()) == super::osd::osd_window_id()
            || Some(win.id()) == super::notify::toast_window_id()
            || Some(win.id()) == super::notify::center_window_id()
            || Some(win.id()) == super::tray::menu_window_id()
            || Some(win.id()) == super::switcher::alt_tab_window_id()
            || Some(win.id()) == super::switcher::overview_window_id()
            || Some(win.id()) == super::desktop::menu_window_id()
            || super::calc::is_calc_window(win.id())
            || super::settings::is_settings_window(win.id())
            || super::files::is_files_window(win.id())
            || super::tasks::is_tasks_window(win.id())
            || super::editor::is_editor_window(win.id())
            || super::apps::is_apps_window(win.id())
            || super::browse::is_browse_window(win.id())
            || super::chat::is_chat_window(win.id())
            || super::term::is_term_window(win.id())
            || super::dialogs::is_wizard_window(win.id())
            || super::dialogs::is_picker_window(win.id())
            || super::dialogs::is_shot_window(win.id())
            || super::dialogs::is_install_window(win.id())
            || super::session::is_login_window(win.id())
            || super::session::is_lock_window(win.id())
            || super::shortcuts::is_shortcuts_window(win.id())
        {
            continue;
        }
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

    if painted {
        emit_once(&PUMP_GATE, GATE_GPUI_PUMP);
        emit_once(&INTERACT_GATE, GATE_GPUI_INTERACT);
        emit_once(&MOTION_GATE, GATE_GPUI_MOTION);
        emit_once(&KIT_GALLERY_GATE, GATE_GPUI_KIT_GALLERY);
        emit_once(&CHROME_GATE, GATE_GPUI_CHROME);
        super::panel::emit_gate();
        super::desktop::emit_gate();
        crate::gui::hot_corners::emit_gate();
        super::launcher::emit_gate();
        super::osd::emit_gate();
        super::notify::emit_gate();
        super::switcher::emit_gate();
        super::calc::emit_gate();
        super::session::emit_gate();
    }
    emit_once(&RASTER_GATE, GATE_GPUI_SCENE_RASTER);

    if !enter_done || gallery_minimizing() {
        let rest = proof_dest_rect(screen_w, screen_h);
        let prev = LAST_Y_OFFSET.swap(y_offset, Ordering::SeqCst);
        let a = Rect::new(rest.x, rest.y + prev, rest.width, rest.height);
        let b = Rect::new(rest.x, rest.y + y_offset, rest.width, rest.height);
        drop(slot);
        crate::gui::request_window_redraw(crate::gui::rect_union(a, b));
    }
}

/// Map kernel `ThemeId` / settings persist onto the GPUI App global (D7.2).
pub fn sync_theme() {
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return;
    };
    app.set_global(theme_from_kernel());
    if let Some(id) = super::proof::proof_window_id() {
        if let Some(win) = app.window_mut(id) {
            win.request_layout();
        }
    }
    drop(slot);
    super::pointer::damage_proof();
}

fn now_ms() -> u64 {
    dispatcher::now_ticks().saturating_mul(10)
}

fn sample_enter(theme: &Theme, reduced: bool) -> (f32, i32, bool) {
    let start = ENTER_START_MS.load(Ordering::SeqCst);
    if start == 0 {
        return (1.0, 0, true);
    }
    let elapsed = Duration::from_millis(now_ms().saturating_sub(start));
    enter_opacity_y(elapsed, &theme.motion, reduced)
}

fn sample_exit(theme: &Theme, reduced: bool) -> (f32, i32, bool) {
    let start = EXIT_START_MS.load(Ordering::SeqCst);
    if start == 0 {
        return (1.0, 0, false);
    }
    let elapsed = Duration::from_millis(now_ms().saturating_sub(start));
    exit_opacity_y(elapsed, &theme.motion, reduced)
}

fn ensure_proof_window(app: &mut knox_gpui::App, screen_w: u32, screen_h: u32) -> WindowId {
    let dest = proof_dest_rect(screen_w, screen_h);
    let bounds = Bounds::from_xywh(dest.x, dest.y, dest.width as i32, dest.height as i32);
    let raw = PROOF_WINDOW.load(Ordering::SeqCst);
    if raw == 0 {
        let id = app.open_window(bounds);
        if let Some(win) = app.window_mut(id) {
            win.set_title("Kit Gallery");
            win.set_min_size(Size::wh(dest.width as i32, dest.height as i32));
            win.set_chrome(ChromeKind::Csd);
        }
        PROOF_WINDOW.store(id.as_u64(), Ordering::SeqCst);
        ENTER_START_MS.store(now_ms().max(1), Ordering::SeqCst);
        LAST_Y_OFFSET.store(
            theme_from_kernel().motion.distance_medium.get(),
            Ordering::SeqCst,
        );
        id
    } else {
        let id = WindowId::from_u64(raw);
        if let Some(win) = app.window_mut(id) {
            win.set_origin(Point::xy(dest.x, dest.y));
        }
        id
    }
}

fn emit_once(flag: &AtomicBool, marker: &str) {
    if flag.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::serial_println!("[gpui] {}", marker);
}
