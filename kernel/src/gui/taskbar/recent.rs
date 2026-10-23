/// Recently launched applications, for Start Menu / launcher sections.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::window;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Recently Used Apps   (8.20)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Maximum number of recent apps to track
const MAX_RECENT_APPS: usize = 12;

/// A recently launched application entry
#[derive(Clone)]
pub struct RecentApp {
    pub name: String,
    pub content_type: window::WindowContentType,
    pub launch_tick: u64,
}

static RECENT_APPS: Mutex<Vec<RecentApp>> = Mutex::new(Vec::new());

/// Record that an app was launched (call when opening an application)
pub fn record_app_launch(name: &str, ct: window::WindowContentType, tick: u64) {
    let mut recent = RECENT_APPS.lock();
    // Remove existing entry for same content type to avoid duplicates
    recent.retain(|r| r.content_type != ct);
    recent.insert(
        0,
        RecentApp {
            name: String::from(name),
            content_type: ct,
            launch_tick: tick,
        },
    );
    if recent.len() > MAX_RECENT_APPS {
        recent.truncate(MAX_RECENT_APPS);
    }
}

/// Get list of recently used apps (most recent first)
pub fn recent_apps() -> Vec<RecentApp> {
    RECENT_APPS.lock().clone()
}

/// Draw the "Recent Apps" section inside the Start Menu / App Launcher
pub fn draw_recent_apps_section(fb: &mut FrameBuffer, x: i32, y: i32, w: u32) {
    let recents = RECENT_APPS.lock();
    if recents.is_empty() {
        return;
    }

    font_engine::draw_ui_text(fb, x + 8, y, "Recent", 11, Pixel::new(120, 130, 160, 200));

    let mut iy = y + 18;
    for app in recents.iter().take(6) {
        // Small icon placeholder + label
        fb.fill_rounded_rect_aa(
            Rect::new(x + 8, iy, 20, 20),
            Pixel::new(60, 70, 100, 150),
            4,
        );
        font_engine::draw_ui_text(
            fb,
            x + 34,
            iy + 4,
            &app.name,
            12,
            Pixel::new(200, 210, 230, 230),
        );
        iy += 26;
    }
}
