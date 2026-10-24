// SPDX-License-Identifier: MIT
//! D11.2 Settings as a Knox GPUI view. Replaces `gui/settings/*.rs` paint.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::settings::{self, SettingsTab};
use crate::gui::window::{self, ViewKind, WindowContentType, WindowId as WmId, WindowState};
use knox_gpui::{
    Bounds, ChromeKind, IntoElement, MouseButton, PaintEnv, Point, Scene, Size, SpriteAtlas,
    WindowId, WindowInput, dispatch_mouse_down_button, dispatch_mouse_up,
    dispatch_scroll as dispatch_scroll_element, paint_element_with_input, prepare_element, px,
};
use knox_ui::{
    SettingGroup, SettingItem, SettingPage, SettingsWidget, Theme, TitleBar, WindowBorder,
};
use spin::Mutex;

use super::platform::APP;
use super::proof::app_theme;

static SETTINGS_GATE: AtomicBool = AtomicBool::new(false);
static VIEWS: Mutex<BTreeMap<WmId, u64>> = Mutex::new(BTreeMap::new());

struct Snap {
    wm_id: WmId,
    rect: Rect,
    maximized: bool,
    scroll: i32,
}

pub fn is_live() -> bool {
    super::is_ready()
}

pub fn gpui_id_for(wm: WmId) -> Option<WindowId> {
    let raw = *VIEWS.lock().get(&wm)?;
    Some(WindowId::from_u64(raw))
}

pub fn wm_id_for(gpui: WindowId) -> Option<WmId> {
    let raw = gpui.as_u64();
    VIEWS
        .lock()
        .iter()
        .find(|(_, id)| **id == raw)
        .map(|(wm, _)| *wm)
}

pub fn is_settings_window(id: WindowId) -> bool {
    wm_id_for(id).is_some()
}

fn visible_snaps() -> Vec<Snap> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Settings) && w.is_visible())
        .map(|w| Snap {
            wm_id: w.id,
            rect: w.rect,
            maximized: w.state == WindowState::Maximized,
            scroll: w.scroll_y,
        })
        .collect()
}

fn known_ids() -> Vec<WmId> {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.content_type == WindowContentType::Gpui(ViewKind::Settings))
        .map(|w| w.id)
        .collect()
}

pub fn ensure_settings_windows(app: &mut knox_gpui::App) -> Vec<WindowId> {
    let snaps = visible_snaps();
    let known = known_ids();
    let mut ids = Vec::new();
    let mut map = VIEWS.lock();
    for snap in &snaps {
        let bounds = Bounds::from_xywh(
            snap.rect.x,
            snap.rect.y,
            snap.rect.width as i32,
            snap.rect.height as i32,
        );
        let created = !map.contains_key(&snap.wm_id);
        let gpui = if let Some(&raw) = map.get(&snap.wm_id) {
            WindowId::from_u64(raw)
        } else {
            let id = app.open_window(bounds);
            map.insert(snap.wm_id, id.as_u64());
            id
        };
        if let Some(win) = app.window_mut(gpui) {
            win.set_title("Settings");
            win.set_chrome(ChromeKind::Csd);
            win.set_min_size(Size::wh(640, 420));
            win.set_bounds(bounds);
            if created {
                win.request_layout();
            }
        }
        ids.push(gpui);
    }
    let stale: Vec<WmId> = map
        .keys()
        .copied()
        .filter(|id| !known.contains(id))
        .collect();
    for wm_id in stale {
        if let Some(raw) = map.remove(&wm_id) {
            app.close_window(WindowId::from_u64(raw));
            crate::gui::backing_store::STORE_MANAGER
                .lock()
                .remove(wm_id);
        }
    }
    ids
}

fn selected_tab() -> usize {
    let tab = settings::SETTINGS_STATE.lock().active_tab;
    settings::tab_index(tab)
}

fn set_tab(index: usize) {
    settings::SETTINGS_STATE.lock().active_tab = settings::tab_at(index);
}

fn build_pages() -> Vec<SettingPage> {
    // Paint runs inside `draw_desktop` while FRAMEBUFFER is held. `screen_size()`
    // takes that same spin lock and deadlocks the first Settings frame.
    let (sw, sh) = crate::gui::cached_screen_size();
    let (sw, sh) = (sw as usize, sh as usize);
    let mut display = SettingPage::new("Display").group({
        let mut g = SettingGroup::new("Resolution");
        for &(rw, rh, label) in crate::gui::RESOLUTIONS {
            g = g.item(
                SettingItem::new(label)
                    .hint(format!("{rw}×{rh}"))
                    .selected(rw == sw && rh == sh),
            );
        }
        g
    });
    let brightness = crate::gui::popups::QUICK_SETTINGS.lock().brightness;
    display = display.group(
        SettingGroup::new("Brightness").item(
            SettingItem::new("Brightness")
                .hint("Click to raise by 10%")
                .value(format!("{brightness}%")),
        ),
    );

    let vol = crate::gui::system_tray::get_volume();
    let muted = crate::gui::system_tray::is_muted();
    let sound = SettingPage::new("Sound")
        .group(
            SettingGroup::new("Output")
                .item(
                    SettingItem::new("Master volume")
                        .hint("Click to raise by 10%")
                        .value(format!("{vol}%")),
                )
                .item(SettingItem::new("Mute").toggle(muted)),
        )
        .group(
            SettingGroup::new("Device")
                .item(
                    SettingItem::new("HD Audio Output")
                        .hint("Default")
                        .selected(true),
                )
                .item(SettingItem::new("HDMI Audio").hint("Digital"))
                .item(SettingItem::new("USB Headset").hint("External")),
        );

    let wifi = crate::gui::system_tray::is_wifi_enabled();
    let network = SettingPage::new("Network").group(
        SettingGroup::new("Wi-Fi")
            .item(SettingItem::new("Wi-Fi").hint("Wireless LAN").toggle(wifi))
            .item(
                SettingItem::new("KnoxOS-Net")
                    .hint("Connected")
                    .selected(true),
            )
            .item(SettingItem::new("Office-5G").hint("Secured"))
            .item(SettingItem::new("Guest").hint("Open")),
    );

    let theme_id = crate::gui::theme::active_theme();
    let accent = crate::gui::theme::accent_index();
    let cursor = crate::gui::desktop::cursor_theme();
    let personalization = SettingPage::new("Personalization")
        .group(
            SettingGroup::new("Theme")
                .item(
                    SettingItem::new("Nebula Dark")
                        .selected(theme_id == crate::gui::theme::ThemeId::NebulaDark),
                )
                .item(
                    SettingItem::new("Arctic Light")
                        .selected(theme_id == crate::gui::theme::ThemeId::ArcticLight),
                )
                .item(
                    SettingItem::new("Sunset Warm")
                        .selected(theme_id == crate::gui::theme::ThemeId::SunsetWarm),
                ),
        )
        .group({
            let mut g = SettingGroup::new("Accent");
            for i in 0..6u8 {
                g = g.item(SettingItem::new(format!("Accent {}", i + 1)).selected(accent == i));
            }
            g
        })
        .group(
            SettingGroup::new("Cursor")
                .item(
                    SettingItem::new("White")
                        .selected(cursor == crate::gui::desktop::CursorTheme::Default),
                )
                .item(
                    SettingItem::new("Dark")
                        .selected(cursor == crate::gui::desktop::CursorTheme::Dark),
                )
                .item(
                    SettingItem::new("Accent")
                        .selected(cursor == crate::gui::desktop::CursorTheme::Accent),
                )
                .item(
                    SettingItem::new("Large")
                        .selected(cursor == crate::gui::desktop::CursorTheme::Large),
                ),
        );

    let user_name = crate::users::get_user(crate::users::get_current_uid())
        .map(|u| u.username.clone())
        .unwrap_or_else(|| String::from("user"));
    let users = SettingPage::new("Users").group(
        SettingGroup::new("Session")
            .item(SettingItem::new("Signed in").value(user_name))
            .item(SettingItem::new("Lock screen").hint("Require password"))
            .item(SettingItem::new("Log out").hint("Return to login")),
    );

    let layout = crate::task::keyboard::get_layout();
    let layouts = [
        crate::task::keyboard::KeyboardLayout::Us,
        crate::task::keyboard::KeyboardLayout::Uk,
        crate::task::keyboard::KeyboardLayout::De,
        crate::task::keyboard::KeyboardLayout::Fr,
        crate::task::keyboard::KeyboardLayout::Es,
        crate::task::keyboard::KeyboardLayout::Dvorak,
    ];
    let mut system = SettingPage::new("System").group(
        SettingGroup::new("About this PC")
            .item(SettingItem::new("OS").value("KnoxOS v0.2.6"))
            .item(SettingItem::new("Kernel").value("Microkernel (Rust)")),
    );
    let mut kb = SettingGroup::new("Keyboard");
    for l in layouts {
        kb = kb.item(SettingItem::new(l.name()).selected(layout == l));
    }
    system = system.group(kb);

    let (ntp, h24, seconds) = crate::gui::settings_ext::get_datetime();
    let tz_name = crate::gui::settings_ext::current_timezone_name();
    let mut datetime = SettingPage::new("Date & Time").group(
        SettingGroup::new("Clock")
            .item(SettingItem::new("Set time automatically").toggle(ntp))
            .item(SettingItem::new("24-hour clock").toggle(h24))
            .item(SettingItem::new("Show seconds").toggle(seconds)),
    );
    let mut tz = SettingGroup::new("Timezone");
    for z in crate::gui::settings_ext::common_timezones() {
        tz = tz.item(SettingItem::new(z.name.clone()).selected(z.name == tz_name));
    }
    datetime = datetime.group(tz);

    let privacy_s = crate::gui::settings_ext::PRIVACY.lock();
    let privacy = SettingPage::new("Privacy").group(
        SettingGroup::new("Permissions")
            .item(
                SettingItem::new("Location Services")
                    .hint("Allow apps to determine your location")
                    .toggle(privacy_s.location_services),
            )
            .item(
                SettingItem::new("Analytics")
                    .hint("Anonymous usage statistics")
                    .toggle(privacy_s.analytics_enabled),
            )
            .item(
                SettingItem::new("Camera")
                    .hint("Allow apps to use the camera")
                    .toggle(privacy_s.camera_access),
            )
            .item(
                SettingItem::new("Microphone")
                    .hint("Allow apps to use the microphone")
                    .toggle(privacy_s.microphone_access),
            )
            .item(
                SettingItem::new("Firewall")
                    .hint("Block unauthorized incoming connections")
                    .toggle(privacy_s.firewall_enabled),
            )
            .item(
                SettingItem::new("Automatic Updates")
                    .hint("Keep the system up to date")
                    .toggle(privacy_s.auto_updates_enabled),
            ),
    );
    drop(privacy_s);

    let mut startup = SettingPage::new("Startup Apps");
    let mut apps = SettingGroup::new("Launch at login");
    {
        let st = crate::gui::settings_ext::STARTUP.lock();
        for app in st.apps.iter() {
            apps = apps.item(
                SettingItem::new(app.name.clone())
                    .hint(app.description.clone())
                    .toggle(app.enabled),
            );
        }
    }
    startup = startup.group(apps);

    let about = SettingPage::new("About").group(
        SettingGroup::new("KnoxOS")
            .item(SettingItem::new("Version").value("0.2.6"))
            .item(SettingItem::new("Toolkit").value("Knox GPUI")),
    );

    let a11y = SettingPage::new("Accessibility").group(
        SettingGroup::new("Display")
            .item(
                SettingItem::new("High contrast")
                    .hint("Yellow focus ring, black/white surfaces")
                    .toggle(crate::gui::accessibility::is_high_contrast()),
            )
            .item(
                SettingItem::new("Reduced motion")
                    .hint("Skip enter/exit animation")
                    .toggle(crate::gui::accessibility::is_reduced_motion()),
            ),
    );

    alloc::vec![
        display,
        sound,
        network,
        personalization,
        users,
        system,
        datetime,
        privacy,
        startup,
        about,
        a11y,
    ]
}

fn bump_volume() {
    let v = crate::gui::system_tray::get_volume().saturating_add(10);
    crate::gui::system_tray::set_volume(if v > 100 { 10 } else { v });
    crate::gui::system_tray::set_volume_muted(false);
}

fn bump_brightness() {
    let mut qs = crate::gui::popups::QUICK_SETTINGS.lock();
    let n = qs.brightness.saturating_add(10);
    qs.brightness = if n > 100 { 10 } else { n };
}

fn on_item(page: usize, group: usize, item: usize) {
    match (page, group, item) {
        (0, 0, i) => {
            if let Some(&(rw, rh, _)) = crate::gui::RESOLUTIONS.get(i) {
                let (cw, ch) = crate::gui::cached_screen_size();
                let (cw, ch) = (cw as usize, ch as usize);
                if rw != cw || rh != ch {
                    crate::gui::change_resolution(rw, rh);
                }
            }
        }
        (0, 1, 0) => bump_brightness(),
        (1, 0, 0) => bump_volume(),
        (3, 0, 0) => crate::gui::theme::set_theme(crate::gui::theme::ThemeId::NebulaDark),
        (3, 0, 1) => crate::gui::theme::set_theme(crate::gui::theme::ThemeId::ArcticLight),
        (3, 0, 2) => crate::gui::theme::set_theme(crate::gui::theme::ThemeId::SunsetWarm),
        (3, 1, i) if i < 6 => crate::gui::theme::set_accent_index(i as u8),
        (3, 2, 0) => {
            crate::gui::desktop::set_cursor_theme(crate::gui::desktop::CursorTheme::Default)
        }
        (3, 2, 1) => crate::gui::desktop::set_cursor_theme(crate::gui::desktop::CursorTheme::Dark),
        (3, 2, 2) => {
            crate::gui::desktop::set_cursor_theme(crate::gui::desktop::CursorTheme::Accent)
        }
        (3, 2, 3) => crate::gui::desktop::set_cursor_theme(crate::gui::desktop::CursorTheme::Large),
        (4, 0, 1) => crate::gui::lock_screen::lock(),
        (4, 0, 2) => {
            crate::gui::lock_screen::lock();
            crate::gui::login::reset();
        }
        (5, 1, i) => {
            let layouts = [
                crate::task::keyboard::KeyboardLayout::Us,
                crate::task::keyboard::KeyboardLayout::Uk,
                crate::task::keyboard::KeyboardLayout::De,
                crate::task::keyboard::KeyboardLayout::Fr,
                crate::task::keyboard::KeyboardLayout::Es,
                crate::task::keyboard::KeyboardLayout::Dvorak,
            ];
            if let Some(l) = layouts.get(i) {
                crate::task::keyboard::set_layout(*l);
            }
        }
        (6, 1, i) => {
            let tzs = crate::gui::settings_ext::common_timezones();
            if let Some(tz) = tzs.get(i) {
                crate::gui::settings_ext::set_timezone(&tz.name);
            }
        }
        _ => {}
    }
    crate::gui::request_redraw();
}

fn on_toggle(page: usize, group: usize, item: usize, value: bool) {
    match (page, group, item) {
        (1, 0, 1) => crate::gui::system_tray::set_volume_muted(value),
        (2, 0, 0) => crate::gui::system_tray::set_wifi_enabled(value),
        (6, 0, 0) => {
            let _ = crate::gui::settings_ext::toggle_datetime_flag("ntp");
        }
        (6, 0, 1) => {
            let _ = crate::gui::settings_ext::toggle_datetime_flag("24h");
        }
        (6, 0, 2) => {
            let _ = crate::gui::settings_ext::toggle_datetime_flag("seconds");
        }
        (7, 0, i) => {
            let keys = [
                "location",
                "analytics",
                "camera",
                "microphone",
                "firewall",
                "autoupdate",
            ];
            if let Some(k) = keys.get(i) {
                let _ = crate::gui::settings_ext::toggle_privacy(k);
            }
        }
        (8, 0, i) => {
            let name = {
                let st = crate::gui::settings_ext::STARTUP.lock();
                st.apps.get(i).map(|a| a.name.clone())
            };
            if let Some(name) = name {
                let _ = crate::gui::settings_ext::toggle_startup_app(&name);
            }
        }
        (10, 0, 0) => {
            if crate::gui::accessibility::is_high_contrast() != value {
                crate::gui::accessibility::toggle_high_contrast();
            }
            super::pump::sync_theme();
        }
        (10, 0, 1) => {
            if crate::gui::accessibility::is_reduced_motion() != value {
                crate::gui::accessibility::toggle_reduced_motion();
            }
            super::pump::sync_theme();
        }
        _ => {}
    }
    crate::gui::request_redraw();
}

fn settings_tree(
    theme: &Theme,
    wm_id: WmId,
    width: i32,
    height: i32,
    maximized: bool,
    scroll: i32,
) -> knox_gpui::Div {
    let pages = build_pages();
    let selected = selected_tab();
    let titlebar = TitleBar::new("Settings")
        .themed(theme)
        .maximized(maximized)
        .on_drag(move |_| start_drag(wm_id))
        .on_min(move || minimize(wm_id))
        .on_max(move || maximize(wm_id))
        .on_close(move || close(wm_id));
    WindowBorder::new("Settings")
        .themed(theme)
        .size(px(width), px(height))
        .titlebar(titlebar)
        .body(
            SettingsWidget::new()
                .themed(theme)
                .size(px(width), px((height - 32).max(1)))
                .scroll(scroll)
                .selected(selected)
                .pages(pages)
                .on_page(|i| {
                    set_tab(i);
                    crate::gui::request_redraw();
                })
                .on_item(on_item)
                .on_toggle(on_toggle)
                .on_scroll(move |off| set_scroll(wm_id, off)),
        )
        .into_element()
}

fn set_scroll(wid: WmId, offset: i32) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.scroll_y = offset.max(0);
    }
}

fn start_drag(wid: WmId) {
    let mouse = crate::gui::input::MOUSE.lock();
    let (x, y) = (mouse.x, mouse.y);
    drop(mouse);
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.state == WindowState::Maximized {
            win.state = WindowState::Normal;
            win.rect = win.saved_rect;
            win.rect.x = x - win.rect.width as i32 / 2;
            win.rect.y = y.max(0);
        }
        win.dragging = true;
        win.drag_offset_x = x - win.rect.x;
        win.drag_offset_y = y - win.rect.y;
    }
}

fn close(wid: WmId) {
    let old = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.rect)
            .unwrap_or_else(|| Rect::new(0, 0, 1, 1))
    };
    window::WINDOW_MANAGER.lock().close_window(wid);
    crate::gui::taskbar::remove_entry(wid);
    crate::gui::request_window_redraw(old);
}

fn minimize(wid: WmId) {
    let old = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.rect)
            .unwrap_or_else(|| Rect::new(0, 0, 1, 1))
    };
    window::WINDOW_MANAGER.lock().minimize_window(wid);
    crate::gui::request_window_redraw(old);
}

fn maximize(wid: WmId) {
    let (sw, sh) = crate::gui::cached_screen_size();
    let old = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.rect)
            .unwrap_or_else(|| Rect::new(0, 0, 1, 1))
    };
    crate::gui::request_window_redraw(old);
    window::WINDOW_MANAGER
        .lock()
        .toggle_maximize(wid, sw as u32, sh as u32);
}

pub fn paint_settings_scene(
    input: &mut WindowInput,
    atlas: &mut SpriteAtlas,
    theme: &Theme,
    width: i32,
    height: i32,
    wm_id: WmId,
) -> Scene {
    let (maximized, scroll) = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wm_id)
            .map(|w| (w.state == WindowState::Maximized, w.scroll_y))
            .unwrap_or((false, 0))
    };
    let mut scene = Scene::new();
    let sys = super::text_system::KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas,
        text: Some(&sys),
    };
    let mut root = settings_tree(theme, wm_id, width, height, maximized, scroll);
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

pub fn present_settings(fb: &mut FrameBuffer, wid: WmId, area: Rect) {
    let _ = crate::gui::backing_store::STORE_MANAGER
        .lock()
        .blit(wid, fb, area.x, area.y);
}

fn hit_visible(x: i32, y: i32) -> Option<(WmId, WindowId)> {
    let wm = window::WINDOW_MANAGER.lock();
    let wid = wm.window_at(x, y)?;
    let ok = wm
        .windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.content_type == WindowContentType::Gpui(ViewKind::Settings) && w.is_visible())
        .unwrap_or(false);
    drop(wm);
    if !ok {
        return None;
    }
    Some((wid, gpui_id_for(wid)?))
}

fn win_meta(wm_id: WmId) -> (bool, i32) {
    let wm = window::WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .find(|w| w.id == wm_id)
        .map(|w| (w.state == WindowState::Maximized, w.scroll_y))
        .unwrap_or((false, 0))
}

pub fn dispatch_pointer_down(x: i32, y: i32, button: MouseButton) -> bool {
    let Some((wm_id, id)) = hit_visible(x, y) else {
        return false;
    };
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    let local = Point::xy(x - bounds.x(), y - bounds.y());
    let size = Size::wh(bounds.width(), bounds.height());
    let (maximized, scroll) = win_meta(wm_id);
    {
        let input = win.input_mut();
        let mut root = settings_tree(
            &theme,
            wm_id,
            size.width.0,
            size.height.0,
            maximized,
            scroll,
        );
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_mouse_down_button(&mut root, input, local, button);
    }
    win.request_layout();
    true
}

pub fn dispatch_pointer_up(x: i32, y: i32) -> bool {
    let map: Vec<(WmId, WindowId)> = VIEWS
        .lock()
        .iter()
        .map(|(wm, raw)| (*wm, WindowId::from_u64(*raw)))
        .collect();
    if map.is_empty() {
        return false;
    }
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let theme = app_theme(app);
    let mut handled = false;
    for (wm_id, id) in map {
        let Some(win) = app.window_mut(id) else {
            continue;
        };
        if !win.input().is_mouse_down() && win.input().mouse_down_hitbox().is_none() {
            continue;
        }
        let bounds = win.bounds();
        let local = Point::xy(x - bounds.x(), y - bounds.y());
        let size = Size::wh(bounds.width(), bounds.height());
        let (maximized, scroll) = win_meta(wm_id);
        {
            let input = win.input_mut();
            let mut root = settings_tree(
                &theme,
                wm_id,
                size.width.0,
                size.height.0,
                maximized,
                scroll,
            );
            prepare_element(&mut root, Point::ZERO, size, input);
            let _ = dispatch_mouse_up(&mut root, input, local);
        }
        win.request_layout();
        handled = true;
    }
    handled
}

pub fn pointer_moved(x: i32, y: i32) -> bool {
    let Some((_wm_id, id)) = hit_visible(x, y) else {
        return false;
    };
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let local = Point::xy(x - win.bounds().x(), y - win.bounds().y());
    win.input_mut().set_pointer_position(local);
    // Hover must not relayout: Settings raster on TCG starves the tablet.
    false
}

pub fn dispatch_scroll(x: i32, y: i32, delta_y: i32) -> bool {
    let Some((wm_id, id)) = hit_visible(x, y) else {
        return false;
    };
    let mut slot = APP.lock();
    let Some(app) = slot.as_mut() else {
        return false;
    };
    let theme = app_theme(app);
    let Some(win) = app.window_mut(id) else {
        return false;
    };
    let bounds = win.bounds();
    let local = Point::xy(x - bounds.x(), y - bounds.y());
    let size = Size::wh(bounds.width(), bounds.height());
    let (maximized, scroll) = win_meta(wm_id);
    {
        let input = win.input_mut();
        input.set_pointer_position(local);
        let mut root = settings_tree(
            &theme,
            wm_id,
            size.width.0,
            size.height.0,
            maximized,
            scroll,
        );
        prepare_element(&mut root, Point::ZERO, size, input);
        dispatch_scroll_element(&mut root, input, Point::xy(0, delta_y));
    }
    win.request_layout();
    true
}

pub fn emit_gate() {
    if SETTINGS_GATE.swap(true, Ordering::SeqCst) {
        return;
    }
}
