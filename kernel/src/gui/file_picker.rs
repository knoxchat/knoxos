/// File Picker Dialog — Modal open/save dialog overlay
///
/// Works as a popup overlay (not a window) that blocks interaction with
/// the underlying window. Triggered programmatically when an app needs
/// to open or save a file. Returns the selected path via a callback.
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::colors;
use super::explorer::{ExplorerFileEntry, read_entries};
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

// ─── File Picker Mode ────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum FilePickerMode {
    Open,
    Save,
    SelectFolder,
}

// ─── File Picker State ──────────────────────────────────────────────

pub struct FilePicker {
    pub visible: bool,
    pub mode: FilePickerMode,
    /// Current directory being browsed
    pub current_path: String,
    /// Directory entries
    pub entries: Vec<ExplorerFileEntry>,
    /// Index of selected entry (-1 = none)
    pub selected_index: i32,
    /// Filename text (for Save mode)
    pub filename: String,
    /// Scroll offset for long lists
    pub scroll_offset: u32,
    /// Navigation history
    pub history: Vec<String>,
    pub history_idx: usize,
    /// Hovered entry index
    pub hovered_index: i32,
    /// Filter extension (e.g. "txt", "rs", empty = all)
    pub filter_ext: String,
    /// Result path (set when user confirms)
    pub result_path: Option<String>,
    /// Whether picker was cancelled
    pub cancelled: bool,
}

lazy_static::lazy_static! {
    pub static ref FILE_PICKER: Mutex<FilePicker> = Mutex::new(FilePicker {
        visible: false,
        mode: FilePickerMode::Open,
        current_path: String::from("/home"),
        entries: Vec::new(),
        selected_index: -1,
        filename: String::new(),
        scroll_offset: 0,
        history: Vec::new(),
        history_idx: 0,
        hovered_index: -1,
        filter_ext: String::new(),
        result_path: None,
        cancelled: false,
    });
}

// ─── Dimensions ─────────────────────────────────────────────────────

const PICKER_WIDTH: u32 = 700;
const PICKER_HEIGHT: u32 = 500;
const HEADER_HEIGHT: u32 = 48;
const PATH_BAR_HEIGHT: u32 = 36;
const ENTRY_HEIGHT: u32 = 28;
const FOOTER_HEIGHT: u32 = 52;
const SIDEBAR_WIDTH: u32 = 160;

// ─── Public API ─────────────────────────────────────────────────────

/// Open the file picker in Open mode
pub fn open(initial_path: &str) {
    open_with_mode(initial_path, FilePickerMode::Open, "");
}

/// Open the file picker in Save mode
pub fn save(initial_path: &str, default_filename: &str) {
    let mut picker = FILE_PICKER.lock();
    picker.visible = true;
    picker.mode = FilePickerMode::Save;
    picker.current_path = String::from(initial_path);
    picker.filename = String::from(default_filename);
    picker.selected_index = -1;
    picker.scroll_offset = 0;
    picker.result_path = None;
    picker.cancelled = false;
    picker.history.clear();
    picker.history.push(String::from(initial_path));
    picker.history_idx = 0;
    picker.hovered_index = -1;
    drop(picker);
    refresh_entries();
}

/// Open the file picker in folder selection mode
pub fn select_folder(initial_path: &str) {
    open_with_mode(initial_path, FilePickerMode::SelectFolder, "");
}

fn open_with_mode(initial_path: &str, mode: FilePickerMode, filter: &str) {
    let mut picker = FILE_PICKER.lock();
    picker.visible = true;
    picker.mode = mode;
    picker.current_path = String::from(initial_path);
    picker.filename.clear();
    picker.selected_index = -1;
    picker.scroll_offset = 0;
    picker.result_path = None;
    picker.cancelled = false;
    picker.filter_ext = String::from(filter);
    picker.history.clear();
    picker.history.push(String::from(initial_path));
    picker.history_idx = 0;
    picker.hovered_index = -1;
    drop(picker);
    refresh_entries();
}

/// Close the file picker
pub fn close() {
    let mut picker = FILE_PICKER.lock();
    picker.visible = false;
    picker.cancelled = true;
}

/// Check if picker is open
pub fn is_visible() -> bool {
    FILE_PICKER.lock().visible
}

/// Get the result path (if user confirmed)
pub fn take_result() -> Option<String> {
    FILE_PICKER.lock().result_path.take()
}

/// Refresh directory listing
fn refresh_entries() {
    let path = FILE_PICKER.lock().current_path.clone();
    let filter = FILE_PICKER.lock().filter_ext.clone();
    let mode = FILE_PICKER.lock().mode;

    let mut entries = read_entries(&path, false);

    // In folder selection mode, only show directories
    if mode == FilePickerMode::SelectFolder {
        entries.retain(|e| e.is_dir);
    }

    // Apply extension filter
    if !filter.is_empty() {
        entries.retain(|e| e.is_dir || e.name.ends_with(&format!(".{}", filter)));
    }

    // Sort: directories first, then alphabetically
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let mut picker = FILE_PICKER.lock();
    picker.entries = entries;
    picker.selected_index = -1;
    picker.scroll_offset = 0;
    picker.hovered_index = -1;
}

fn navigate_to(path: &str) {
    let mut picker = FILE_PICKER.lock();
    // Truncate forward history
    let idx = picker.history_idx;
    picker.history.truncate(idx + 1);
    picker.history.push(String::from(path));
    picker.history_idx = picker.history.len() - 1;
    picker.current_path = String::from(path);
    drop(picker);
    refresh_entries();
}

fn navigate_up() {
    let path = FILE_PICKER.lock().current_path.clone();
    if path == "/" {
        return;
    }
    let parent = if let Some(pos) = path.rfind('/') {
        if pos == 0 { "/" } else { &path[..pos] }
    } else {
        "/"
    };
    navigate_to(parent);
}

fn navigate_back() {
    let mut picker = FILE_PICKER.lock();
    if picker.history_idx > 0 {
        picker.history_idx -= 1;
        let path = picker.history[picker.history_idx].clone();
        picker.current_path = path;
        drop(picker);
        refresh_entries();
    }
}

fn navigate_forward() {
    let mut picker = FILE_PICKER.lock();
    if picker.history_idx + 1 < picker.history.len() {
        picker.history_idx += 1;
        let path = picker.history[picker.history_idx].clone();
        picker.current_path = path;
        drop(picker);
        refresh_entries();
    }
}

fn confirm_selection() {
    let mut picker = FILE_PICKER.lock();
    match picker.mode {
        FilePickerMode::Open => {
            if picker.selected_index >= 0 {
                let idx = picker.selected_index as usize;
                if idx < picker.entries.len() {
                    let entry = &picker.entries[idx];
                    if entry.is_dir {
                        let new_path = if picker.current_path == "/" {
                            format!("/{}", entry.name)
                        } else {
                            format!("{}/{}", picker.current_path, entry.name)
                        };
                        drop(picker);
                        navigate_to(&new_path);
                        return;
                    }
                    let full_path = if picker.current_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", picker.current_path, entry.name)
                    };
                    picker.result_path = Some(full_path);
                    picker.visible = false;
                }
            }
        }
        FilePickerMode::Save => {
            if !picker.filename.is_empty() {
                let full_path = if picker.current_path == "/" {
                    format!("/{}", picker.filename)
                } else {
                    format!("{}/{}", picker.current_path, picker.filename)
                };
                picker.result_path = Some(full_path);
                picker.visible = false;
            }
        }
        FilePickerMode::SelectFolder => {
            picker.result_path = Some(picker.current_path.clone());
            picker.visible = false;
        }
    }
}

// ─── Rendering ──────────────────────────────────────────────────────

/// Draw the file picker overlay
pub fn draw(fb: &mut FrameBuffer) {
    let picker = FILE_PICKER.lock();
    if !picker.visible {
        return;
    }

    let (sw, sh) = (fb.width as i32, fb.height as i32);

    // Center the dialog
    let x = (sw - PICKER_WIDTH as i32) / 2;
    let y = (sh - PICKER_HEIGHT as i32) / 2;

    // Dimmed backdrop
    let bg = Pixel::new(0, 0, 0, 140);
    fb.fill_rect(Rect::new(0, 0, sw as u32, sh as u32), bg);

    // Dialog background
    let dialog_bg = Pixel::new(24, 28, 38, 245);
    fb.fill_rounded_rect_aa(Rect::new(x, y, PICKER_WIDTH, PICKER_HEIGHT), dialog_bg, 10);

    // Border
    let border = Pixel::new(60, 80, 120, 100);
    fb.draw_rounded_rect(Rect::new(x, y, PICKER_WIDTH, PICKER_HEIGHT), border, 10, 1);

    // ── Header ──
    let title = match picker.mode {
        FilePickerMode::Open => "Open File",
        FilePickerMode::Save => "Save File",
        FilePickerMode::SelectFolder => "Select Folder",
    };
    let title_color = Pixel::rgb(230, 235, 245);
    fonts::draw_string_compact(fb, x + 16, y + 14, title, title_color, 2);

    // Close button (X)
    let close_x = x + PICKER_WIDTH as i32 - 36;
    let close_y = y + 12;
    fonts::draw_string_compact(fb, close_x, close_y, "✕", Pixel::rgb(180, 180, 200), 2);

    // ── Path bar ──
    let path_y = y + HEADER_HEIGHT as i32;
    let path_bg = Pixel::new(18, 22, 32, 200);
    fb.fill_rect(
        Rect::new(x + 1, path_y, PICKER_WIDTH - 2, PATH_BAR_HEIGHT),
        path_bg,
    );

    // Navigation buttons
    let nav_color = Pixel::rgb(140, 160, 200);
    let can_back = picker.history_idx > 0;
    let can_fwd = picker.history_idx + 1 < picker.history.len();
    let back_color = if can_back {
        nav_color
    } else {
        Pixel::rgb(60, 70, 90)
    };
    let fwd_color = if can_fwd {
        nav_color
    } else {
        Pixel::rgb(60, 70, 90)
    };

    fonts::draw_string_compact(fb, x + 12, path_y + 10, "←", back_color, 2);
    fonts::draw_string_compact(fb, x + 30, path_y + 10, "→", fwd_color, 2);
    fonts::draw_string_compact(fb, x + 48, path_y + 10, "↑", nav_color, 2);

    // Path text
    let path_text = &picker.current_path;
    let path_display = if path_text.len() > 60 {
        format!("...{}", &path_text[path_text.len() - 57..])
    } else {
        path_text.clone()
    };
    fonts::draw_string_compact(
        fb,
        x + 72,
        path_y + 10,
        &path_display,
        Pixel::rgb(180, 190, 210),
        1,
    );

    // ── Sidebar (quick access) ──
    let list_y = path_y + PATH_BAR_HEIGHT as i32;
    let list_height = PICKER_HEIGHT - HEADER_HEIGHT - PATH_BAR_HEIGHT - FOOTER_HEIGHT;

    let sidebar_bg = Pixel::new(20, 24, 34, 200);
    fb.fill_rect(
        Rect::new(x + 1, list_y, SIDEBAR_WIDTH, list_height),
        sidebar_bg,
    );

    let sidebar_items = [
        ("🏠", "Home", "/home"),
        ("📁", "Documents", "/home/documents"),
        ("📥", "Downloads", "/home/downloads"),
        ("🖼", "Pictures", "/home/pictures"),
        ("🎵", "Music", "/home/music"),
        ("💿", "Root", "/"),
    ];

    let label_color = Pixel::rgb(160, 170, 190);
    fonts::draw_string_compact(
        fb,
        x + 10,
        list_y + 6,
        "Quick Access",
        Pixel::rgb(100, 110, 130),
        1,
    );

    for (i, (icon, name, _path)) in sidebar_items.iter().enumerate() {
        let iy = list_y + 24 + i as i32 * 26;
        let is_current = picker.current_path == *_path;
        if is_current {
            let hl = Pixel::new(60, 100, 180, 60);
            fb.fill_rect(Rect::new(x + 2, iy - 2, SIDEBAR_WIDTH - 2, 24), hl);
        }
        let color = if is_current {
            colors::accent()
        } else {
            label_color
        };
        fonts::draw_string_compact(fb, x + 10, iy + 4, icon, color, 1);
        fonts::draw_string_compact(fb, x + 28, iy + 4, name, color, 1);
    }

    // ── File list ──
    let file_x = x + SIDEBAR_WIDTH as i32 + 1;
    let file_w = PICKER_WIDTH - SIDEBAR_WIDTH - 2;
    let max_visible = (list_height / ENTRY_HEIGHT) as usize;
    let scroll = picker.scroll_offset as usize;

    // Column headers
    let hdr_bg = Pixel::new(30, 36, 50, 200);
    fb.fill_rect(Rect::new(file_x, list_y, file_w, 24), hdr_bg);
    let hdr_color = Pixel::rgb(120, 130, 150);
    fonts::draw_string_compact(fb, file_x + 8, list_y + 6, "Name", hdr_color, 1);
    fonts::draw_string_compact(
        fb,
        file_x + file_w as i32 - 140,
        list_y + 6,
        "Size",
        hdr_color,
        1,
    );
    fonts::draw_string_compact(
        fb,
        file_x + file_w as i32 - 70,
        list_y + 6,
        "Type",
        hdr_color,
        1,
    );

    // Entries
    let entry_start_y = list_y + 24;
    for vi in 0..max_visible {
        let ei = scroll + vi;
        if ei >= picker.entries.len() {
            break;
        }
        let entry = &picker.entries[ei];
        let ey = entry_start_y + vi as i32 * ENTRY_HEIGHT as i32;

        // Selection highlight
        if ei as i32 == picker.selected_index {
            let sel_bg = Pixel::new(50, 90, 160, 120);
            fb.fill_rect(Rect::new(file_x, ey, file_w, ENTRY_HEIGHT), sel_bg);
        } else if ei as i32 == picker.hovered_index {
            let hov_bg = Pixel::new(40, 50, 70, 80);
            fb.fill_rect(Rect::new(file_x, ey, file_w, ENTRY_HEIGHT), hov_bg);
        }

        // Icon
        let icon = if entry.is_dir { "📁" } else { "📄" };
        let name_color = if entry.is_dir {
            Pixel::rgb(120, 180, 255)
        } else {
            Pixel::rgb(210, 215, 225)
        };

        fonts::draw_string_compact(fb, file_x + 8, ey + 7, icon, name_color, 1);

        // Name (truncate if too long)
        let max_name_chars = ((file_w - 180) / 7) as usize;
        let display_name = if entry.name.len() > max_name_chars {
            format!("{}…", &entry.name[..max_name_chars - 1])
        } else {
            entry.name.clone()
        };
        fonts::draw_string_compact(fb, file_x + 26, ey + 7, &display_name, name_color, 1);

        // Size
        if !entry.size.is_empty() {
            fonts::draw_string_compact(
                fb,
                file_x + file_w as i32 - 140,
                ey + 7,
                &entry.size,
                Pixel::rgb(140, 150, 170),
                1,
            );
        }

        // Type (short)
        let short_kind = if entry.kind.len() > 8 {
            &entry.kind[..8]
        } else {
            &entry.kind
        };
        fonts::draw_string_compact(
            fb,
            file_x + file_w as i32 - 70,
            ey + 7,
            short_kind,
            Pixel::rgb(100, 110, 130),
            1,
        );
    }

    // Scrollbar if needed
    if picker.entries.len() > max_visible {
        let sb_x = x + PICKER_WIDTH as i32 - 8;
        let total = picker.entries.len() as f32;
        let visible = max_visible as f32;
        let ratio = visible / total;
        let bar_h = ((list_height as f32 - 24.0) * ratio).max(20.0) as u32;
        let scroll_range = total - visible;
        let bar_offset = if scroll_range > 0.0 {
            ((list_height - 24 - bar_h) as f32 * (scroll as f32 / scroll_range)) as i32
        } else {
            0
        };
        let sb_bg = Pixel::new(50, 60, 80, 100);
        let sb_fg = Pixel::new(100, 120, 160, 160);
        fb.fill_rect(Rect::new(sb_x, entry_start_y, 6, list_height - 24), sb_bg);
        fb.fill_rounded_rect_aa(
            Rect::new(sb_x, entry_start_y + bar_offset, 6, bar_h),
            sb_fg,
            3,
        );
    }

    // ── Footer ──
    let footer_y = y + PICKER_HEIGHT as i32 - FOOTER_HEIGHT as i32;
    let footer_bg = Pixel::new(20, 24, 34, 220);
    fb.fill_rect(
        Rect::new(x + 1, footer_y, PICKER_WIDTH - 2, FOOTER_HEIGHT - 1),
        footer_bg,
    );

    // Filename input (Save mode)
    if picker.mode == FilePickerMode::Save {
        let input_bg = Pixel::new(30, 36, 48, 220);
        fb.fill_rounded_rect_aa(Rect::new(x + 12, footer_y + 10, 350, 28), input_bg, 4);
        let input_border = Pixel::new(70, 100, 160, 120);
        fb.draw_rounded_rect(
            Rect::new(x + 12, footer_y + 10, 350, 28),
            input_border,
            4,
            1,
        );
        let text = if picker.filename.is_empty() {
            "Enter filename..."
        } else {
            &picker.filename
        };
        let text_color = if picker.filename.is_empty() {
            Pixel::rgb(100, 110, 130)
        } else {
            Pixel::rgb(220, 225, 235)
        };
        fonts::draw_string_compact(fb, x + 20, footer_y + 18, text, text_color, 1);
    }

    // Buttons
    let btn_w = 90u32;
    let btn_h = 30u32;
    let cancel_x = x + PICKER_WIDTH as i32 - (btn_w as i32 * 2 + 24);
    let confirm_x = x + PICKER_WIDTH as i32 - btn_w as i32 - 12;
    let btn_y = footer_y + (FOOTER_HEIGHT as i32 - btn_h as i32) / 2;

    // Cancel button
    let cancel_bg = Pixel::new(50, 55, 70, 200);
    fb.fill_rounded_rect_aa(Rect::new(cancel_x, btn_y, btn_w, btn_h), cancel_bg, 6);
    fonts::draw_string_compact(
        fb,
        cancel_x + 24,
        btn_y + 9,
        "Cancel",
        Pixel::rgb(180, 185, 200),
        1,
    );

    // Confirm button
    let confirm_label = match picker.mode {
        FilePickerMode::Open => "Open",
        FilePickerMode::Save => "Save",
        FilePickerMode::SelectFolder => "Select",
    };
    let accent = colors::accent();
    fb.fill_rounded_rect_aa(Rect::new(confirm_x, btn_y, btn_w, btn_h), accent, 6);
    fonts::draw_string_compact(
        fb,
        confirm_x + 24,
        btn_y + 9,
        confirm_label,
        Pixel::rgb(255, 255, 255),
        1,
    );
}

// ─── Hit Testing & Click Handling ───────────────────────────────────

/// Handle click events for the file picker. Returns true if the click
/// was consumed (i.e. the picker is visible and the click hit it).
pub fn handle_click(mx: i32, my: i32) -> bool {
    let visible = FILE_PICKER.lock().visible;
    if !visible {
        return false;
    }

    let (sw, sh) = super::cached_screen_size();
    let x = (sw - PICKER_WIDTH as i32) / 2;
    let y = (sh - PICKER_HEIGHT as i32) / 2;

    // Outside dialog → cancel
    if mx < x || mx > x + PICKER_WIDTH as i32 || my < y || my > y + PICKER_HEIGHT as i32 {
        close();
        return true;
    }

    // Close button
    let close_x = x + PICKER_WIDTH as i32 - 36;
    let close_y = y + 12;
    if mx >= close_x && mx <= close_x + 20 && my >= close_y && my <= close_y + 20 {
        close();
        return true;
    }

    let path_y = y + HEADER_HEIGHT as i32;
    let list_y = path_y + PATH_BAR_HEIGHT as i32;

    // Navigation buttons
    if my >= path_y && my < path_y + PATH_BAR_HEIGHT as i32 {
        if mx >= x + 8 && mx < x + 28 {
            navigate_back();
            return true;
        }
        if mx >= x + 28 && mx < x + 46 {
            navigate_forward();
            return true;
        }
        if mx >= x + 46 && mx < x + 64 {
            navigate_up();
            return true;
        }
        return true;
    }

    // Sidebar quick access
    let list_height = PICKER_HEIGHT - HEADER_HEIGHT - PATH_BAR_HEIGHT - FOOTER_HEIGHT;
    if mx >= x && mx < x + SIDEBAR_WIDTH as i32 && my >= list_y && my < list_y + list_height as i32
    {
        let sidebar_items = [
            "/home",
            "/home/documents",
            "/home/downloads",
            "/home/pictures",
            "/home/music",
            "/",
        ];
        let rel_y = my - (list_y + 24);
        if rel_y >= 0 {
            let idx = rel_y / 26;
            if (idx as usize) < sidebar_items.len() {
                navigate_to(sidebar_items[idx as usize]);
            }
        }
        return true;
    }

    // File list entries
    let file_x = x + SIDEBAR_WIDTH as i32;
    let entry_start_y = list_y + 24;
    let file_w = PICKER_WIDTH - SIDEBAR_WIDTH - 2;
    if mx >= file_x && mx < file_x + file_w as i32 && my >= entry_start_y {
        let max_visible = ((list_height - 24) / ENTRY_HEIGHT) as usize;
        let rel_y = my - entry_start_y;
        let vi = rel_y / ENTRY_HEIGHT as i32;
        let picker_info = FILE_PICKER.lock();
        let ei = picker_info.scroll_offset as i32 + vi;
        let count = picker_info.entries.len() as i32;
        drop(picker_info);

        if ei >= 0 && ei < count {
            FILE_PICKER.lock().selected_index = ei;
        }
        return true;
    }

    // Footer buttons
    let footer_y = y + PICKER_HEIGHT as i32 - FOOTER_HEIGHT as i32;
    let btn_w = 90i32;
    let btn_h = 30i32;
    let cancel_x = x + PICKER_WIDTH as i32 - (btn_w * 2 + 24);
    let confirm_x = x + PICKER_WIDTH as i32 - btn_w - 12;
    let btn_y = footer_y + (FOOTER_HEIGHT as i32 - btn_h) / 2;

    if my >= btn_y && my <= btn_y + btn_h {
        if mx >= cancel_x && mx <= cancel_x + btn_w {
            close();
            return true;
        }
        if mx >= confirm_x && mx <= confirm_x + btn_w {
            confirm_selection();
            return true;
        }
    }

    true // Consume click even if it didn't hit a specific control
}

/// Handle double-click — open directories, confirm files
pub fn handle_double_click(mx: i32, my: i32) -> bool {
    let visible = FILE_PICKER.lock().visible;
    if !visible {
        return false;
    }

    let (sw, sh) = super::cached_screen_size();
    let x = (sw - PICKER_WIDTH as i32) / 2;
    let y = (sh - PICKER_HEIGHT as i32) / 2;
    let path_y = y + HEADER_HEIGHT as i32;
    let list_y = path_y + PATH_BAR_HEIGHT as i32;
    let list_height = PICKER_HEIGHT - HEADER_HEIGHT - PATH_BAR_HEIGHT - FOOTER_HEIGHT;
    let file_x = x + SIDEBAR_WIDTH as i32;
    let entry_start_y = list_y + 24;
    let file_w = PICKER_WIDTH - SIDEBAR_WIDTH - 2;

    if mx >= file_x && mx < file_x + file_w as i32 && my >= entry_start_y {
        let rel_y = my - entry_start_y;
        let vi = rel_y / ENTRY_HEIGHT as i32;
        let picker = FILE_PICKER.lock();
        let ei = picker.scroll_offset as i32 + vi;
        if ei >= 0 && (ei as usize) < picker.entries.len() {
            let entry = picker.entries[ei as usize].clone();
            let current = picker.current_path.clone();
            drop(picker);

            if entry.is_dir {
                let new_path = if current == "/" {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", current, entry.name)
                };
                navigate_to(&new_path);
            } else {
                // Double-click on file → confirm
                FILE_PICKER.lock().selected_index = ei;
                confirm_selection();
            }
            return true;
        }
    }

    handle_click(mx, my)
}

/// Handle scroll in file list
pub fn handle_scroll(mx: i32, my: i32, delta: i8) -> bool {
    let visible = FILE_PICKER.lock().visible;
    if !visible {
        return false;
    }

    let (sw, sh) = super::cached_screen_size();
    let x = (sw - PICKER_WIDTH as i32) / 2;
    let y = (sh - PICKER_HEIGHT as i32) / 2;

    // Only scroll if mouse is over the dialog
    if mx < x || mx > x + PICKER_WIDTH as i32 || my < y || my > y + PICKER_HEIGHT as i32 {
        return false;
    }

    let mut picker = FILE_PICKER.lock();
    let list_height = PICKER_HEIGHT - HEADER_HEIGHT - PATH_BAR_HEIGHT - FOOTER_HEIGHT;
    let max_visible = ((list_height - 24) / ENTRY_HEIGHT) as usize;
    let total = picker.entries.len();

    if delta > 0 && picker.scroll_offset > 0 {
        picker.scroll_offset = picker.scroll_offset.saturating_sub(3);
    } else if delta < 0 && total > max_visible {
        let max_scroll = (total - max_visible) as u32;
        picker.scroll_offset = (picker.scroll_offset + 3).min(max_scroll);
    }
    true
}

/// Handle hover for entry highlighting
pub fn update_hover(mx: i32, my: i32) {
    let visible = FILE_PICKER.lock().visible;
    if !visible {
        return;
    }

    let (sw, sh) = super::cached_screen_size();
    let x = (sw - PICKER_WIDTH as i32) / 2;
    let y = (sh - PICKER_HEIGHT as i32) / 2;
    let path_y = y + HEADER_HEIGHT as i32;
    let list_y = path_y + PATH_BAR_HEIGHT as i32;
    let file_x = x + SIDEBAR_WIDTH as i32;
    let entry_start_y = list_y + 24;
    let file_w = PICKER_WIDTH - SIDEBAR_WIDTH - 2;
    let list_height = PICKER_HEIGHT - HEADER_HEIGHT - PATH_BAR_HEIGHT - FOOTER_HEIGHT;

    let mut picker = FILE_PICKER.lock();
    if mx >= file_x && mx < file_x + file_w as i32 && my >= entry_start_y {
        let rel_y = my - entry_start_y;
        let vi = rel_y / ENTRY_HEIGHT as i32;
        let ei = picker.scroll_offset as i32 + vi;
        if ei >= 0 && (ei as usize) < picker.entries.len() {
            picker.hovered_index = ei;
        } else {
            picker.hovered_index = -1;
        }
    } else {
        picker.hovered_index = -1;
    }
}

/// Handle keyboard input for the file picker (Save mode filename entry)
pub fn handle_key(c: char) {
    let mut picker = FILE_PICKER.lock();
    if !picker.visible || picker.mode != FilePickerMode::Save {
        return;
    }
    match c {
        '\x08' => {
            // Backspace
            picker.filename.pop();
        }
        '\n' | '\r' => {
            // Enter → confirm
            drop(picker);
            confirm_selection();
        }
        c if !c.is_control() && picker.filename.len() < 255 => {
            picker.filename.push(c);
        }
        _ => {}
    }
}

/// Returns true if any popup from the file picker is open
pub fn any_popup_open() -> bool {
    FILE_PICKER.lock().visible
}
