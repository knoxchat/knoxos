/// Exposé / Mission Control — Full-screen window overview
///
/// Triggered by Super+E or hot corner. Shows all visible windows in a
/// scaled grid layout. Clicking a window focuses it, pressing Escape exits.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window::{WINDOW_MANAGER, WindowContentType, WindowId};

lazy_static::lazy_static! {
    pub static ref EXPOSE_STATE: Mutex<ExposeState> = Mutex::new(ExposeState::new());
}

/// State for the Exposé overlay
pub struct ExposeState {
    /// Whether the overlay is visible
    pub visible: bool,
    /// Cached window entries for display
    pub entries: Vec<ExposeEntry>,
    /// Index of the currently hovered/selected entry
    pub selected: Option<usize>,
}

/// An entry in the Exposé grid
pub struct ExposeEntry {
    pub window_id: WindowId,
    pub title: String,
    pub content_type: WindowContentType,
    /// Original window rect (for content-color preview proportions)
    pub orig_rect: Rect,
    /// Computed display rect within the Exposé grid
    pub display_rect: Rect,
}

impl ExposeState {
    pub fn new() -> Self {
        Self {
            visible: false,
            entries: Vec::new(),
            selected: None,
        }
    }
}

/// Toggle the Exposé overlay
pub fn toggle() {
    let mut state = EXPOSE_STATE.lock();
    if state.visible {
        state.visible = false;
        state.entries.clear();
        state.selected = None;
    } else {
        show_inner(&mut state);
    }
}

/// Show the Exposé overlay
pub fn show() {
    let mut state = EXPOSE_STATE.lock();
    if state.visible {
        return;
    }
    show_inner(&mut state);
}

fn show_inner(state: &mut ExposeState) {
    let wm = WINDOW_MANAGER.lock();

    state.entries.clear();
    for w in wm.windows.iter().rev() {
        if w.visible {
            state.entries.push(ExposeEntry {
                window_id: w.id,
                title: w.title.clone(),
                content_type: w.content_type,
                orig_rect: w.rect,
                display_rect: Rect::new(0, 0, 0, 0), // computed during draw
            });
        }
    }

    if state.entries.is_empty() {
        state.visible = false;
        return;
    }

    state.visible = true;
    state.selected = None;
}

/// Close the overlay without changing focus
pub fn dismiss() {
    let mut state = EXPOSE_STATE.lock();
    state.visible = false;
    state.entries.clear();
    state.selected = None;
}

/// Check if the overlay is visible
pub fn is_visible() -> bool {
    EXPOSE_STATE.lock().visible
}

/// Handle mouse move — update hover/selection
pub fn handle_mouse_move(mx: i32, my: i32) {
    let mut state = EXPOSE_STATE.lock();
    if !state.visible {
        return;
    }
    let mut found = None;
    for (i, entry) in state.entries.iter().enumerate() {
        if entry.display_rect.contains(mx, my) {
            found = Some(i);
            break;
        }
    }
    if state.selected != found {
        state.selected = found;
        super::request_redraw();
    }
}

/// Handle mouse click — select hovered window and exit Exposé
pub fn handle_click(mx: i32, my: i32) -> Option<WindowId> {
    let mut state = EXPOSE_STATE.lock();
    if !state.visible {
        return None;
    }
    for entry in state.entries.iter() {
        if entry.display_rect.contains(mx, my) {
            let wid = entry.window_id;
            state.visible = false;
            state.entries.clear();
            state.selected = None;
            return Some(wid);
        }
    }
    // Clicked outside any window preview — dismiss
    state.visible = false;
    state.entries.clear();
    state.selected = None;
    None
}

/// Draw the Exposé overlay
pub fn draw(fb: &mut FrameBuffer) {
    let mut state = EXPOSE_STATE.lock();
    if !state.visible || state.entries.is_empty() {
        return;
    }

    let sw = fb.width as i32;
    let sh = fb.height as i32;

    // Dim background
    fb.fill_rect(
        Rect::new(0, 0, sw as u32, sh as u32),
        Pixel::new(0, 0, 0, 140),
    );

    // Title: "Mission Control" at top center
    let title = "Mission Control";
    let title_w = fonts::measure_string_width_compact(title, 2) as i32;
    fonts::draw_string_compact(
        fb,
        (sw - title_w) / 2,
        24,
        title,
        Pixel::new(220, 230, 255, 220),
        2,
    );

    // Compute grid layout
    let n = state.entries.len();
    let margin = 48i32;
    let title_area = 60i32; // Top area for title
    let label_h = 28i32; // Bottom label space per tile
    let gap = 16i32;

    let avail_w = sw - margin * 2;
    let avail_h = sh - title_area - margin - 54; // 54 = taskbar height

    // Compute grid dimensions (rows × cols)
    let cols = compute_columns(n, avail_w, avail_h);
    let rows = n.div_ceil(cols);

    // Tile size
    let tile_w = (avail_w - (cols as i32 - 1) * gap) / cols as i32;
    let tile_h = (avail_h - (rows as i32 - 1) * gap) / rows as i32;
    // Cap tile height to maintain aspect ratio appearance
    let tile_h = tile_h.min(tile_w * 3 / 4);
    let content_h = tile_h - label_h;

    // Center the grid
    let grid_w = cols as i32 * tile_w + (cols as i32 - 1) * gap;
    let grid_h = rows as i32 * tile_h + (rows as i32 - 1) * gap;
    let start_x = (sw - grid_w) / 2;
    let start_y = title_area + (avail_h - grid_h) / 2;

    let selected_idx = state.selected;

    for (i, entry) in state.entries.iter_mut().enumerate() {
        let col = i % cols;
        let row = i / cols;

        let tx = start_x + col as i32 * (tile_w + gap);
        let ty = start_y + row as i32 * (tile_h + gap);

        // Store computed display rect for hit testing
        entry.display_rect = Rect::new(tx, ty, tile_w as u32, tile_h as u32);

        let is_selected = selected_idx == Some(i);

        // Selection glow
        if is_selected {
            let glow = Rect::new(tx - 4, ty - 4, tile_w as u32 + 8, tile_h as u32 + 8);
            fb.fill_rounded_rect_aa(glow, Pixel::new(0, 180, 255, 35), 14);
            fb.draw_rounded_rect(glow, Pixel::new(0, 200, 255, 180), 14, 2);
        }

        // Window preview area — content-colored rounded rect
        let preview_rect = Rect::new(tx, ty, tile_w as u32, content_h as u32);
        let bg_color = content_type_color(entry.content_type);
        fb.fill_rounded_rect_aa(preview_rect, bg_color, 10);

        // Fake title bar at top of preview
        let title_bar = Rect::new(tx, ty, tile_w as u32, 20);
        fb.fill_rounded_rect_aa(title_bar, Pixel::new(30, 35, 50, 200), 10);
        // Fake title bar bottom (square off the bottom corners)
        fb.fill_rect(
            Rect::new(tx, ty + 10, tile_w as u32, 10),
            Pixel::new(30, 35, 50, 200),
        );

        // Window control dots in fake title bar
        let dot_y_pos = ty + 10;
        fb.fill_circle_aa(tx + tile_w - 16, dot_y_pos, 4, Pixel::rgb(255, 95, 86));
        fb.fill_circle_aa(tx + tile_w - 30, dot_y_pos, 4, Pixel::rgb(255, 189, 46));
        fb.fill_circle_aa(tx + tile_w - 44, dot_y_pos, 4, Pixel::rgb(39, 201, 63));

        // Window type icon centered in content area
        let icon_cx = tx + tile_w / 2;
        let icon_cy = ty + 20 + (content_h - 20) / 2;
        draw_expose_icon(fb, icon_cx, icon_cy, entry.content_type);

        // Border
        fb.draw_rounded_rect(
            preview_rect,
            Pixel::new(80, 100, 140, if is_selected { 160 } else { 60 }),
            10,
            1,
        );

        // Window title below preview
        let label_y = ty + content_h + 4;
        let label_color = if is_selected {
            Pixel::new(220, 240, 255, 255)
        } else {
            Pixel::new(170, 180, 200, 200)
        };
        let max_chars = (tile_w / 7) as usize;
        let display_title = if entry.title.len() > max_chars {
            let mut t = String::from(&entry.title[..max_chars.saturating_sub(2)]);
            t.push_str("..");
            t
        } else {
            entry.title.clone()
        };
        let text_w = fonts::measure_string_width_compact(&display_title, 1) as i32;
        let text_x = tx + (tile_w - text_w) / 2;
        fonts::draw_string_compact(fb, text_x, label_y, &display_title, label_color, 1);
    }

    // Hint text at the bottom
    let hint = "Click a window to focus  |  Esc to close  |  Super+E to toggle";
    let hint_w = fonts::measure_string_width_compact(hint, 1) as i32;
    fonts::draw_string_compact(
        fb,
        (sw - hint_w) / 2,
        sh - 70,
        hint,
        Pixel::new(140, 150, 170, 160),
        1,
    );
}

/// Compute optimal number of columns for the grid
fn compute_columns(n: usize, avail_w: i32, avail_h: i32) -> usize {
    if n <= 1 {
        return 1;
    }
    if n <= 2 {
        return 2;
    }
    if n <= 4 {
        return 2;
    }
    if n <= 6 {
        return 3;
    }
    if n <= 9 {
        return 3;
    }
    // For many windows, estimate cols from count
    let cols = if avail_w > avail_h {
        n.div_ceil(2) + 1
    } else {
        n.div_ceil(3) + 1
    };
    cols.clamp(2, 6)
}

/// Get a background color for a given content type
fn content_type_color(ct: WindowContentType) -> Pixel {
    match ct {
        WindowContentType::Terminal => Pixel::new(25, 28, 38, 230),
        WindowContentType::Browser => Pixel::new(40, 45, 58, 230),
        WindowContentType::FileExplorer => Pixel::new(30, 38, 48, 230),
        WindowContentType::Settings => Pixel::new(22, 28, 38, 230),
        WindowContentType::AIAssistant => Pixel::new(18, 22, 38, 230),
        WindowContentType::TextEditor => Pixel::new(28, 28, 30, 230),
        WindowContentType::Empty => Pixel::new(35, 40, 52, 230),
        WindowContentType::ArchiveViewer => Pixel::new(45, 40, 28, 230),
        WindowContentType::DiskUtility => Pixel::new(30, 40, 52, 230),
        WindowContentType::BluetoothManager => Pixel::new(18, 35, 55, 230),
        WindowContentType::CalendarApp => Pixel::new(50, 28, 28, 230),
        WindowContentType::LogViewer => Pixel::new(28, 32, 28, 230),
        WindowContentType::SoftwareUpdater => Pixel::new(22, 38, 28, 230),
        WindowContentType::SoftwareCenter => Pixel::new(28, 32, 52, 230),
        WindowContentType::SetupWizard => Pixel::new(22, 42, 32, 230),
        WindowContentType::TaskManager => Pixel::new(38, 32, 48, 230),
        WindowContentType::Calculator => Pixel::new(32, 32, 42, 230),
        WindowContentType::ImageViewer => Pixel::new(28, 28, 32, 230),
    }
}

/// Draw a larger icon for each content type (larger than alt_tab version)
fn draw_expose_icon(fb: &mut FrameBuffer, cx: i32, cy: i32, ct: WindowContentType) {
    let color = Pixel::new(100, 150, 220, 140);

    match ct {
        WindowContentType::Terminal => {
            // >_ prompt icon (larger)
            fb.fill_rect(Rect::new(cx - 18, cy - 6, 9, 3), color);
            fb.fill_rect(Rect::new(cx - 18, cy - 3, 3, 9), color);
            fb.fill_rect(Rect::new(cx - 9, cy + 6, 16, 3), color);
        }
        WindowContentType::Browser => {
            // Globe
            fb.draw_rounded_rect(Rect::new(cx - 16, cy - 16, 32, 32), color, 16, 2);
            fb.fill_rect(Rect::new(cx - 14, cy - 1, 28, 2), color);
            fb.fill_rect(Rect::new(cx - 1, cy - 14, 2, 28), color);
        }
        WindowContentType::FileExplorer => {
            // Folder
            fb.fill_rounded_rect_aa(Rect::new(cx - 16, cy - 8, 32, 20), color, 4);
            fb.fill_rect(Rect::new(cx - 16, cy - 12, 16, 6), color);
        }
        WindowContentType::Settings => {
            // Gear
            fb.draw_rounded_rect(Rect::new(cx - 10, cy - 10, 20, 20), color, 10, 3);
            fb.fill_rect(Rect::new(cx - 1, cy - 16, 2, 6), color);
            fb.fill_rect(Rect::new(cx - 1, cy + 10, 2, 6), color);
            fb.fill_rect(Rect::new(cx - 16, cy - 1, 6, 2), color);
            fb.fill_rect(Rect::new(cx + 10, cy - 1, 6, 2), color);
        }
        WindowContentType::AIAssistant => {
            fb.fill_circle_aa(cx, cy, 14, color);
            fb.fill_circle_aa(cx, cy, 7, Pixel::new(0, 200, 255, 100));
        }
        WindowContentType::TextEditor => {
            fb.fill_rounded_rect_aa(Rect::new(cx - 12, cy - 16, 24, 32), color, 3);
            for i in 0..5 {
                fb.fill_rect(
                    Rect::new(cx - 8, cy - 10 + i * 6, 16, 2),
                    Pixel::new(40, 50, 60, 180),
                );
            }
        }
        WindowContentType::TaskManager => {
            fb.fill_rect(Rect::new(cx - 10, cy + 2, 6, 10), Pixel::rgb(82, 139, 255));
            fb.fill_rect(Rect::new(cx - 2, cy - 6, 6, 18), Pixel::rgb(60, 180, 100));
            fb.fill_rect(Rect::new(cx + 6, cy - 2, 6, 14), Pixel::rgb(230, 180, 40));
        }
        WindowContentType::Calculator => {
            fb.fill_rounded_rect_aa(Rect::new(cx - 12, cy - 12, 24, 24), color, 4);
            fb.fill_rect(Rect::new(cx - 7, cy - 7, 5, 5), Pixel::new(40, 50, 60, 180));
            fb.fill_rect(Rect::new(cx + 2, cy - 7, 5, 5), Pixel::new(40, 50, 60, 180));
            fb.fill_rect(Rect::new(cx - 7, cy + 2, 5, 5), Pixel::new(40, 50, 60, 180));
            fb.fill_rect(Rect::new(cx + 2, cy + 2, 5, 5), Pixel::new(40, 50, 60, 180));
        }
        _ => {
            // Generic window icon
            fb.draw_rounded_rect(Rect::new(cx - 16, cy - 12, 32, 24), color, 6, 2);
            fb.fill_rect(Rect::new(cx - 16, cy - 12, 32, 6), color);
        }
    }
}
