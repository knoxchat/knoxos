use alloc::format;
/// GUI Software Center — browse, install, update, and manage applications
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme;
use super::window::{self, WindowContentType, WindowId};

// ── Views ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwView {
    Featured,
    Categories,
    Installed,
    Updates,
    AppDetail,
}

// ── Categories ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCategory {
    Productivity,
    Internet,
    Media,
    Development,
    Games,
    System,
    Education,
    Graphics,
    Science,
    Accessibility,
}

impl AppCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Productivity => "Productivity",
            Self::Internet => "Internet",
            Self::Media => "Media",
            Self::Development => "Development",
            Self::Games => "Games",
            Self::System => "System",
            Self::Education => "Education",
            Self::Graphics => "Graphics",
            Self::Science => "Science",
            Self::Accessibility => "Accessibility",
        }
    }

    fn icon_color(self) -> Pixel {
        match self {
            Self::Productivity => Pixel::from_hex(0x4A90D9),
            Self::Internet => Pixel::from_hex(0x2ECC71),
            Self::Media => Pixel::from_hex(0xE74C3C),
            Self::Development => Pixel::from_hex(0xF39C12),
            Self::Games => Pixel::from_hex(0x9B59B6),
            Self::System => Pixel::from_hex(0x1ABC9C),
            Self::Education => Pixel::from_hex(0xE67E22),
            Self::Graphics => Pixel::from_hex(0xE91E63),
            Self::Science => Pixel::from_hex(0x3498DB),
            Self::Accessibility => Pixel::from_hex(0x27AE60),
        }
    }

    const ALL: [AppCategory; 10] = [
        Self::Productivity,
        Self::Internet,
        Self::Media,
        Self::Development,
        Self::Games,
        Self::System,
        Self::Education,
        Self::Graphics,
        Self::Science,
        Self::Accessibility,
    ];
}

// ── App listing ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct AppListing {
    name: &'static str,
    version: &'static str,
    description: &'static str,
    category: AppCategory,
    size_mb: u32,
    rating: u8, // 1-5
    downloads: u32,
    installed: bool,
    update_available: bool,
}

static DEFAULT_APPS: &[AppListing] = &[
    AppListing {
        name: "KnoxEditor",
        version: "1.2.0",
        description: "Powerful text and code editor with syntax highlighting",
        category: AppCategory::Productivity,
        size_mb: 12,
        rating: 5,
        downloads: 48200,
        installed: true,
        update_available: false,
    },
    AppListing {
        name: "KnoxBrowser",
        version: "3.1.0",
        description: "Fast, privacy-focused web browser",
        category: AppCategory::Internet,
        size_mb: 85,
        rating: 4,
        downloads: 122000,
        installed: true,
        update_available: true,
    },
    AppListing {
        name: "KnoxTerminal",
        version: "2.0.1",
        description: "Modern GPU-accelerated terminal emulator",
        category: AppCategory::System,
        size_mb: 8,
        rating: 5,
        downloads: 95300,
        installed: true,
        update_available: false,
    },
    AppListing {
        name: "PhotoEdit",
        version: "1.0.0",
        description: "Image editor with layers and filters",
        category: AppCategory::Graphics,
        size_mb: 45,
        rating: 4,
        downloads: 31200,
        installed: false,
        update_available: false,
    },
    AppListing {
        name: "MusicBox",
        version: "2.3.0",
        description: "Audio player with equalizer and playlists",
        category: AppCategory::Media,
        size_mb: 18,
        rating: 4,
        downloads: 67800,
        installed: false,
        update_available: false,
    },
    AppListing {
        name: "DevStudio",
        version: "4.0.0",
        description: "Integrated development environment for Rust, C, Python",
        category: AppCategory::Development,
        size_mb: 210,
        rating: 5,
        downloads: 28900,
        installed: false,
        update_available: false,
    },
    AppListing {
        name: "KnoxCalc",
        version: "1.1.0",
        description: "Scientific calculator with graphing",
        category: AppCategory::Education,
        size_mb: 3,
        rating: 3,
        downloads: 54100,
        installed: true,
        update_available: false,
    },
    AppListing {
        name: "SystemMonitor",
        version: "1.4.0",
        description: "CPU, memory, disk, and network monitoring",
        category: AppCategory::System,
        size_mb: 6,
        rating: 4,
        downloads: 41700,
        installed: true,
        update_available: true,
    },
    AppListing {
        name: "ChessMaster",
        version: "1.0.0",
        description: "Chess game with multiple AI difficulty levels",
        category: AppCategory::Games,
        size_mb: 24,
        rating: 4,
        downloads: 19800,
        installed: false,
        update_available: false,
    },
    AppListing {
        name: "SciPlot",
        version: "2.1.0",
        description: "Scientific data visualization and plotting",
        category: AppCategory::Science,
        size_mb: 32,
        rating: 4,
        downloads: 15600,
        installed: false,
        update_available: false,
    },
];

// ── State ────────────────────────────────────────────────────────────

struct SwState {
    window_id: WindowId,
    view: SwView,
    selected_category: Option<AppCategory>,
    detail_idx: Option<usize>,
    scroll_y: i32,
}

lazy_static! {
    static ref STATES: Mutex<Vec<SwState>> = Mutex::new(Vec::new());
}

// ── Public API ───────────────────────────────────────────────────────

pub fn open() {
    let mut win = window::Window::new("Software Center", 100, 60, 640, 500);
    win.content_type = WindowContentType::SoftwareCenter;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    STATES.lock().push(SwState {
        window_id: wid,
        view: SwView::Featured,
        selected_category: None,
        detail_idx: None,
        scroll_y: 0,
    });
    super::request_redraw();
}

pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, area: Rect, _scroll_y: i32) {
    let tc = theme::colors();
    let accent = colors::accent();
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let x0 = area.x;
    let y0 = area.y;

    // ── Tab bar ──
    fb.fill_rect(Rect::new(x0, y0, area.width, 32), tc.bg_surface);

    let tabs: &[(&str, SwView)] = &[
        ("Featured", SwView::Featured),
        ("Categories", SwView::Categories),
        ("Installed", SwView::Installed),
        ("Updates", SwView::Updates),
    ];

    let mut tx = x0 + 8;
    for &(label, view) in tabs {
        let tw = fonts::measure_string_width_compact(label, 1) as i32 + 16;
        let selected = state.view == view
            || (view == SwView::Categories
                && state.view == SwView::AppDetail
                && state.selected_category.is_some());
        if selected {
            fb.fill_rounded_rect_aa(Rect::new(tx, y0 + 4, tw as u32, 24), accent, 4);
            fonts::draw_string_compact(fb, tx + 8, y0 + 10, label, Pixel::rgb(255, 255, 255), 1);
        } else {
            fonts::draw_string_compact(fb, tx + 8, y0 + 10, label, tc.text_secondary, 1);
        }
        tx += tw + 4;
    }

    // ── Content area ──
    let content_y = y0 + 36;
    let content_h = area.height.saturating_sub(36);

    match state.view {
        SwView::Featured | SwView::Installed | SwView::Updates => {
            draw_app_list(fb, state, x0, content_y, area.width, content_h, &tc);
        }
        SwView::Categories => {
            draw_categories(fb, x0, content_y, area.width, content_h, &tc);
        }
        SwView::AppDetail => {
            draw_app_detail(fb, state, x0, content_y, area.width, content_h, &tc);
        }
    }
}

fn draw_app_list(
    fb: &mut FrameBuffer,
    state: &SwState,
    x0: i32,
    y0: i32,
    w: u32,
    h: u32,
    tc: &theme::ThemeColors,
) {
    let accent = colors::accent();
    let row_h: i32 = 54;

    let apps: Vec<&AppListing> = DEFAULT_APPS
        .iter()
        .filter(|a| match state.view {
            SwView::Installed => a.installed,
            SwView::Updates => a.update_available,
            SwView::Featured => true,
            _ => state.selected_category.is_none_or(|c| {
                core::mem::discriminant(&a.category) == core::mem::discriminant(&c)
            }),
        })
        .collect();

    for (i, app) in apps.iter().enumerate() {
        let ry = y0 + (i as i32 * row_h) - state.scroll_y;
        if ry + row_h < y0 || ry > y0 + h as i32 {
            continue;
        }

        // Row background (alternating)
        if i % 2 == 0 {
            fb.fill_rect(Rect::new(x0, ry, w, row_h as u32), tc.bg_surface);
        }

        // App icon circle
        fb.fill_circle_aa(x0 + 24, ry + row_h / 2, 16, app.category.icon_color());

        // Name + version
        fonts::draw_string_compact(fb, x0 + 50, ry + 6, app.name, tc.text_primary, 1);
        let vx = x0 + 50 + fonts::measure_string_width_compact(app.name, 1) as i32 + 8;
        fonts::draw_string_compact(fb, vx, ry + 8, app.version, tc.text_muted, 1);

        // Description
        fonts::draw_string_compact(fb, x0 + 50, ry + 22, app.description, tc.text_secondary, 1);

        // Rating stars
        let stars_x = x0 + 50;
        for s in 0..5u32 {
            let star_col = if s < app.rating as u32 {
                Pixel::from_hex(0xF1C40F)
            } else {
                tc.bg_tertiary
            };
            fb.fill_circle_aa(stars_x + (s as i32 * 12), ry + 38, 4, star_col);
        }

        // Size
        let size_str = format!("{} MB", app.size_mb);
        let sx = x0 + w as i32 - 140;
        fonts::draw_string_compact(fb, sx, ry + 6, &size_str, tc.text_muted, 1);

        // Install / Update button
        let btn_x = x0 + w as i32 - 80;
        if app.update_available {
            fb.fill_rounded_rect_aa(
                Rect::new(btn_x, ry + 12, 64, 24),
                Pixel::from_hex(0x2ECC71),
                4,
            );
            fonts::draw_string_compact(
                fb,
                btn_x + 8,
                ry + 17,
                "Update",
                Pixel::rgb(255, 255, 255),
                1,
            );
        } else if app.installed {
            fb.fill_rounded_rect_aa(Rect::new(btn_x, ry + 12, 64, 24), tc.bg_tertiary, 4);
            fonts::draw_string_compact(fb, btn_x + 4, ry + 17, "Installed", tc.text_secondary, 1);
        } else {
            fb.fill_rounded_rect_aa(Rect::new(btn_x, ry + 12, 64, 24), accent, 4);
            fonts::draw_string_compact(
                fb,
                btn_x + 8,
                ry + 17,
                "Install",
                Pixel::rgb(255, 255, 255),
                1,
            );
        }
    }
}

fn draw_categories(
    fb: &mut FrameBuffer,
    x0: i32,
    y0: i32,
    w: u32,
    _h: u32,
    tc: &theme::ThemeColors,
) {
    let cols = 3u32;
    let cell_w = (w - 32) / cols;
    let cell_h: u32 = 64;

    for (i, cat) in AppCategory::ALL.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        let cx = x0 + 12 + (col * cell_w) as i32;
        let cy = y0 + 8 + (row * (cell_h + 8)) as i32;

        fb.fill_rounded_rect_aa(Rect::new(cx, cy, cell_w - 8, cell_h), tc.bg_surface, 6);
        fb.fill_circle_aa(cx + 28, cy + 22, 12, cat.icon_color());

        fonts::draw_string_compact(fb, cx + 8, cy + 42, cat.label(), tc.text_primary, 1);

        let count = DEFAULT_APPS
            .iter()
            .filter(|a| core::mem::discriminant(&a.category) == core::mem::discriminant(cat))
            .count();
        let count_str = format!("{} apps", count);
        fonts::draw_string_compact(fb, cx + 8, cy + 52, &count_str, tc.text_muted, 1);
    }
}

fn draw_app_detail(
    fb: &mut FrameBuffer,
    state: &SwState,
    x0: i32,
    y0: i32,
    w: u32,
    _h: u32,
    tc: &theme::ThemeColors,
) {
    let accent = colors::accent();
    let idx = match state.detail_idx {
        Some(i) if i < DEFAULT_APPS.len() => i,
        _ => return,
    };
    let app = &DEFAULT_APPS[idx];

    // Large icon
    fb.fill_circle_aa(x0 + 48, y0 + 48, 32, app.category.icon_color());

    // Name & version
    fonts::draw_string_compact(fb, x0 + 96, y0 + 20, app.name, tc.text_primary, 2);
    fonts::draw_string_compact(fb, x0 + 96, y0 + 44, app.version, tc.text_secondary, 1);

    // Install / Remove button
    let btn_x = x0 + w as i32 - 110;
    if app.installed {
        fb.fill_rounded_rect_aa(
            Rect::new(btn_x, y0 + 24, 80, 28),
            Pixel::from_hex(0xE74C3C),
            6,
        );
        fonts::draw_string_compact(
            fb,
            btn_x + 12,
            y0 + 30,
            "Remove",
            Pixel::rgb(255, 255, 255),
            1,
        );
    } else {
        fb.fill_rounded_rect_aa(Rect::new(btn_x, y0 + 24, 80, 28), accent, 6);
        fonts::draw_string_compact(
            fb,
            btn_x + 12,
            y0 + 30,
            "Install",
            Pixel::rgb(255, 255, 255),
            1,
        );
    }

    // Stats
    let stats_y = y0 + 90;
    fb.fill_rect(Rect::new(x0, stats_y, w, 1), tc.border_subtle);

    let dl_str = format!("{} downloads", app.downloads);
    fonts::draw_string_compact(fb, x0 + 12, stats_y + 8, &dl_str, tc.text_secondary, 1);

    let size_str = format!("{} MB", app.size_mb);
    fonts::draw_string_compact(fb, x0 + 200, stats_y + 8, &size_str, tc.text_secondary, 1);

    // Description
    fb.fill_rect(Rect::new(x0, stats_y + 28, w, 1), tc.border_subtle);
    fonts::draw_string_compact(
        fb,
        x0 + 12,
        stats_y + 36,
        app.description,
        tc.text_primary,
        1,
    );
}

// ── Click handling ───────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, mx: i32, my: i32) {
    let area = {
        let wm = window::WINDOW_MANAGER.lock();
        match wm.windows.iter().find(|w| w.id == wid) {
            Some(w) => w.content_rect(),
            None => return,
        }
    };
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let y0 = area.y;

    // Tab clicks
    if my >= y0 && my <= y0 + 32 {
        let tab_views = [
            SwView::Featured,
            SwView::Categories,
            SwView::Installed,
            SwView::Updates,
        ];
        let mut tx = area.x + 8;
        for &view in &tab_views {
            let label = match view {
                SwView::Featured => "Featured",
                SwView::Categories => "Categories",
                SwView::Installed => "Installed",
                SwView::Updates => "Updates",
                _ => "",
            };
            let tw = fonts::measure_string_width_compact(label, 1) as i32 + 16;
            if mx >= tx && mx <= tx + tw {
                state.view = view;
                state.scroll_y = 0;
                state.detail_idx = None;
                state.selected_category = None;
                super::request_redraw();
                return;
            }
            tx += tw + 4;
        }
    }

    // Content area clicks
    let content_y = y0 + 36;

    if state.view == SwView::Categories {
        // Category grid click
        let cols = 3u32;
        let cell_w = (area.width - 32) / cols;
        let cell_h = 64u32 + 8;
        let rel_x = mx - area.x - 12;
        let rel_y = my - content_y - 8;
        if rel_x >= 0 && rel_y >= 0 {
            let col = (rel_x as u32) / cell_w;
            let row = (rel_y as u32) / cell_h;
            let idx = (row * cols + col) as usize;
            if col < cols && idx < AppCategory::ALL.len() {
                state.selected_category = Some(AppCategory::ALL[idx]);
                state.view = SwView::Featured;
                state.scroll_y = 0;
                super::request_redraw();
            }
        }
    } else if state.view == SwView::Featured
        || state.view == SwView::Installed
        || state.view == SwView::Updates
    {
        // App row click
        let row_h = 54;
        let rel_y = my - content_y + state.scroll_y;
        if rel_y >= 0 {
            let idx = (rel_y / row_h) as usize;
            if idx < DEFAULT_APPS.len() {
                state.detail_idx = Some(idx);
                state.view = SwView::AppDetail;
                super::request_redraw();
            }
        }
    }
}

pub fn handle_scroll(wid: WindowId, delta: i32) {
    let mut states = STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.scroll_y = (state.scroll_y - delta * 20).max(0);
        super::request_redraw();
    }
}
