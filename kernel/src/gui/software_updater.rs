use alloc::format;
/// Software Updater — GUI for checking and applying package updates
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use super::window::{self, WindowContentType, WindowId};
use crate::package_manager::{self, PKG_MANAGER, PackageState, Version};

struct UpdateEntry {
    name: String,
    description: String,
    current_ver: String,
    new_ver: String,
    size_bytes: u64,
    selected: bool,
}

struct UpdaterState {
    window_id: WindowId,
    entries: Vec<UpdateEntry>,
    checking: bool,
    applying: bool,
    progress: u8,
    status_msg: String,
}

lazy_static! {
    static ref STATES: Mutex<Vec<UpdaterState>> = Mutex::new(Vec::new());
}

/// Open the software updater window
pub fn open() {
    let mut win = window::Window::new("Software Updater", 180, 100, 560, 440);
    win.content_type = WindowContentType::SoftwareUpdater;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    // Gather updates
    let mgr = PKG_MANAGER.lock();
    let updates = mgr.check_updates();
    let mut entries = Vec::new();
    for (name, cur, new) in &updates {
        let desc = mgr
            .get_info(name)
            .map(|p| p.description.clone())
            .unwrap_or_default();
        let size = mgr.get_info(name).map(|p| p.size_bytes).unwrap_or(0);
        entries.push(UpdateEntry {
            name: name.clone(),
            description: desc,
            current_ver: format!("{}", cur),
            new_ver: format!("{}", new),
            size_bytes: size,
            selected: true,
        });
    }
    drop(mgr);

    // Also list installed packages for display
    let installed_count = {
        let mgr = PKG_MANAGER.lock();
        mgr.list_installed().len()
    };

    let status = if entries.is_empty() {
        String::from("System is up to date")
    } else {
        format!("{} update(s) available", entries.len())
    };

    STATES.lock().push(UpdaterState {
        window_id: wid,
        entries,
        checking: false,
        applying: false,
        progress: 0,
        status_msg: status,
    });

    super::request_redraw();
}

/// Draw updater content
pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, area: Rect, scroll_y: i32) {
    let tc = theme::colors();
    let accent = colors::accent();
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let x0 = area.x;
    let y0 = area.y;
    let w = area.width as i32;

    // -- Header --
    fb.fill_rect(
        Rect::new(x0, y0, area.width, 50),
        tc.bg_surface.with_alpha(230),
    );

    // Shield icon
    fb.fill_rounded_rect_aa(Rect::new(x0 + 12, y0 + 8, 20, 24), accent, 4);
    fb.fill_rect(Rect::new(x0 + 18, y0 + 14, 8, 2), Pixel::rgb(255, 255, 255));
    fb.fill_rect(Rect::new(x0 + 21, y0 + 11, 2, 8), Pixel::rgb(255, 255, 255));

    fonts::draw_string_bold_compact(fb, x0 + 40, y0 + 8, "Software Updater", tc.text_primary, 1);
    fonts::draw_string_compact(
        fb,
        x0 + 40,
        y0 + 24,
        &state.status_msg,
        tc.text_secondary,
        1,
    );

    // Check for Updates button
    let check_x = x0 + w - 130;
    fb.fill_rounded_rect_aa(Rect::new(check_x, y0 + 10, 60, 16), tc.bg_tertiary, 4);
    fonts::draw_string_compact(fb, check_x + 6, y0 + 13, "Check", tc.text_primary, 1);

    // Apply Updates button
    let apply_x = x0 + w - 64;
    let has_selected = state.entries.iter().any(|e| e.selected);
    let apply_bg = if has_selected { accent } else { tc.bg_tertiary };
    let apply_fg = if has_selected {
        Pixel::rgb(255, 255, 255)
    } else {
        tc.text_secondary
    };
    fb.fill_rounded_rect_aa(Rect::new(apply_x, y0 + 10, 54, 16), apply_bg, 4);
    fonts::draw_string_compact(fb, apply_x + 6, y0 + 13, "Apply", apply_fg, 1);

    // -- Progress bar (if applying) --
    if state.applying {
        let bar_y = y0 + 38;
        fb.fill_rect(
            Rect::new(x0 + 10, bar_y, (w - 20) as u32, 6),
            tc.bg_tertiary,
        );
        let fill_w = ((w - 20) as u32 * state.progress as u32) / 100;
        fb.fill_rect(Rect::new(x0 + 10, bar_y, fill_w, 6), accent);
    }

    // -- Package list --
    let list_y = y0 + 54;
    let list_h = area.height as i32 - 54;
    let row_h = 40i32;

    if state.entries.is_empty() {
        // No updates message
        let msg = "All packages are up to date.";
        let mw = fonts::measure_string_width_compact(msg, 1) as i32;
        fonts::draw_string_compact(
            fb,
            x0 + (w - mw) / 2,
            list_y + 40,
            msg,
            tc.text_secondary,
            1,
        );

        // Show installed count
        let mgr = PKG_MANAGER.lock();
        let count = mgr.list_installed().len();
        drop(mgr);
        let count_msg = format!("{} packages installed", count);
        let cw = fonts::measure_string_width_compact(&count_msg, 1) as i32;
        fonts::draw_string_compact(
            fb,
            x0 + (w - cw) / 2,
            list_y + 60,
            &count_msg,
            tc.text_secondary,
            1,
        );
        return;
    }

    for (i, entry) in state.entries.iter().enumerate() {
        let ry = list_y + (i as i32) * row_h + scroll_y;
        if ry + row_h < list_y || ry > list_y + list_h {
            continue;
        }

        // Row background
        if i % 2 == 0 {
            fb.fill_rect(
                Rect::new(x0, ry, area.width, row_h as u32),
                tc.bg_tertiary.with_alpha(50),
            );
        }

        // Checkbox
        let cb_x = x0 + 10;
        let cb_y = ry + 12;
        fb.draw_rounded_rect(Rect::new(cb_x, cb_y, 14, 14), tc.text_secondary, 2, 1);
        if entry.selected {
            fb.fill_rounded_rect_aa(Rect::new(cb_x + 2, cb_y + 2, 10, 10), accent, 2);
        }

        // Package name + version info
        fonts::draw_string_bold_compact(fb, x0 + 32, ry + 4, &entry.name, tc.text_primary, 1);

        let ver_str = format!("{} → {}", entry.current_ver, entry.new_ver);
        fonts::draw_string_compact(fb, x0 + 32, ry + 18, &ver_str, accent, 1);

        // Description
        if !entry.description.is_empty() {
            let desc_x = x0 + 200;
            fonts::draw_string_compact(
                fb,
                desc_x,
                ry + 10,
                &entry.description,
                tc.text_secondary,
                1,
            );
        }

        // Size
        if entry.size_bytes > 0 {
            let sz = format_size(entry.size_bytes);
            let sw = fonts::measure_string_width_compact(&sz, 1) as i32;
            fonts::draw_string_compact(fb, x0 + w - sw - 10, ry + 10, &sz, tc.text_secondary, 1);
        }
    }
}

/// Handle click events
pub fn handle_click(wid: WindowId, x: i32, y: i32) {
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return,
    };
    let area = win.content_rect();
    drop(wm);

    let x0 = area.x;
    let y0 = area.y;
    let w = area.width as i32;

    // Check button
    let check_x = x0 + w - 130;
    if y >= y0 + 10 && y < y0 + 26 && x >= check_x && x < check_x + 60 {
        // Re-check updates
        let mgr = PKG_MANAGER.lock();
        let updates = mgr.check_updates();
        state.entries.clear();
        for (name, cur, new) in &updates {
            let desc = mgr
                .get_info(name)
                .map(|p| p.description.clone())
                .unwrap_or_default();
            let size = mgr.get_info(name).map(|p| p.size_bytes).unwrap_or(0);
            state.entries.push(UpdateEntry {
                name: name.clone(),
                description: desc,
                current_ver: format!("{}", cur),
                new_ver: format!("{}", new),
                size_bytes: size,
                selected: true,
            });
        }
        drop(mgr);
        state.status_msg = if state.entries.is_empty() {
            String::from("System is up to date")
        } else {
            format!("{} update(s) available", state.entries.len())
        };
        drop(states);
        super::request_redraw();
        return;
    }

    // Apply button
    let apply_x = x0 + w - 64;
    if y >= y0 + 10 && y < y0 + 26 && x >= apply_x && x < apply_x + 54 {
        let selected: Vec<String> = state
            .entries
            .iter()
            .filter(|e| e.selected)
            .map(|e| e.name.clone())
            .collect();
        if !selected.is_empty() {
            let mut mgr = PKG_MANAGER.lock();
            for name in &selected {
                let _ = mgr.upgrade(name);
            }
            drop(mgr);
            state.entries.retain(|e| !e.selected);
            state.status_msg = String::from("Updates applied successfully");
        }
        drop(states);
        super::request_redraw();
        return;
    }

    // Checkbox clicks in list
    let list_y = y0 + 54;
    let row_h = 40i32;
    let wm = window::WINDOW_MANAGER.lock();
    let scroll_y = wm
        .windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.scroll_y)
        .unwrap_or(0);
    drop(wm);

    for (i, entry) in state.entries.iter_mut().enumerate() {
        let ry = list_y + (i as i32) * row_h + scroll_y;
        let cb_x = x0 + 10;
        let cb_y = ry + 12;
        if x >= cb_x && x < cb_x + 14 && y >= cb_y && y < cb_y + 14 {
            entry.selected = !entry.selected;
            drop(states);
            super::request_redraw();
            return;
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
