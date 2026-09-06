/// Alt+Tab Window Switcher Overlay (P8.15)
///
/// Displays a centered panel showing all open windows as thumbnails
/// with titles, allowing keyboard-driven window switching.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window::{WINDOW_MANAGER, WindowContentType, WindowId};

/// Global Alt+Tab state
lazy_static::lazy_static! {
    pub static ref ALT_TAB: Mutex<AltTabState> = Mutex::new(AltTabState::new());
}

/// Alt+Tab overlay state
pub struct AltTabState {
    /// Whether the overlay is currently visible
    pub visible: bool,
    /// Index of the currently highlighted window in the entries list
    pub selected_index: usize,
    /// Cached list of (window_id, title, content_type) for display
    pub entries: Vec<AltTabEntry>,
}

/// A single entry in the Alt+Tab switcher
pub struct AltTabEntry {
    pub window_id: WindowId,
    pub title: String,
    pub content_type: WindowContentType,
}

impl AltTabState {
    pub fn new() -> Self {
        Self {
            visible: false,
            selected_index: 0,
            entries: Vec::new(),
        }
    }
}

/// Show the Alt+Tab overlay and populate it with current windows.
/// `reverse` = true means the user pressed Shift+Alt+Tab.
pub fn show(reverse: bool) {
    let mut state = ALT_TAB.lock();
    let wm = WINDOW_MANAGER.lock();

    // Collect all non-minimized windows in z-order (topmost first)
    state.entries.clear();
    for w in wm.windows.iter().rev() {
        if w.visible {
            state.entries.push(AltTabEntry {
                window_id: w.id,
                title: w.title.clone(),
                content_type: w.content_type,
            });
        }
    }

    if state.entries.is_empty() {
        state.visible = false;
        return;
    }

    state.visible = true;

    // Start with the second window selected (the one we're switching TO)
    if state.entries.len() > 1 {
        state.selected_index = if reverse { state.entries.len() - 1 } else { 1 };
    } else {
        state.selected_index = 0;
    }
}

/// Cycle to the next/previous window in the Alt+Tab overlay.
pub fn cycle(reverse: bool) {
    let mut state = ALT_TAB.lock();
    if state.entries.is_empty() {
        return;
    }
    let len = state.entries.len();
    if reverse {
        state.selected_index = if state.selected_index == 0 {
            len - 1
        } else {
            state.selected_index - 1
        };
    } else {
        state.selected_index = (state.selected_index + 1) % len;
    }
}

/// Confirm the selection and close the overlay. Returns the selected window ID.
pub fn confirm() -> Option<WindowId> {
    let mut state = ALT_TAB.lock();
    if !state.visible || state.entries.is_empty() {
        state.visible = false;
        return None;
    }
    let wid = state.entries[state.selected_index].window_id;
    state.visible = false;
    state.entries.clear();
    Some(wid)
}

/// Close the overlay without switching
pub fn dismiss() {
    let mut state = ALT_TAB.lock();
    state.visible = false;
    state.entries.clear();
}

/// Check if the overlay is visible
pub fn is_visible() -> bool {
    ALT_TAB.lock().visible
}

/// Draw the Alt+Tab overlay on the framebuffer
pub fn draw(fb: &mut FrameBuffer) {
    let state = ALT_TAB.lock();
    if !state.visible || state.entries.is_empty() {
        return;
    }

    let screen_w = fb.width as i32;
    let screen_h = fb.height as i32;

    let entry_count = state.entries.len();
    let thumb_w = 120i32;
    let thumb_h = 90i32;
    let padding = 16i32;
    let spacing = 12i32;
    let title_h = 24i32;

    // Panel dimensions
    let total_w = entry_count as i32 * (thumb_w + spacing) - spacing + padding * 2;
    let total_h = thumb_h + title_h + padding * 2 + 8;
    let panel_x = (screen_w - total_w) / 2;
    let panel_y = (screen_h - total_h) / 2;

    // Clamp panel width to screen
    let panel_w = total_w.min(screen_w - 40) as u32;
    let panel_h = total_h as u32;
    let panel_rect = Rect::new(panel_x.max(20), panel_y, panel_w, panel_h);

    // Dimmed background overlay
    fb.fill_rect(
        Rect::new(0, 0, screen_w as u32, screen_h as u32),
        Pixel::new(0, 0, 0, 120),
    );

    // Panel background: dark frosted glass
    fb.fill_rounded_rect_aa(panel_rect, Pixel::new(20, 25, 35, 230), 16);
    // Panel border: subtle holographic
    fb.draw_rounded_rect(panel_rect, Pixel::new(80, 160, 255, 60), 16, 1);

    // Draw each window entry
    let start_x = panel_rect.x + padding;
    let start_y = panel_rect.y + padding;

    for (i, entry) in state.entries.iter().enumerate() {
        let ex = start_x + i as i32 * (thumb_w + spacing);
        if ex + thumb_w > panel_rect.x + panel_rect.width as i32 - padding {
            break; // Don't draw entries that overflow
        }

        let is_selected = i == state.selected_index;

        // Thumbnail background
        let thumb_rect = Rect::new(ex, start_y, thumb_w as u32, thumb_h as u32);

        if is_selected {
            // Highlighted: cyan glow border
            let glow_rect = Rect::new(ex - 3, start_y - 3, thumb_w as u32 + 6, thumb_h as u32 + 6);
            fb.fill_rounded_rect_aa(glow_rect, Pixel::new(0, 180, 255, 40), 10);
            fb.draw_rounded_rect(glow_rect, Pixel::new(0, 200, 255, 200), 10, 2);
        }

        // Window type icon/color preview
        let content_color = match entry.content_type {
            WindowContentType::Terminal => Pixel::rgb(30, 30, 40),
            WindowContentType::Browser => Pixel::rgb(45, 50, 60),
            WindowContentType::FileExplorer => Pixel::rgb(35, 40, 50),
            WindowContentType::Settings => Pixel::rgb(25, 30, 40),
            WindowContentType::AIAssistant => Pixel::rgb(20, 25, 40),
            WindowContentType::TextEditor => Pixel::rgb(30, 30, 30),
            WindowContentType::Empty => Pixel::rgb(40, 45, 55),
            WindowContentType::ArchiveViewer => Pixel::rgb(50, 45, 30),
            WindowContentType::DiskUtility => Pixel::rgb(35, 45, 55),
            WindowContentType::BluetoothManager => Pixel::rgb(20, 40, 60),
            WindowContentType::CalendarApp => Pixel::rgb(55, 30, 30),
            WindowContentType::LogViewer => Pixel::rgb(30, 35, 30),
            WindowContentType::SoftwareUpdater => Pixel::rgb(25, 40, 30),
            WindowContentType::SoftwareCenter => Pixel::rgb(30, 35, 55),
            WindowContentType::SetupWizard => Pixel::rgb(25, 45, 35),
            WindowContentType::TaskManager => Pixel::rgb(40, 35, 50),
            WindowContentType::Calculator => Pixel::rgb(35, 35, 45),
            WindowContentType::ImageViewer => Pixel::rgb(30, 30, 35),
        };
        fb.fill_rounded_rect_aa(thumb_rect, content_color, 8);
        fb.draw_rounded_rect(thumb_rect, Pixel::new(80, 90, 110, 80), 8, 1);

        // Draw window type icon centered in the thumbnail
        let icon_cx = ex + thumb_w / 2;
        let icon_cy = start_y + thumb_h / 2;
        draw_content_type_icon(fb, icon_cx, icon_cy, entry.content_type);

        // Title below thumbnail
        let title_y = start_y + thumb_h + 4;
        let title_color = if is_selected {
            Pixel::new(200, 230, 255, 255)
        } else {
            Pixel::new(160, 170, 190, 200)
        };

        // Truncate title to fit
        let max_title_chars = (thumb_w / 7) as usize;
        let display_title = if entry.title.len() > max_title_chars {
            let mut t = String::from(&entry.title[..max_title_chars.saturating_sub(2)]);
            t.push_str("..");
            t
        } else {
            entry.title.clone()
        };
        // Center the title
        let title_pixel_w = display_title.len() as i32 * 7;
        let title_x = ex + (thumb_w - title_pixel_w) / 2;
        fonts::draw_string_compact(fb, title_x, title_y, &display_title, title_color, 1);
    }
}

/// Draw a simple icon representing the window content type
pub fn draw_content_type_icon(fb: &mut FrameBuffer, cx: i32, cy: i32, ct: WindowContentType) {
    let icon_color = Pixel::new(120, 160, 220, 120);

    match ct {
        WindowContentType::Terminal => {
            // Terminal: >_ prompt
            fb.fill_rect(Rect::new(cx - 12, cy - 4, 6, 2), icon_color);
            fb.fill_rect(Rect::new(cx - 12, cy - 2, 2, 6), icon_color);
            fb.fill_rect(Rect::new(cx - 6, cy + 4, 10, 2), icon_color);
        }
        WindowContentType::Browser => {
            // Globe icon (circle with horizontal lines)
            fb.draw_rounded_rect(Rect::new(cx - 10, cy - 10, 20, 20), icon_color, 10, 1);
            fb.fill_rect(Rect::new(cx - 8, cy - 1, 16, 2), icon_color);
            fb.fill_rect(Rect::new(cx - 1, cy - 8, 2, 16), icon_color);
        }
        WindowContentType::FileExplorer => {
            // Folder icon
            fb.fill_rounded_rect_aa(Rect::new(cx - 10, cy - 6, 20, 14), icon_color, 3);
            fb.fill_rect(Rect::new(cx - 10, cy - 8, 10, 4), icon_color);
        }
        WindowContentType::Settings => {
            // Gear: circle with notches
            fb.draw_rounded_rect(Rect::new(cx - 6, cy - 6, 12, 12), icon_color, 6, 2);
            fb.fill_rect(Rect::new(cx - 1, cy - 10, 2, 4), icon_color);
            fb.fill_rect(Rect::new(cx - 1, cy + 6, 2, 4), icon_color);
            fb.fill_rect(Rect::new(cx - 10, cy - 1, 4, 2), icon_color);
            fb.fill_rect(Rect::new(cx + 6, cy - 1, 4, 2), icon_color);
        }
        WindowContentType::AIAssistant => {
            // Brain/sparkle icon
            fb.fill_circle_aa(cx, cy, 8, icon_color);
            fb.fill_circle_aa(cx, cy, 4, Pixel::new(0, 200, 255, 80));
        }
        WindowContentType::TextEditor => {
            // Document with lines
            fb.fill_rounded_rect_aa(Rect::new(cx - 8, cy - 10, 16, 20), icon_color, 2);
            for i in 0..4 {
                fb.fill_rect(
                    Rect::new(cx - 5, cy - 6 + i * 4, 10, 1),
                    Pixel::new(40, 50, 60, 180),
                );
            }
        }
        WindowContentType::Empty => {
            // Generic window icon
            fb.draw_rounded_rect(Rect::new(cx - 10, cy - 8, 20, 16), icon_color, 4, 1);
            fb.fill_rect(Rect::new(cx - 10, cy - 8, 20, 4), icon_color);
        }
        WindowContentType::ArchiveViewer => {
            // Archive/zip icon — stacked pages
            fb.fill_rounded_rect_aa(Rect::new(cx - 8, cy - 8, 16, 16), icon_color, 2);
            fb.fill_rect(
                Rect::new(cx - 5, cy - 4, 10, 1),
                Pixel::new(40, 50, 60, 180),
            );
            fb.fill_rect(Rect::new(cx - 5, cy, 8, 1), Pixel::new(40, 50, 60, 180));
            fb.fill_rect(Rect::new(cx - 5, cy + 4, 6, 1), Pixel::new(40, 50, 60, 180));
        }
        WindowContentType::DiskUtility => {
            // Disk icon
            fb.fill_circle_aa(cx, cy, 8, icon_color);
            fb.fill_circle_aa(cx, cy, 3, Pixel::new(40, 50, 60, 180));
        }
        WindowContentType::BluetoothManager => {
            // BT icon
            fb.fill_rect(Rect::new(cx - 1, cy - 8, 2, 16), icon_color);
            fb.draw_line_aa(cx - 6, cy - 4, cx + 4, cy + 4, icon_color);
            fb.draw_line_aa(cx - 6, cy + 4, cx + 4, cy - 4, icon_color);
        }
        WindowContentType::CalendarApp => {
            // Calendar icon
            fb.fill_rounded_rect_aa(Rect::new(cx - 8, cy - 8, 16, 16), icon_color, 2);
            fb.fill_rect(
                Rect::new(cx - 5, cy - 2, 10, 1),
                Pixel::new(40, 50, 60, 180),
            );
            fb.fill_rect(
                Rect::new(cx - 5, cy + 2, 10, 1),
                Pixel::new(40, 50, 60, 180),
            );
        }
        WindowContentType::LogViewer => {
            // Log lines icon
            fb.fill_rect(Rect::new(cx - 7, cy - 6, 14, 2), Pixel::rgb(80, 200, 120));
            fb.fill_rect(Rect::new(cx - 7, cy - 1, 10, 2), Pixel::rgb(220, 180, 40));
            fb.fill_rect(Rect::new(cx - 7, cy + 4, 12, 2), Pixel::rgb(220, 80, 60));
        }
        WindowContentType::SoftwareUpdater => {
            // Shield icon
            fb.fill_rounded_rect_aa(
                Rect::new(cx - 6, cy - 8, 12, 16),
                Pixel::rgb(60, 160, 80),
                3,
            );
            fb.fill_rect(Rect::new(cx - 3, cy - 3, 6, 2), icon_color);
            fb.fill_rect(Rect::new(cx - 1, cy - 5, 2, 6), icon_color);
        }
        WindowContentType::SoftwareCenter => {
            fb.fill_rounded_rect_aa(
                Rect::new(cx - 6, cy - 5, 12, 12),
                Pixel::rgb(74, 144, 217),
                3,
            );
            fb.fill_rect(Rect::new(cx - 2, cy - 8, 4, 4), Pixel::rgb(74, 144, 217));
        }
        WindowContentType::SetupWizard => {
            fb.fill_circle_aa(cx, cy, 8, Pixel::rgb(46, 204, 113));
            fb.fill_circle_aa(cx, cy, 3, icon_color);
        }
        WindowContentType::TaskManager => {
            // Bar chart icon
            fb.fill_rect(Rect::new(cx - 6, cy + 2, 4, 6), Pixel::rgb(82, 139, 255));
            fb.fill_rect(Rect::new(cx - 1, cy - 4, 4, 12), Pixel::rgb(60, 180, 100));
            fb.fill_rect(Rect::new(cx + 4, cy - 1, 4, 9), Pixel::rgb(230, 180, 40));
        }
        WindowContentType::Calculator => {
            // Calculator icon: grid dots
            fb.fill_rounded_rect_aa(Rect::new(cx - 8, cy - 8, 16, 16), icon_color, 3);
            fb.fill_rect(Rect::new(cx - 5, cy - 5, 3, 3), Pixel::new(40, 50, 60, 180));
            fb.fill_rect(Rect::new(cx + 2, cy - 5, 3, 3), Pixel::new(40, 50, 60, 180));
            fb.fill_rect(Rect::new(cx - 5, cy + 2, 3, 3), Pixel::new(40, 50, 60, 180));
            fb.fill_rect(Rect::new(cx + 2, cy + 2, 3, 3), Pixel::new(40, 50, 60, 180));
        }
        WindowContentType::ImageViewer => {
            // Picture icon: mountain in frame
            fb.draw_rounded_rect(Rect::new(cx - 8, cy - 6, 16, 12), icon_color, 2, 1);
            fb.fill_circle_aa(cx + 3, cy - 2, 2, Pixel::rgb(255, 220, 80));
            fb.draw_line_aa(cx - 6, cy + 4, cx - 1, cy - 1, Pixel::rgb(80, 180, 80));
            fb.draw_line_aa(cx - 1, cy - 1, cx + 5, cy + 4, Pixel::rgb(80, 180, 80));
        }
    }
}
