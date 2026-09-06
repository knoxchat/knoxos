use alloc::format;
/// Disk Utility — GUI for viewing filesystems, mounts, and disk usage
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

/// Info about a mounted filesystem for display
#[derive(Clone)]
struct FsEntry {
    mount_point: String,
    source: String,
    fs_type: String,
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
}

struct DiskUtilState {
    window_id: WindowId,
    entries: Vec<FsEntry>,
    selected: i32,
}

lazy_static! {
    static ref STATES: Mutex<Vec<DiskUtilState>> = Mutex::new(Vec::new());
}

const ROW_HEIGHT: i32 = 28;
const HEADER_HEIGHT: i32 = 48;

// ─── Public API ─────────────────────────────────────────────────────

pub fn open() {
    let entries = gather_filesystem_info();

    let mut win = window::Window::new("Disk Utility", 180, 80, 700, 480);
    win.content_type = WindowContentType::DiskUtility;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    let mut states = STATES.lock();
    states.push(DiskUtilState {
        window_id: wid,
        entries,
        selected: -1,
    });
    drop(states);

    super::request_redraw();
}

pub fn on_window_closed(wid: WindowId) {
    let mut states = STATES.lock();
    states.retain(|s| s.window_id != wid);
}

// ─── Drawing ────────────────────────────────────────────────────────

pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, content: Rect, scroll_y: i32) {
    let states = STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let tc = theme::colors();
    let bg = tc.bg_surface;
    fb.fill_rect(content, bg);

    // Header
    let header = Rect::new(content.x, content.y, content.width, HEADER_HEIGHT as u32);
    fb.fill_rect(header, colors::darken(bg, 15));
    fonts::draw_string_bold_compact(
        fb,
        content.x + 16,
        content.y + 8,
        "Mounted Filesystems",
        tc.text_primary,
        1,
    );

    let mut summary = String::new();
    let _ = write!(summary, "{} volumes", state.entries.len());
    fonts::draw_string_compact(
        fb,
        content.x + 16,
        content.y + 26,
        &summary,
        tc.text_secondary,
        1,
    );

    // Refresh button
    let btn_w = 70u32;
    let btn_x = content.x + content.width as i32 - btn_w as i32 - 12;
    let btn_y = content.y + 12;
    let btn_rect = Rect::new(btn_x, btn_y, btn_w, 24);
    fb.fill_rounded_rect_aa(btn_rect, theme::accent_color(), 4);
    fonts::draw_string_centered_compact(
        fb,
        btn_x,
        btn_y,
        btn_w,
        24,
        "Refresh",
        Pixel::rgb(255, 255, 255),
        1,
    );

    // Column headers
    let col_y = content.y + HEADER_HEIGHT;
    let col_rect = Rect::new(content.x, col_y, content.width, 22);
    fb.fill_rect(col_rect, colors::darken(bg, 8));

    let col_x = content.x + 12;
    fonts::draw_string_compact(fb, col_x, col_y + 4, "Mount", tc.text_secondary, 1);
    fonts::draw_string_compact(fb, col_x + 160, col_y + 4, "Type", tc.text_secondary, 1);
    fonts::draw_string_compact(
        fb,
        col_x + 240,
        col_y + 4,
        "Used / Total",
        tc.text_secondary,
        1,
    );
    fonts::draw_string_compact(fb, col_x + 430, col_y + 4, "Usage", tc.text_secondary, 1);

    // Filesystem rows
    let list_y = col_y + 22;
    let visible_h = content.height as i32 - HEADER_HEIGHT - 22;

    for (i, entry) in state.entries.iter().enumerate() {
        let ey = list_y + (i as i32 * ROW_HEIGHT) - scroll_y;
        if ey + ROW_HEIGHT < list_y || ey > content.y + content.height as i32 {
            continue;
        }

        // Selection / alternating
        if i as i32 == state.selected {
            fb.fill_rect(
                Rect::new(content.x, ey, content.width, ROW_HEIGHT as u32),
                theme::accent_color().with_alpha(50),
            );
        } else if i % 2 == 1 {
            fb.fill_rect(
                Rect::new(content.x, ey, content.width, ROW_HEIGHT as u32),
                colors::lighten(bg, 3),
            );
        }

        // Drive icon
        let icon_x = col_x;
        let icon_y = ey + 5;
        fb.fill_rounded_rect_aa(
            Rect::new(icon_x, icon_y, 14, 10),
            Pixel::rgb(100, 130, 180),
            2,
        );
        fb.fill_rect(
            Rect::new(icon_x + 3, icon_y + 3, 3, 4),
            Pixel::rgb(60, 80, 120),
        );
        fb.fill_rect(
            Rect::new(icon_x + 8, icon_y + 3, 3, 4),
            Pixel::rgb(60, 80, 120),
        );

        // Mount point
        fonts::draw_string_compact(
            fb,
            col_x + 20,
            ey + 7,
            &entry.mount_point,
            tc.text_primary,
            1,
        );

        // FS type
        fonts::draw_string_compact(
            fb,
            col_x + 160,
            ey + 7,
            &entry.fs_type,
            tc.text_secondary,
            1,
        );

        // Size info
        let size_str = format!(
            "{} / {}",
            format_bytes(entry.used_bytes),
            format_bytes(entry.total_bytes)
        );
        fonts::draw_string_compact(fb, col_x + 240, ey + 7, &size_str, tc.text_secondary, 1);

        // Usage bar
        let bar_x = col_x + 430;
        let bar_w = 100u32;
        let bar_h = 8u32;
        let bar_y = ey + 9;
        let bar_rect = Rect::new(bar_x, bar_y, bar_w, bar_h);
        fb.fill_rounded_rect_aa(bar_rect, Pixel::new(255, 255, 255, 20), 4);

        let pct = (entry.used_bytes * 100)
            .checked_div(entry.total_bytes)
            .unwrap_or(0) as u32;
        let fill_w = (bar_w as u64 * pct.min(100) as u64 / 100) as u32;
        if fill_w > 0 {
            let bar_color = if pct > 90 {
                Pixel::rgb(255, 80, 80)
            } else if pct > 75 {
                Pixel::rgb(240, 180, 60)
            } else {
                theme::accent_color()
            };
            fb.fill_rounded_rect_aa(Rect::new(bar_x, bar_y, fill_w, bar_h), bar_color, 4);
        }

        // Percentage text
        let mut pct_str = String::new();
        let _ = write!(pct_str, "{}%", pct);
        fonts::draw_string_compact(
            fb,
            bar_x + bar_w as i32 + 6,
            ey + 7,
            &pct_str,
            tc.text_secondary,
            1,
        );
    }

    // Update max scroll
    let total_h = state.entries.len() as i32 * ROW_HEIGHT;
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.max_scroll_y = (total_h - visible_h).max(0);
    }
}

// ─── Click Handling ────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let content = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w.content_rect(),
        None => return false,
    };
    let scroll_y = wm
        .windows
        .iter()
        .find(|w| w.id == wid)
        .map(|w| w.scroll_y)
        .unwrap_or(0);
    drop(wm);

    // Refresh button
    let btn_w = 70i32;
    let btn_x = content.x + content.width as i32 - btn_w - 12;
    let btn_y = content.y + 12;
    if x >= btn_x && x < btn_x + btn_w && y >= btn_y && y < btn_y + 24 {
        let entries = gather_filesystem_info();
        let mut states = STATES.lock();
        if let Some(s) = states.iter_mut().find(|s| s.window_id == wid) {
            s.entries = entries;
        }
        return true;
    }

    // Row selection
    let list_y = content.y + HEADER_HEIGHT + 22;
    if y >= list_y {
        let row = (y - list_y + scroll_y) / ROW_HEIGHT;
        let mut states = STATES.lock();
        if let Some(s) = states.iter_mut().find(|s| s.window_id == wid) {
            if row >= 0 && (row as usize) < s.entries.len() {
                s.selected = row;
                return true;
            }
        }
    }

    false
}

// ─── Data Gathering ────────────────────────────────────────────────

fn gather_filesystem_info() -> Vec<FsEntry> {
    let mounts = crate::mount::list_mounts();
    let mut entries = Vec::new();

    for m in &mounts {
        // Skip virtual filesystems for cleaner display
        let skip = matches!(
            m.fs_type,
            crate::mount::FilesystemType::Procfs
                | crate::mount::FilesystemType::Sysfs
                | crate::mount::FilesystemType::Devfs
                | crate::mount::FilesystemType::Devpts
                | crate::mount::FilesystemType::Mqueue
                | crate::mount::FilesystemType::Debugfs
                | crate::mount::FilesystemType::Cgroup
                | crate::mount::FilesystemType::Cgroup2
        );
        if skip {
            continue;
        }

        let (total, used, free) = match crate::posix_ext::statvfs(&m.target) {
            Ok(sv) => {
                let total = sv.f_blocks * sv.f_bsize;
                let free = sv.f_bfree * sv.f_bsize;
                let used = total.saturating_sub(free);
                (total, used, free)
            }
            Err(_) => (0, 0, 0),
        };

        entries.push(FsEntry {
            mount_point: m.target.clone(),
            source: m.source.clone(),
            fs_type: String::from(m.fs_type.as_str()),
            total_bytes: total,
            used_bytes: used,
            free_bytes: free,
        });
    }

    entries
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return String::from("0 B");
    }
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes / 1024)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
