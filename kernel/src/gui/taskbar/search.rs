/// Desktop search overlay (apps + common file paths).
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Desktop Search   (8.28)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Desktop search result
#[derive(Clone)]
pub struct DesktopSearchResult {
    pub name: String,
    pub path: String,
    /// "file", "app", "setting"
    pub kind: String,
}

/// Desktop search overlay state
struct DesktopSearch {
    visible: bool,
    query: String,
    results: Vec<DesktopSearchResult>,
    selected: usize,
}

static DESKTOP_SEARCH: Mutex<DesktopSearch> = Mutex::new(DesktopSearch {
    visible: false,
    query: String::new(),
    results: Vec::new(),
    selected: 0,
});

/// Open the desktop search overlay (e.g. Super key or hot corner)
pub fn open_desktop_search() {
    let mut ds = DESKTOP_SEARCH.lock();
    ds.visible = true;
    ds.query.clear();
    ds.results.clear();
    ds.selected = 0;
    crate::gui::request_redraw();
}

/// Close the desktop search overlay
pub fn close_desktop_search() {
    DESKTOP_SEARCH.lock().visible = false;
    crate::gui::request_redraw();
}

/// Is desktop search visible
pub fn is_desktop_search_visible() -> bool {
    DESKTOP_SEARCH.lock().visible
}

/// Type a character into the search query
pub fn desktop_search_input(ch: char) {
    let mut ds = DESKTOP_SEARCH.lock();
    if !ds.visible {
        return;
    }
    if ch == '\x08' {
        // Backspace
        ds.query.pop();
    } else if ch.is_ascii_graphic() || ch == ' ' {
        ds.query.push(ch);
    }
    ds.selected = 0;

    // Perform search
    let query_lower = ds.query.to_ascii_lowercase();
    if query_lower.is_empty() {
        ds.results.clear();
        return;
    }

    let mut results = Vec::new();

    // Search apps by name
    let app_names = [
        "Terminal",
        "Files",
        "Browser",
        "Settings",
        "Text Editor",
        "Calculator",
        "System Monitor",
        "Image Viewer",
        "Music Player",
    ];
    for name in &app_names {
        if name.to_ascii_lowercase().contains(&query_lower) {
            results.push(DesktopSearchResult {
                name: String::from(*name),
                path: String::new(),
                kind: String::from("app"),
            });
        }
    }

    // Search files in common paths
    let search_dirs = ["/home/user/Desktop", "/home/user/Documents"];
    for dir in &search_dirs {
        if let Ok(entries) = crate::file_manager::list_dir(dir) {
            for entry in entries {
                if entry.name.to_ascii_lowercase().contains(&query_lower) {
                    results.push(DesktopSearchResult {
                        name: entry.name.clone(),
                        path: alloc::format!("{}/{}", dir, entry.name),
                        kind: String::from("file"),
                    });
                }
            }
        }
    }

    ds.results = results;
}

/// Navigate desktop search results (delta: -1 = up, +1 = down)
pub fn desktop_search_navigate(delta: i32) {
    let mut ds = DESKTOP_SEARCH.lock();
    if ds.results.is_empty() {
        return;
    }
    let count = ds.results.len() as i32;
    ds.selected = ((ds.selected as i32 + delta).rem_euclid(count)) as usize;
}

/// Activate the selected search result
pub fn desktop_search_activate() -> Option<DesktopSearchResult> {
    let ds = DESKTOP_SEARCH.lock();
    if ds.results.is_empty() {
        return None;
    }
    let result = ds.results.get(ds.selected).cloned();
    drop(ds);
    close_desktop_search();
    result
}

/// Draw the desktop search overlay
pub fn draw_desktop_search(fb: &mut FrameBuffer) {
    let ds = DESKTOP_SEARCH.lock();
    if !ds.visible {
        return;
    }

    let sw = fb.width;
    let sh = fb.height;
    let popup_w = 480usize.min(sw - 40) as u32;
    let popup_h = 44 + ds.results.len().min(8) as u32 * 30 + 12;
    let px = (sw as i32 - popup_w as i32) / 2;
    let py = (sh as i32 / 3).max(60);

    // Dim background
    fb.fill_rect(
        Rect::new(0, 0, sw as u32, sh as u32),
        Pixel::new(0, 0, 0, 120),
    );

    // Search box
    fb.fill_rounded_rect_aa(
        Rect::new(px, py, popup_w, popup_h),
        Pixel::new(20, 24, 35, 240),
        12,
    );
    fb.draw_rounded_rect(
        Rect::new(px, py, popup_w, popup_h),
        Pixel::new(0, 150, 255, 120),
        12,
        2,
    );

    // Search icon & text field
    fb.fill_circle_aa(px + 22, py + 22, 8, Pixel::new(0, 150, 255, 180));
    let display_query = if ds.query.is_empty() {
        "Search files, apps, settings…"
    } else {
        &ds.query
    };
    let tc = if ds.query.is_empty() {
        Pixel::new(100, 110, 140, 180)
    } else {
        Pixel::new(220, 230, 255, 255)
    };
    font_engine::draw_ui_text(fb, px + 40, py + 14, display_query, 12, tc);

    // Results
    let mut iy = py + 44;
    for (i, result) in ds.results.iter().take(8).enumerate() {
        let selected = i == ds.selected;
        if selected {
            fb.fill_rounded_rect_aa(
                Rect::new(px + 4, iy, popup_w - 8, 28),
                Pixel::new(0, 100, 200, 50),
                4,
            );
        }
        let kind_color = match result.kind.as_str() {
            "app" => Pixel::new(0, 200, 100, 200),
            "setting" => Pixel::new(200, 160, 0, 200),
            _ => Pixel::new(100, 140, 200, 200),
        };
        // Kind badge
        font_engine::draw_ui_text(fb, px + 12, iy + 7, &result.kind, 11, kind_color);
        // Name
        let nc = if selected {
            Pixel::new(0, 220, 255, 255)
        } else {
            Pixel::new(200, 210, 230, 230)
        };
        font_engine::draw_ui_text(fb, px + 60, iy + 7, &result.name, 12, nc);
        iy += 30;
    }
}
