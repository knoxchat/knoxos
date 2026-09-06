use alloc::format;
/// Archive Manager — Browse and extract TAR/ZIP archives
/// Displays archive contents in a scrollable list and supports extraction.
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use super::window::{self, WindowContentType, WindowId};

/// An entry displayed in the archive viewer
#[derive(Clone)]
pub struct ArchiveEntry {
    pub path: String,
    pub size: usize,
    pub is_dir: bool,
}

/// State for an open archive viewer
pub struct ArchiveViewState {
    pub window_id: WindowId,
    pub archive_path: String,
    pub entries: Vec<ArchiveEntry>,
    pub selected: i32,
    pub extracted: bool,
}

lazy_static! {
    static ref ARCHIVE_STATES: Mutex<Vec<ArchiveViewState>> = Mutex::new(Vec::new());
}

const ROW_HEIGHT: i32 = 22;
const HEADER_HEIGHT: i32 = 40;

// ─── Public API ─────────────────────────────────────────────────────

/// Open an archive file in the archive viewer
pub fn open_archive(path: &str) {
    let data = match crate::file_manager::read_file(path) {
        Ok(d) => d,
        Err(_) => return,
    };

    let entries = parse_archive(&data, path);

    let title = format!("Archive — {}", path);
    let mut win = window::Window::new(&title, 200, 100, 650, 450);
    win.content_type = WindowContentType::ArchiveViewer;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    let mut states = ARCHIVE_STATES.lock();
    states.push(ArchiveViewState {
        window_id: wid,
        archive_path: String::from(path),
        entries,
        selected: -1,
        extracted: false,
    });
    drop(states);

    super::request_redraw();
}

/// Remove state when the archive window is closed
pub fn on_window_closed(wid: WindowId) {
    let mut states = ARCHIVE_STATES.lock();
    states.retain(|s| s.window_id != wid);
}

// ─── Drawing ────────────────────────────────────────────────────────

/// Draw the archive viewer content inside its window
pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, content: Rect, scroll_y: i32) {
    let states = ARCHIVE_STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let tc = theme::colors();
    let bg = tc.bg_surface;
    fb.fill_rect(content, bg);

    // Header bar with archive name and extract button
    let header_rect = Rect::new(content.x, content.y, content.width, HEADER_HEIGHT as u32);
    fb.fill_rect(header_rect, colors::darken(bg, 15));

    // Archive name
    let name = if let Some(slash) = state.archive_path.rfind('/') {
        &state.archive_path[slash + 1..]
    } else {
        &state.archive_path
    };
    let mut info = String::new();
    let _ = write!(info, "{} — {} items", name, state.entries.len());
    fonts::draw_string_bold_compact(fb, content.x + 12, content.y + 6, &info, tc.text_primary, 1);

    // Extract button
    let btn_w = 80u32;
    let btn_h = 24u32;
    let btn_x = content.x + content.width as i32 - btn_w as i32 - 12;
    let btn_y = content.y + 8;
    let btn_rect = Rect::new(btn_x, btn_y, btn_w, btn_h);
    let btn_label = if state.extracted {
        "Extracted"
    } else {
        "Extract All"
    };
    let btn_bg = if state.extracted {
        Pixel::rgb(40, 120, 40)
    } else {
        theme::accent_color()
    };
    fb.fill_rounded_rect_aa(btn_rect, btn_bg, 4);
    fonts::draw_string_centered_compact(
        fb,
        btn_x,
        btn_y,
        btn_w,
        btn_h,
        btn_label,
        Pixel::rgb(255, 255, 255),
        1,
    );

    // Column headers
    let list_y = content.y + HEADER_HEIGHT;
    let col_header_rect = Rect::new(content.x, list_y, content.width, 20);
    fb.fill_rect(col_header_rect, colors::darken(bg, 8));
    fonts::draw_string_compact(fb, content.x + 32, list_y + 3, "Name", tc.text_secondary, 1);
    fonts::draw_string_compact(
        fb,
        content.x + content.width as i32 - 100,
        list_y + 3,
        "Size",
        tc.text_secondary,
        1,
    );

    // File list
    let list_start_y = list_y + 20;
    let visible_h = content.height as i32 - HEADER_HEIGHT - 20;
    let max_visible = visible_h / ROW_HEIGHT;

    for (i, entry) in state.entries.iter().enumerate() {
        let ey = list_start_y + (i as i32 * ROW_HEIGHT) - scroll_y;
        if ey + ROW_HEIGHT < list_start_y || ey > content.y + content.height as i32 {
            continue;
        }

        // Selection highlight
        if i as i32 == state.selected {
            let sel_rect = Rect::new(content.x, ey, content.width, ROW_HEIGHT as u32);
            fb.fill_rect(sel_rect, theme::accent_color().with_alpha(60));
        } else if i % 2 == 1 {
            // Alternating row background
            let alt_rect = Rect::new(content.x, ey, content.width, ROW_HEIGHT as u32);
            fb.fill_rect(alt_rect, colors::lighten(bg, 3));
        }

        // File/dir icon (simple)
        let icon_x = content.x + 10;
        let icon_y = ey + 3;
        if entry.is_dir {
            // Folder icon — small filled rect
            fb.fill_rounded_rect_aa(
                Rect::new(icon_x, icon_y + 2, 14, 10),
                Pixel::rgb(70, 150, 255),
                2,
            );
            fb.fill_rect(
                Rect::new(icon_x, icon_y + 1, 6, 3),
                Pixel::rgb(70, 150, 255),
            );
        } else {
            // File icon — document shape
            fb.fill_rounded_rect_aa(
                Rect::new(icon_x + 1, icon_y, 12, 14),
                Pixel::rgb(180, 180, 200),
                2,
            );
            fb.fill_rect(
                Rect::new(icon_x + 3, icon_y + 4, 8, 1),
                Pixel::rgb(120, 120, 140),
            );
            fb.fill_rect(
                Rect::new(icon_x + 3, icon_y + 7, 6, 1),
                Pixel::rgb(120, 120, 140),
            );
        }

        // File name (truncate if needed)
        let max_name_w = content.width as i32 - 140;
        let display_name = truncate_name(&entry.path, max_name_w as u32);
        fonts::draw_string_compact(
            fb,
            content.x + 32,
            ey + 4,
            &display_name,
            tc.text_primary,
            1,
        );

        // Size
        if !entry.is_dir {
            let size_str = format_size(entry.size);
            let size_w = fonts::measure_string_width_compact(&size_str, 1);
            fonts::draw_string_compact(
                fb,
                content.x + content.width as i32 - size_w as i32 - 12,
                ey + 4,
                &size_str,
                tc.text_secondary,
                1,
            );
        }
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
    let mut states = ARCHIVE_STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return false,
    };

    // Get content rect
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

    // Extract button hit test
    let btn_w = 80i32;
    let btn_x = content.x + content.width as i32 - btn_w - 12;
    let btn_y = content.y + 8;
    if x >= btn_x && x < btn_x + btn_w && y >= btn_y && y < btn_y + 24 && !state.extracted {
        let path = state.archive_path.clone();
        drop(states);
        extract_archive(&path);
        let mut states = ARCHIVE_STATES.lock();
        if let Some(s) = states.iter_mut().find(|s| s.window_id == wid) {
            s.extracted = true;
        }
        return true;
    }

    // Row selection
    let list_y = content.y + HEADER_HEIGHT + 20;
    if y >= list_y {
        let row = (y - list_y + scroll_y) / ROW_HEIGHT;
        if row >= 0 && (row as usize) < state.entries.len() {
            state.selected = row;
            return true;
        }
    }

    false
}

// ─── Archive Parsing ───────────────────────────────────────────────

fn parse_archive(data: &[u8], path: &str) -> Vec<ArchiveEntry> {
    if path.ends_with(".tar") || path.ends_with(".tar.gz") || path.ends_with(".tgz") {
        // Decompress gzip if needed
        let raw = if path.ends_with(".gz") || path.ends_with(".tgz") {
            decompress_gzip(data).unwrap_or_default()
        } else {
            data.to_vec()
        };
        parse_tar_entries(&raw)
    } else if path.ends_with(".zip") {
        parse_zip_entries(data)
    } else {
        // Try TAR first, then ZIP
        let tar = parse_tar_entries(data);
        if !tar.is_empty() {
            tar
        } else {
            parse_zip_entries(data)
        }
    }
}

fn parse_tar_entries(data: &[u8]) -> Vec<ArchiveEntry> {
    let tar_entries = crate::dpkg::parse_tar(data);
    tar_entries
        .into_iter()
        .map(|e| ArchiveEntry {
            path: e.path,
            size: e.size,
            is_dir: e.entry_type == crate::dpkg::TarEntryType::Directory,
        })
        .collect()
}

fn parse_zip_entries(data: &[u8]) -> Vec<ArchiveEntry> {
    // ZIP end-of-central-directory signature
    const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];

    // Find EOCD
    let mut eocd_pos = None;
    if data.len() >= 22 {
        for i in (0..=(data.len() - 22)).rev() {
            if data[i..i + 4] == EOCD_SIG {
                eocd_pos = Some(i);
                break;
            }
        }
    }

    let eocd_pos = match eocd_pos {
        Some(p) => p,
        None => return Vec::new(),
    };

    // Parse EOCD
    let cd_size = u32::from_le_bytes([
        data[eocd_pos + 12],
        data[eocd_pos + 13],
        data[eocd_pos + 14],
        data[eocd_pos + 15],
    ]) as usize;
    let cd_offset = u32::from_le_bytes([
        data[eocd_pos + 16],
        data[eocd_pos + 17],
        data[eocd_pos + 18],
        data[eocd_pos + 19],
    ]) as usize;

    let mut entries = Vec::new();
    let mut pos = cd_offset;
    let cd_end = cd_offset + cd_size;

    while pos + 46 <= cd_end && pos + 46 <= data.len() {
        // Central directory file header signature
        if data[pos..pos + 4] != [0x50, 0x4B, 0x01, 0x02] {
            break;
        }

        let compressed_size = u32::from_le_bytes([
            data[pos + 20],
            data[pos + 21],
            data[pos + 22],
            data[pos + 23],
        ]) as usize;
        let uncompressed_size = u32::from_le_bytes([
            data[pos + 24],
            data[pos + 25],
            data[pos + 26],
            data[pos + 27],
        ]) as usize;
        let name_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([data[pos + 30], data[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([data[pos + 32], data[pos + 33]]) as usize;

        if pos + 46 + name_len <= data.len() {
            let name = String::from_utf8_lossy(&data[pos + 46..pos + 46 + name_len]).to_string();
            let is_dir = name.ends_with('/') || uncompressed_size == 0 && name_len > 0;

            entries.push(ArchiveEntry {
                path: name,
                size: uncompressed_size,
                is_dir,
            });
        }

        pos += 46 + name_len + extra_len + comment_len;
    }

    entries
}

// ─── Extraction ────────────────────────────────────────────────────

fn extract_archive(path: &str) {
    let data = match crate::file_manager::read_file(path) {
        Ok(d) => d,
        Err(_) => return,
    };

    // Derive output directory from archive name
    let base = if let Some(slash) = path.rfind('/') {
        &path[..slash]
    } else {
        "/home/user"
    };
    let stem = archive_stem(path);
    let out_dir = format!("{}/{}", base, stem);
    crate::vfs::ensure_directory(&out_dir);

    if path.ends_with(".tar") || path.ends_with(".tar.gz") || path.ends_with(".tgz") {
        let raw = if path.ends_with(".gz") || path.ends_with(".tgz") {
            decompress_gzip(&data).unwrap_or_default()
        } else {
            data.to_vec()
        };
        extract_tar(&raw, &out_dir);
    } else if path.ends_with(".zip") {
        extract_zip(&data, &out_dir);
    }

    // Show notification
    super::notifications::NOTIFICATIONS.lock().push(
        "Archive extracted",
        &format!("Extracted to {}", out_dir),
        super::notifications::NotificationIcon::System,
        super::notifications::NotificationUrgency::Normal,
    );
}

fn extract_tar(data: &[u8], out_dir: &str) {
    let entries = crate::dpkg::parse_tar(data);
    for entry in &entries {
        let full_path = format!("{}/{}", out_dir, entry.path);
        match entry.entry_type {
            crate::dpkg::TarEntryType::Directory => {
                crate::vfs::ensure_directory(&full_path);
            }
            crate::dpkg::TarEntryType::RegularFile
                if entry.data_offset + entry.size <= data.len() =>
            {
                let file_data = &data[entry.data_offset..entry.data_offset + entry.size];
                let _ = crate::file_manager::write_file(&full_path, file_data);
            }
            _ => {}
        }
    }
}

fn extract_zip(data: &[u8], out_dir: &str) {
    // Extract stored (uncompressed) entries from ZIP
    const LOCAL_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
    let mut pos = 0;

    while pos + 30 <= data.len() {
        if data[pos..pos + 4] != LOCAL_SIG {
            break;
        }

        let method = u16::from_le_bytes([data[pos + 8], data[pos + 9]]);
        let compressed_size = u32::from_le_bytes([
            data[pos + 18],
            data[pos + 19],
            data[pos + 20],
            data[pos + 21],
        ]) as usize;
        let uncompressed_size = u32::from_le_bytes([
            data[pos + 22],
            data[pos + 23],
            data[pos + 24],
            data[pos + 25],
        ]) as usize;
        let name_len = u16::from_le_bytes([data[pos + 26], data[pos + 27]]) as usize;
        let extra_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;

        let name_start = pos + 30;
        let name_end = name_start + name_len;
        let data_start = name_end + extra_len;

        if name_end > data.len() || data_start > data.len() {
            break;
        }

        let name = String::from_utf8_lossy(&data[name_start..name_end]).to_string();
        let full_path = format!("{}/{}", out_dir, name);

        if name.ends_with('/') {
            crate::vfs::ensure_directory(&full_path);
        } else if method == 0 && data_start + uncompressed_size <= data.len() {
            // Stored (no compression)
            let file_data = &data[data_start..data_start + uncompressed_size];
            let _ = crate::file_manager::write_file(&full_path, file_data);
        } else if method == 8 && data_start + compressed_size <= data.len() {
            // Deflate — use inflate decompressor
            let compressed = &data[data_start..data_start + compressed_size];
            if let Some(decompressed) = crate::gui::image::inflate_decompress_raw(compressed) {
                let _ = crate::file_manager::write_file(&full_path, &decompressed);
            }
        }

        pos = data_start + compressed_size;
    }
}

// ─── Helpers ───────────────────────────────────────────────────────

fn decompress_gzip(data: &[u8]) -> Option<Vec<u8>> {
    // Gzip header: 1F 8B, then method, flags, etc.
    if data.len() < 10 || data[0] != 0x1F || data[1] != 0x8B {
        return None;
    }
    // Skip gzip header (10 bytes minimum, more if FEXTRA/FNAME/FCOMMENT flags set)
    let flags = data[3];
    let mut off = 10;
    if flags & 0x04 != 0 {
        // FEXTRA
        if off + 2 > data.len() {
            return None;
        }
        let xlen = u16::from_le_bytes([data[off], data[off + 1]]) as usize;
        off += 2 + xlen;
    }
    if flags & 0x08 != 0 {
        // FNAME — null-terminated
        while off < data.len() && data[off] != 0 {
            off += 1;
        }
        off += 1;
    }
    if flags & 0x10 != 0 {
        // FCOMMENT — null-terminated
        while off < data.len() && data[off] != 0 {
            off += 1;
        }
        off += 1;
    }
    if flags & 0x02 != 0 {
        // FHCRC
        off += 2;
    }
    if off >= data.len() {
        return None;
    }

    // The rest is raw deflate data (before 8-byte trailer)
    let deflate_data = if data.len() >= off + 8 {
        &data[off..data.len() - 8]
    } else {
        &data[off..]
    };

    crate::gui::image::inflate_decompress_raw(deflate_data)
}

fn archive_stem(path: &str) -> String {
    let name = if let Some(slash) = path.rfind('/') {
        &path[slash + 1..]
    } else {
        path
    };
    // Strip known extensions
    let stem = name
        .strip_suffix(".tar.gz")
        .or_else(|| name.strip_suffix(".tgz"))
        .or_else(|| name.strip_suffix(".tar"))
        .or_else(|| name.strip_suffix(".zip"))
        .unwrap_or(name);
    String::from(stem)
}

fn truncate_name(name: &str, max_w: u32) -> String {
    let w = fonts::measure_string_width_compact(name, 1);
    if w <= max_w {
        return String::from(name);
    }
    let mut trunc = String::from(name);
    while trunc.len() > 3 {
        trunc.pop();
        let w = fonts::measure_string_width_compact(&trunc, 1)
            + fonts::measure_string_width_compact("...", 1);
        if w <= max_w {
            trunc.push_str("...");
            return trunc;
        }
    }
    String::from("...")
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
