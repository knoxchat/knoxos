/// Desktop - Main desktop environment rendering
/// Draws the Aurora wallpaper, desktop icons, and coordinates all GUI elements
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use libm::{sinf, sqrtf};
use spin::Mutex;

use super::FRAMEBUFFER;
use super::colors;
use super::font_engine;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::icon_theme;
use super::icon_theme::IconCategory;
use super::icons;
use super::startmenu;
use super::taskbar;
use super::window;

/// Desktop icon entry
#[derive(Clone)]
pub struct DesktopIcon {
    pub name: String,
    pub icon_type: IconType,
    pub x: i32,
    pub y: i32,
    pub selected: bool,
    pub is_shortcut: bool,
    /// Filesystem path this icon represents (empty for shortcuts)
    pub path: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum IconType {
    MyPC,
    Folder,
    Document,
    Globe,
    Terminal,
    MediaPlayer,
    Game,
    AIBrain,
    Settings,
    Trash,
    Image,
    Archive,
    Script,
}

/// Grid layout constants — scaled for 1920×1080
const ICON_GRID_X: i32 = 28; // Left margin for desktop area
const ICON_GRID_Y: i32 = 18; // Top margin
const ICON_GRID_SPACING_X: i32 = 110; // Horizontal spacing between columns
const ICON_GRID_SPACING_Y: i32 = 100; // Vertical spacing — compact for 1080p
const ICON_WIDTH: u32 = 80; // gridEntryWidth
const ICON_HEIGHT: u32 = 78; // gridEntryHeight
const ICON_SIZE_ACTUAL: i32 = 48; // Actual icon size (48x48 pixels)
const TEXT_LABEL_MARGIN_TOP: i32 = 6; // Space between icon and text
const TEXT_LABEL_PADDING_LEFT: i32 = 10; // Left padding for text

/// Desktop right-click context menu item
#[derive(Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub separator: bool,
    pub action: ContextAction,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ContextAction {
    None,
    OpenTerminal,
    OpenBrowser,
    OpenExplorer,
    NewFile,
    NewFolder,
    Refresh,
    TakeScreenshot,
    TakeScreenshotRegion,
    ChangeWallpaper,
    DisplaySettings,
    Separator,
}

/// Desktop context menu state
pub struct DesktopContextMenu {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub items: Vec<ContextMenuItem>,
}

lazy_static::lazy_static! {
    pub static ref CONTEXT_MENU: Mutex<DesktopContextMenu> = Mutex::new(DesktopContextMenu {
        visible: false,
        x: 0,
        y: 0,
        items: vec![
            ContextMenuItem { label: String::from("Open Terminal"), separator: false, action: ContextAction::OpenTerminal },
            ContextMenuItem { label: String::from("Open Files"), separator: false, action: ContextAction::OpenExplorer },
            ContextMenuItem { label: String::from("Open Browser"), separator: false, action: ContextAction::OpenBrowser },
            ContextMenuItem { label: String::from(""), separator: true, action: ContextAction::Separator },
            ContextMenuItem { label: String::from("New File"), separator: false, action: ContextAction::NewFile },
            ContextMenuItem { label: String::from("New Folder"), separator: false, action: ContextAction::NewFolder },
            ContextMenuItem { label: String::from(""), separator: true, action: ContextAction::Separator },
            ContextMenuItem { label: String::from("Screenshot"), separator: false, action: ContextAction::TakeScreenshot },
            ContextMenuItem { label: String::from("Screenshot Region"), separator: false, action: ContextAction::TakeScreenshotRegion },
            ContextMenuItem { label: String::from("Change Wallpaper"), separator: false, action: ContextAction::ChangeWallpaper },
            ContextMenuItem { label: String::from("Display Settings"), separator: false, action: ContextAction::DisplaySettings },
            ContextMenuItem { label: String::from(""), separator: true, action: ContextAction::Separator },
            ContextMenuItem { label: String::from("Refresh Desktop"), separator: false, action: ContextAction::Refresh },
        ],
    });
}

/// The desktop state
pub struct Desktop {
    pub icons: Vec<DesktopIcon>,
    pub selected_icon: Option<usize>,
    /// Icon being dragged — index into `icons`
    pub dragging_icon: Option<usize>,
    /// Drag offset from icon origin to mouse position
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
    /// Current drag position (icon origin while dragging)
    pub drag_x: i32,
    pub drag_y: i32,
    /// Rubber band selection state
    pub rubber_band: Option<RubberBand>,
}

/// Rubber band selection rectangle state
#[derive(Clone, Copy)]
pub struct RubberBand {
    /// Start point (where mouse was pressed)
    pub start_x: i32,
    pub start_y: i32,
    /// Current point (where mouse is now)
    pub end_x: i32,
    pub end_y: i32,
}

lazy_static::lazy_static! {
    pub static ref DESKTOP: Mutex<Desktop> = Mutex::new(Desktop::new());
}

impl Default for Desktop {
    fn default() -> Self {
        Self::new()
    }
}

impl Desktop {
    pub fn new() -> Self {
        // Create desktop icons — a comprehensive set
        let icons = vec![
            DesktopIcon {
                name: String::from("Files"),
                icon_type: IconType::MyPC,
                x: ICON_GRID_X,
                y: ICON_GRID_Y,
                selected: false,
                is_shortcut: false,
                path: String::from("/home"),
            },
            DesktopIcon {
                name: String::from("Terminal"),
                icon_type: IconType::Terminal,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Browser"),
                icon_type: IconType::Globe,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 2,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Documents"),
                icon_type: IconType::Folder,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 3,
                selected: false,
                is_shortcut: false,
                path: String::from("/home/user/Documents"),
            },
            DesktopIcon {
                name: String::from("AI Assistant"),
                icon_type: IconType::AIBrain,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 4,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Settings"),
                icon_type: IconType::Settings,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 5,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Trash"),
                icon_type: IconType::Trash,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 6,
                selected: false,
                is_shortcut: false,
                path: String::from("/home/user/.trash"),
            },
        ];

        Self {
            icons,
            selected_icon: None,
            dragging_icon: None,
            drag_offset_x: 0,
            drag_offset_y: 0,
            drag_x: 0,
            drag_y: 0,
            rubber_band: None,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Desktop Folder Integration — ~/Desktop mapped to desktop icons
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const DESKTOP_DIR: &str = "/home/user/Desktop";
const TRASH_DIR: &str = "/home/user/.trash";

/// Determine icon type from a filesystem entry name
fn icon_type_for_file(name: &str) -> IconType {
    if name.ends_with('/') {
        return IconType::Folder;
    }
    let lower = {
        let mut s = String::new();
        for c in name.chars() {
            s.push(if c.is_ascii_uppercase() {
                (c as u8 + 32) as char
            } else {
                c
            });
        }
        s
    };
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".bmp")
        || lower.ends_with(".jpeg")
    {
        IconType::Image
    } else if lower.ends_with(".zip")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".7z")
    {
        IconType::Archive
    } else if lower.ends_with(".sh") || lower.ends_with(".py") || lower.ends_with(".rs") {
        IconType::Script
    } else {
        IconType::Document
    }
}

/// Sync desktop icons with the ~/Desktop directory.
/// Preserves pinned app shortcuts, adds filesystem entries.
pub fn sync_desktop_folder() {
    let entries = match crate::vfs::list_directory(DESKTOP_DIR) {
        Some(e) => e,
        None => {
            // Try to create the Desktop directory
            crate::vfs::ensure_directory(DESKTOP_DIR);
            return;
        }
    };

    let mut desktop = DESKTOP.lock();

    // Separate pinned shortcuts from filesystem icons
    let pinned: Vec<DesktopIcon> = desktop
        .icons
        .iter()
        .filter(|i| {
            i.is_shortcut || i.icon_type == IconType::MyPC || i.icon_type == IconType::Trash
        })
        .cloned()
        .collect();

    // Determine grid positions for filesystem entries (start after pinned icons)
    let pinned_count = pinned.len() as i32;
    let screen_h = super::screen_size().1 as i32;
    let max_rows = (screen_h - ICON_GRID_Y) / ICON_GRID_SPACING_Y;
    let max_rows = max_rows.max(4);

    let mut icons = pinned;
    for (i, entry_name) in entries.iter().enumerate() {
        // Skip . and ..
        if entry_name == "." || entry_name == ".." {
            continue;
        }
        // Check if already represented by a pinned icon
        if icons.iter().any(|ic| ic.name == *entry_name) {
            continue;
        }

        let slot = pinned_count + i as i32;
        let col = slot / max_rows;
        let row = slot % max_rows;

        let mut full_path = String::from(DESKTOP_DIR);
        full_path.push('/');
        full_path.push_str(entry_name);

        icons.push(DesktopIcon {
            name: String::from(entry_name.as_str()),
            icon_type: icon_type_for_file(entry_name),
            x: ICON_GRID_X + col * ICON_GRID_SPACING_X,
            y: ICON_GRID_Y + row * ICON_GRID_SPACING_Y,
            selected: false,
            is_shortcut: false,
            path: full_path,
        });
    }

    desktop.icons = icons;
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Recycle Bin / Trash
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Move a file to the trash directory instead of deleting permanently
pub fn move_to_trash(path: &str) -> Result<(), &'static str> {
    // Ensure trash dir exists
    crate::vfs::ensure_directory(TRASH_DIR);

    // Extract filename from path
    let filename = path.rsplit('/').next().unwrap_or(path);
    let mut trash_path = String::from(TRASH_DIR);
    trash_path.push('/');
    trash_path.push_str(filename);

    // Move file to trash
    crate::file_manager::rename(path, &trash_path).map_err(|_| "Failed to move to trash")
}

/// Restore a file from trash to the desktop directory
pub fn restore_from_trash(filename: &str) -> Result<(), &'static str> {
    let mut trash_path = String::from(TRASH_DIR);
    trash_path.push('/');
    trash_path.push_str(filename);

    let mut dest_path = String::from(DESKTOP_DIR);
    dest_path.push('/');
    dest_path.push_str(filename);

    crate::file_manager::rename(&trash_path, &dest_path).map_err(|_| "Failed to restore from trash")
}

/// Empty the trash (permanently delete all files in trash)
pub fn empty_trash() -> Result<u32, &'static str> {
    let entries = crate::vfs::list_directory(TRASH_DIR).ok_or("Trash not found")?;
    let mut count = 0u32;
    for entry in &entries {
        if entry == "." || entry == ".." {
            continue;
        }
        let mut path = String::from(TRASH_DIR);
        path.push('/');
        path.push_str(entry);
        if crate::vfs::remove_dispatch(&path).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

/// Get the number of items in trash
pub fn trash_count() -> u32 {
    match crate::vfs::list_directory(TRASH_DIR) {
        Some(entries) => entries.iter().filter(|e| *e != "." && *e != "..").count() as u32,
        None => 0,
    }
}

/// Cached wallpaper buffer — rendered once, then blit for subsequent frames
static WALLPAPER_CACHE: Mutex<Option<alloc::vec::Vec<u8>>> = Mutex::new(None);

/// Invalidate the wallpaper cache (call when wallpaper/resolution changes)
pub fn invalidate_wallpaper_cache() {
    *WALLPAPER_CACHE.lock() = None;
}

/// Invalidate the saved cursor background (call after resolution change)
pub fn invalidate_cursor_bg() {
    CURSOR_BG_VALID.store(false, core::sync::atomic::Ordering::Relaxed);
}

/// Draw the complete desktop (called at startup and on redraws).
///
/// Accepts a list of damage rects describing which regions changed.
/// Only pixels inside the union of those rects are recomposed and presented
/// to the HW framebuffer. This is the key optimization that keeps the cursor
/// responsive even while windows are being dragged or resized:
///
///  - Mouse-only movement uses the fast `update_cursor_only()` path (no compositing)
///  - Window drag pushes two small damage rects (old position + new position)
///  - Click/scroll pushes damage for just the affected widget/area
///  - Full redraws (resolution change, first paint) push a screen-sized rect
pub fn draw_desktop_damaged(damage: &[Rect]) {
    // Read mouse position BEFORE locking FRAMEBUFFER to avoid deadlock.
    let (mx, my) = {
        let mouse = super::input::MOUSE.lock();
        (mouse.x, mouse.y)
    };

    if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
        let screen_w = fb.width as u32;
        let screen_h = fb.height as u32;
        let screen_rect = Rect::new(0, 0, screen_w, screen_h);

        // ── 0. Restore old cursor pixels FIRST ──────────────────────
        // The cursor was drawn into the back buffer on the previous frame.
        // We MUST erase it before compositing, otherwise the old cursor
        // pixels get baked into the wallpaper cache reads and leave ghosts.
        let (old_cx, old_cy) = last_cursor_pos();
        let old_cursor_rect = Rect::new(
            old_cx - CURSOR_SAVE_PAD,
            old_cy - CURSOR_SAVE_PAD,
            CURSOR_SAVE_W as u32,
            CURSOR_SAVE_H as u32,
        );
        restore_cursor_background(fb);

        // New cursor rect for the position we'll draw at this frame
        let new_cursor_rect = Rect::new(
            mx - CURSOR_SAVE_PAD,
            my - CURSOR_SAVE_PAD,
            CURSOR_SAVE_W as u32,
            CURSOR_SAVE_H as u32,
        );

        // Compute the union bounding box of all damage rects, clamped to screen.
        // Include both old and new cursor positions so the HW framebuffer
        // is updated everywhere the cursor was or will be.
        let base_damage = if damage.is_empty() {
            screen_rect
        } else {
            let mut u = damage[0];
            for r in &damage[1..] {
                u = super::rect_union(u, *r);
            }
            u
        };
        let damage_union = clamp_rect_to_screen(
            super::rect_union(
                super::rect_union(base_damage, old_cursor_rect),
                new_cursor_rect,
            ),
            screen_w,
            screen_h,
        );

        // Check if this is a full-screen redraw (damage covers ≥90% of screen)
        let damage_area = damage_union.width as u64 * damage_union.height as u64;
        let screen_area = screen_w as u64 * screen_h as u64;
        let is_full = damage_area * 100 / screen_area.max(1) >= 90;

        // Update taskbar/dock hover state and auto-hide animation
        taskbar::update_hover(mx, my, screen_w, screen_h);
        taskbar::update_auto_hide(my, screen_h);
        if startmenu::is_visible() {
            startmenu::update_hover(mx, my);
        }

        // 1. Restore wallpaper in damaged region only
        {
            let mut cache = WALLPAPER_CACHE.lock();
            if cache.is_none() {
                // First time: render full wallpaper and cache it.
                // The cursor has already been erased above, so this is clean.
                draw_wallpaper(fb);
                *cache = Some(fb.buffer.clone());
            } else if let Some(ref cached) = *cache {
                if is_full {
                    // Full screen — single memcpy (fastest)
                    let len = fb.buffer.len().min(cached.len());
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            cached.as_ptr(),
                            fb.buffer.as_mut_ptr(),
                            len,
                        );
                    }
                } else {
                    // Partial — only copy rows within the damage union
                    let bpp = fb.bytes_per_pixel;
                    let pitch = fb.pitch;
                    let x0 = damage_union.x.max(0) as usize;
                    let y0 = damage_union.y.max(0) as usize;
                    let x1 = ((damage_union.x + damage_union.width as i32) as usize).min(fb.width);
                    let y1 =
                        ((damage_union.y + damage_union.height as i32) as usize).min(fb.height);
                    let row_bytes = (x1 - x0) * bpp;
                    for row in y0..y1 {
                        let off = row * pitch + x0 * bpp;
                        if off + row_bytes <= cached.len() && off + row_bytes <= fb.buffer.len() {
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    cached.as_ptr().add(off),
                                    fb.buffer.as_mut_ptr().add(off),
                                    row_bytes,
                                );
                            }
                        }
                    }
                }
            }
        }

        // 2-8: Compose all layers (icons, taskbar, windows, overlays)
        //
        // For partial redraws, push a clip so drawing outside the damage
        // region is skipped. Additionally, skip entire layers that don't
        // overlap the damage region at all — avoids function call overhead,
        // lock acquisitions, and loop iterations for unaffected layers.
        if !is_full {
            fb.push_clip(damage_union);
        }

        // Desktop widgets — rendered on wallpaper surface before icons/windows
        {
            let widget_area = super::desktop_widgets::bounding_rect(screen_w);
            if is_full || damage_union.intersects(&widget_area) {
                super::desktop_widgets::draw(fb);
            }
        }

        // Desktop icons — only if damage overlaps the icon area (top-left region)
        {
            let icon_area = Rect::new(0, 0, 200, screen_h);
            if is_full || damage_union.intersects(&icon_area) {
                draw_desktop_icons(fb);
            }
        }

        // Rubber band selection overlay
        draw_rubber_band(fb);

        // Taskbar — only if damage overlaps the taskbar strip at screen bottom
        {
            let taskbar_y = screen_h as i32 - super::scale::taskbar_height() as i32;
            let taskbar_area = Rect::new(0, taskbar_y, screen_w, super::scale::taskbar_height());
            if is_full || damage_union.intersects(&taskbar_area) {
                taskbar::draw_taskbar(fb);
            }
        }

        // Windows — draw_all now does per-window clip intersection checks internally
        window::WINDOW_MANAGER.lock().draw_all(fb);

        // Snap preview — only while dragging
        draw_snap_preview(fb, mx, my);

        // Alt+Tab overlay — drawn above all windows
        if super::alt_tab::is_visible() {
            super::alt_tab::draw(fb);
        }

        // Exposé / Mission Control overlay — drawn above all windows
        if super::expose::is_visible() {
            super::expose::draw(fb);
        }

        // Overlays — only if damage overlaps their approximate regions or if visible
        if is_full || startmenu::is_visible() {
            startmenu::draw_start_menu(fb);
        }
        {
            let cm = CONTEXT_MENU.lock();
            if cm.visible {
                let cm_rect = Rect::new(cm.x, cm.y, CONTEXT_MENU_WIDTH, 200);
                if is_full || damage_union.intersects(&cm_rect) {
                    drop(cm);
                    draw_context_menu(fb);
                } else {
                    drop(cm);
                }
            }
        }
        if is_full || super::popups::any_popup_open() {
            super::popups::draw_calendar(fb);
            super::popups::draw_volume_popup(fb);
            super::popups::draw_quick_settings(fb);
        }
        if is_full || super::system_tray::is_context_menu_open() {
            super::system_tray::draw_context_menu(fb);
        }
        if is_full || super::taskbar::is_context_menu_open() {
            super::taskbar::draw_context_menu(fb);
        }
        if is_full || super::taskbar::is_preview_visible() {
            super::taskbar::draw_window_preview(fb);
        }
        if is_full
            || super::notifications::NOTIFICATIONS
                .lock()
                .has_visible_toasts()
        {
            super::notifications::draw_toasts(fb);
        }
        if is_full || super::notifications::is_panel_open() {
            super::notifications::draw_notification_panel(fb);
        }

        // File picker modal overlay — drawn above everything except cursor
        if is_full || super::file_picker::is_visible() {
            super::file_picker::draw(fb);
        }

        // Keyboard shortcuts overlay
        if is_full || super::shortcuts_overlay::is_visible() {
            super::shortcuts_overlay::draw(fb);
        }

        // Tick system sounds (turns off speaker after tone duration)
        super::sounds::tick();

        if !is_full {
            fb.pop_clip();
        }

        // 9. Cursor — save background (clean, cursor-free), then draw cursor on top.
        update_cursor_for_position(mx, my);
        save_cursor_background(fb, mx, my);
        draw_cursor(fb, mx, my);
        set_last_cursor_pos(mx, my);

        // 9.5 Screen magnification (17.9) — Zoom lens around cursor
        if super::accessibility::is_zoomed() {
            apply_zoom_lens(fb, mx, my);
        }

        // 9.6 Night light — warm color temperature shift (blue light filter)
        if super::night_light::is_active() {
            super::night_light::apply(fb, damage_union);
        }

        // 10. Present the damaged region (which now includes old + new cursor area)
        // to HW framebuffer.
        fb.present_rect(
            damage_union.x,
            damage_union.y,
            damage_union.width,
            damage_union.height,
        );
    }
}

/// Legacy full-screen redraw (calls damage-based path with full screen rect).
pub fn draw_desktop() {
    let damage = super::take_damage();
    if damage.is_empty() {
        // No damage rects queued — full screen
        let (sw, sh) = super::cached_screen_size();
        draw_desktop_damaged(&[Rect::new(0, 0, sw as u32, sh as u32)]);
    } else {
        draw_desktop_damaged(&damage);
    }
}

/// Clamp a rect to screen bounds.
fn clamp_rect_to_screen(r: Rect, sw: u32, sh: u32) -> Rect {
    let x0 = r.x.max(0).min(sw as i32);
    let y0 = r.y.max(0).min(sh as i32);
    let x1 = (r.x + r.width as i32).max(0).min(sw as i32);
    let y1 = (r.y + r.height as i32).max(0).min(sh as i32);
    Rect::new(x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}

/// Draw the desktop wallpaper — "Aurora" design
/// Warm gradient with soft, organic blob shapes and gentle color washes
/// for a sophisticated, human-centered feel
fn draw_wallpaper(fb: &mut FrameBuffer) {
    let w = fb.width;
    let h = fb.height;

    // ══════════════════════════════════════════════════════════
    // BASE: Warm vertical gradient (dark charcoal to deep plum)
    // ══════════════════════════════════════════════════════════
    let top = Pixel::rgb(22, 20, 26); // Warm charcoal with purple tint
    let bottom = Pixel::rgb(14, 12, 16); // Deep warm black
    fb.fill_gradient_v(Rect::new(0, 0, w as u32, h as u32), top, bottom);

    // Subtle warm diagonal wash for depth
    for py in 0..h {
        for px in 0..w {
            let diag = ((px + py) as f32 / ((w + h) as f32)) * 255.0;
            let alpha = (diag * 0.035) as u8;
            fb.blend_pixel(px, py, Pixel::new(40, 28, 32, alpha));
        }
    }

    // ══════════════════════════════════════════════════════════
    // ORGANIC BLOBS — Soft color washes with warm tones
    // ══════════════════════════════════════════════════════════

    // Blob 1: Large coral glow (center-right)
    {
        let cx = (w * 7 / 10) as i32;
        let cy = (h * 3 / 10) as i32;
        let radius = (w / 5) as i32;

        for py in (cy - radius).max(0)..(cy + radius).min(h as i32) {
            for px in (cx - radius).max(0)..(cx + radius).min(w as i32) {
                let dx = (px - cx) as f32;
                let dy = (py - cy) as f32;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (radius * radius) as f32;
                if dist_sq < r_sq {
                    let t = 1.0 - (dist_sq / r_sq);
                    let alpha = (t * t * t * 28.0) as u8;
                    if alpha > 0 {
                        fb.blend_pixel(px as usize, py as usize, Pixel::new(232, 121, 100, alpha));
                    }
                }
            }
        }
    }

    // Blob 2: Soft lavender glow (upper-left)
    {
        let cx = (w / 5) as i32;
        let cy = (h / 4) as i32;
        let radius = (w / 7) as i32;

        for py in (cy - radius).max(0)..(cy + radius).min(h as i32) {
            for px in (cx - radius).max(0)..(cx + radius).min(w as i32) {
                let dx = (px - cx) as f32;
                let dy = (py - cy) as f32;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (radius * radius) as f32;
                if dist_sq < r_sq {
                    let t = 1.0 - (dist_sq / r_sq);
                    let alpha = (t * t * t * 22.0) as u8;
                    if alpha > 0 {
                        fb.blend_pixel(px as usize, py as usize, Pixel::new(160, 130, 200, alpha));
                    }
                }
            }
        }
    }

    // Blob 3: Warm peach wash (bottom-left, large and diffuse)
    {
        let cx = (w / 4) as i32;
        let cy = (h * 3 / 4) as i32;
        let radius_x = (w / 3) as i32;
        let radius_y = (h / 3) as i32;
        let x0 = (cx - radius_x).max(0);
        let y0 = (cy - radius_y).max(0);
        let x1 = (cx + radius_x).min(w as i32);
        let y1 = (cy + radius_y).min(h as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = (px - cx) as f32 / radius_x as f32;
                let dy = (py - cy) as f32 / radius_y as f32;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq >= 1.0 {
                    continue;
                }
                let t = 1.0 - dist_sq;
                let alpha = (t * t * 10.0) as u8;
                if alpha > 0 {
                    fb.blend_pixel(px as usize, py as usize, Pixel::new(200, 130, 100, alpha));
                }
            }
        }
    }

    // Blob 4: Faint sage green (right side, mid-height)
    {
        let cx = (w * 4 / 5) as i32;
        let cy = (h * 6 / 10) as i32;
        let radius = (w / 10) as i32;

        for py in (cy - radius).max(0)..(cy + radius).min(h as i32) {
            for px in (cx - radius).max(0)..(cx + radius).min(w as i32) {
                let dx = (px - cx) as f32;
                let dy = (py - cy) as f32;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (radius * radius) as f32;
                if dist_sq < r_sq {
                    let t = 1.0 - (dist_sq / r_sq);
                    let alpha = (t * t * t * 16.0) as u8;
                    if alpha > 0 {
                        fb.blend_pixel(px as usize, py as usize, Pixel::new(130, 180, 150, alpha));
                    }
                }
            }
        }
    }

    // ── Subtle warm noise texture for organic feel ──────────────────
    // Sparse warm-tinted dots for a gentle grain
    {
        let warm_dot = Pixel::new(180, 160, 140, 12);
        let dim_dot = Pixel::new(140, 120, 110, 8);
        let mut seed: u32 = 0xCAFE_BABE;
        for _ in 0..90 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let sx = (seed % w as u32) as usize;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let sy = (seed % h as u32) as usize;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let bright = (seed % 3) == 0;
            if sx < w && sy < h {
                fb.blend_pixel(sx, sy, if bright { warm_dot } else { dim_dot });
            }
        }
    }

    // ── Aurora wave — a soft horizontal color wave across the top ────
    {
        let wave_h = h / 3;
        for py in 0..wave_h {
            let t = py as f32 / wave_h as f32;
            let alpha = ((1.0 - t) * (1.0 - t) * 12.0) as u8;
            if alpha == 0 {
                continue;
            }
            for px in 0..w {
                let wave_offset = sinf(px as f32 * 0.005 + py as f32 * 0.008) * 0.5 + 0.5;
                let r = (200.0 + wave_offset * 32.0) as u8;
                let g = (110.0 + wave_offset * 30.0) as u8;
                let b = (130.0 + wave_offset * 50.0) as u8;
                fb.blend_pixel(px, py, Pixel::new(r, g, b, alpha));
            }
        }
    }
}

/// Draw all desktop icons
fn draw_desktop_icons(fb: &mut FrameBuffer) {
    let desktop = DESKTOP.lock();

    for (i, icon) in desktop.icons.iter().enumerate() {
        // If this icon is being dragged, draw a dim ghost at original position
        // and draw the actual icon at the drag position
        let is_dragging = desktop.dragging_icon == Some(i);

        if is_dragging {
            // Ghost at original position (30% opacity via dimmer color)
            let ghost_alpha = Pixel::new(100, 90, 80, 40);
            fb.fill_rounded_rect_aa(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                ghost_alpha,
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                Pixel::new(200, 140, 120, 50),
                6,
                1,
            );
            continue; // Draw the dragging icon below
        }

        // Selection/focus background with rounded corners
        if icon.selected {
            fb.fill_rounded_rect_aa(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                colors::ICON_BG_FOCUSED,
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT),
                colors::ICON_BORDER_FOCUSED,
                6,
                1,
            );
        }

        // Draw the 48×48 icon graphic centered in the cell
        let ix = icon.x + (ICON_WIDTH as i32 - icons::ICON_SIZE as i32) / 2;
        let iy = icon.y + 2;

        draw_icon_graphic(fb, ix, iy, icon.icon_type);

        // Draw icon label with text shadow (font_engine)
        draw_icon_label_fe(fb, icon.x, icon.y, &icon.name);
    }

    // Draw the dragging icon on top of everything (at drag position)
    if let Some(drag_idx) = desktop.dragging_icon {
        if let Some(icon) = desktop.icons.get(drag_idx) {
            let dx = desktop.drag_x;
            let dy = desktop.drag_y;

            // Floating card with glow
            fb.fill_rounded_rect_aa(
                Rect::new(dx, dy, ICON_WIDTH, ICON_HEIGHT),
                Pixel::new(200, 130, 110, 45),
                6,
            );
            fb.draw_rounded_rect(
                Rect::new(dx, dy, ICON_WIDTH, ICON_HEIGHT),
                Pixel::new(232, 160, 140, 100),
                6,
                1,
            );

            // Icon graphic
            let ix = dx + (ICON_WIDTH as i32 - icons::ICON_SIZE as i32) / 2;
            let iy = dy + 2;
            draw_icon_graphic(fb, ix, iy, icon.icon_type);

            // Label (font_engine)
            draw_icon_label_fe(fb, dx, dy, &icon.name);
        }
    }
}

/// Map desktop IconType to icon_theme (category, name) pairs
pub(crate) fn icon_type_theme(icon_type: IconType) -> (IconCategory, &'static str) {
    match icon_type {
        IconType::MyPC => (IconCategory::Devices, "computer"),
        IconType::Folder => (IconCategory::Places, "folder"),
        IconType::Document => (IconCategory::Mimetypes, "text-x-generic"),
        IconType::Globe => (IconCategory::Apps, "web-browser"),
        IconType::Terminal => (IconCategory::Apps, "utilities-x-terminal"),
        IconType::MediaPlayer => (IconCategory::Apps, "multimedia-video-player"),
        IconType::Game => (IconCategory::Categories, "applications-games"),
        IconType::AIBrain => (IconCategory::Apps, "preferences-system"),
        IconType::Settings => (IconCategory::Apps, "org.gnome.Settings"),
        IconType::Trash => (IconCategory::Places, "user-trash"),
        IconType::Image => (IconCategory::Mimetypes, "image-x-generic"),
        IconType::Archive => (IconCategory::Mimetypes, "application-zip"),
        IconType::Script => (IconCategory::Mimetypes, "text-x-script"),
    }
}

/// Helper: draw the icon graphic by type using icon_theme (48×48)
fn draw_icon_graphic(fb: &mut FrameBuffer, ix: i32, iy: i32, icon_type: IconType) {
    let (category, name) = icon_type_theme(icon_type);
    icon_theme::draw_desktop_icon(fb, ix, iy, category, name);
}

/// Draw a centered icon label below the icon using font_engine with shadow
fn draw_icon_label_fe(fb: &mut FrameBuffer, icon_x: i32, icon_y: i32, text: &str) {
    const LABEL_SIZE: u16 = 11;
    let tw = font_engine::measure_ui_text(text, LABEL_SIZE) as i32;
    let cx = icon_x + (ICON_WIDTH as i32 - tw) / 2;
    let cy = icon_y + ICON_SIZE_ACTUAL + TEXT_LABEL_MARGIN_TOP;
    // Shadow
    font_engine::draw_ui_text(
        fb,
        cx + 1,
        cy + 1,
        text,
        LABEL_SIZE,
        colors::ICON_TEXT_SHADOW,
    );
    // Text
    font_engine::draw_ui_text(fb, cx, cy, text, LABEL_SIZE, colors::ICON_TEXT);
}

impl RubberBand {
    /// Get the normalized (top-left origin) rectangle
    pub fn to_rect(&self) -> Rect {
        let x0 = self.start_x.min(self.end_x);
        let y0 = self.start_y.min(self.end_y);
        let x1 = self.start_x.max(self.end_x);
        let y1 = self.start_y.max(self.end_y);
        Rect::new(x0, y0, (x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32)
    }
}

/// Draw the rubber band selection rectangle overlay
fn draw_rubber_band(fb: &mut FrameBuffer) {
    let desktop = DESKTOP.lock();
    if let Some(rb) = desktop.rubber_band {
        let rect = rb.to_rect();
        // Semi-transparent fill
        fb.fill_rounded_rect_aa(rect, Pixel::new(232, 121, 100, 25), 2);
        // Warm border
        fb.draw_rounded_rect(rect, Pixel::new(232, 140, 120, 120), 2, 1);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SNAP PREVIEW — visual indicator when dragging window near screen edges
// ═══════════════════════════════════════════════════════════════════════

/// Draw snap preview overlay when a window is being dragged near edges
fn draw_snap_preview(fb: &mut FrameBuffer, mx: i32, my: i32) {
    let wm = window::WINDOW_MANAGER.lock();

    // Only show when actively dragging a window
    if !wm.any_dragging() {
        return;
    }

    let w = fb.width as u32;
    let h = fb.height as u32 - super::scale::taskbar_height();
    let snap_zone = 12; // pixels from edge to trigger preview
    let corner_zone = 48; // corner detection extends further

    // Semi-transparent blue overlay with rounded corner effect
    let preview_fill = Pixel::new(0, 106, 230, 45);
    let preview_border = Pixel::new(0, 106, 230, 140);
    let margin = 6i32;

    let hw = w / 2;
    let hh = h / 2;

    // Corner zones (priority over edge zones)
    if mx <= snap_zone && my <= corner_zone {
        // Top-left quarter snap preview
        let r = super::framebuffer::Rect::new(
            margin,
            margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx >= fb.width as i32 - snap_zone && my <= corner_zone {
        // Top-right quarter snap preview
        let r = super::framebuffer::Rect::new(
            hw as i32 + margin,
            margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx <= snap_zone && my >= h as i32 - corner_zone {
        // Bottom-left quarter snap preview
        let r = super::framebuffer::Rect::new(
            margin,
            hh as i32 + margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx >= fb.width as i32 - snap_zone && my >= h as i32 - corner_zone {
        // Bottom-right quarter snap preview
        let r = super::framebuffer::Rect::new(
            hw as i32 + margin,
            hh as i32 + margin,
            hw - margin as u32 * 2,
            hh - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx <= snap_zone {
        // Left half snap preview
        let r = super::framebuffer::Rect::new(
            margin,
            margin,
            hw - margin as u32 * 2,
            h - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if mx >= fb.width as i32 - snap_zone {
        // Right half snap preview
        let r = super::framebuffer::Rect::new(
            hw as i32 + margin,
            margin,
            hw - margin as u32 * 2,
            h - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    } else if my <= snap_zone {
        // Maximize preview
        let r = super::framebuffer::Rect::new(
            margin,
            margin,
            w - margin as u32 * 2,
            h - margin as u32 * 2,
        );
        fb.fill_rounded_rect_aa(r, preview_fill, 8);
        fb.draw_rounded_rect(r, preview_border, 8, 2);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CURSOR — fast overlay with background save/restore
// ═══════════════════════════════════════════════════════════════════════

/// Cursor sprite dimensions (just the arrow shape)
pub const CURSOR_W: usize = 16;
pub const CURSOR_H: usize = 20;

// ─── Cursor Theme System ─────────────────────────────────────────────
/// Cursor visual style
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CursorTheme {
    Default = 0, // White cursor with black border
    Dark = 1,    // Black cursor with white border
    Accent = 2,  // Theme accent color cursor
    Large = 3,   // Larger white cursor for accessibility
}

static CURSOR_THEME: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

pub fn set_cursor_theme(theme: CursorTheme) {
    CURSOR_THEME.store(theme as u8, core::sync::atomic::Ordering::Relaxed);
}

pub fn cursor_theme() -> CursorTheme {
    match CURSOR_THEME.load(core::sync::atomic::Ordering::Relaxed) {
        1 => CursorTheme::Dark,
        2 => CursorTheme::Accent,
        3 => CursorTheme::Large,
        _ => CursorTheme::Default,
    }
}

/// Get cursor fill and border colors based on theme
pub fn cursor_colors() -> (Pixel, Pixel) {
    match cursor_theme() {
        CursorTheme::Default | CursorTheme::Large => {
            (Pixel::rgb(255, 255, 255), Pixel::rgb(0, 0, 0))
        }
        CursorTheme::Dark => (Pixel::rgb(0, 0, 0), Pixel::rgb(255, 255, 255)),
        CursorTheme::Accent => {
            let accent = super::theme::accent_color();
            (accent, Pixel::rgb(0, 0, 0))
        }
    }
}

/// Full-featured cursor type matching egui::CursorIcon (35 variants)
#[derive(Clone, Copy, PartialEq)]
pub enum CursorType {
    /// Normal arrow cursor
    Default, // 0
    /// Show no cursor
    None, // 1
    /// A context menu is available
    ContextMenu, // 2
    /// Question mark / help
    Help, // 3
    /// Pointing hand for links
    PointingHand, // 4
    /// Processing but still interactive
    Progress, // 5
    /// Not yet ready, try later (hourglass/spinner)
    Wait, // 6
    /// Hover a cell in a table
    Cell, // 7
    /// Precision crosshair
    Crosshair, // 8
    /// Text caret (I-beam)
    Text, // 9
    /// Vertical text caret
    VerticalText, // 10
    /// Alias / shortcut
    Alias, // 11
    /// Copy indicator
    Copy, // 12
    /// Omnidirectional move (arrows in all directions)
    Move, // 13
    /// Can't drop here
    NoDrop, // 14
    /// Forbidden / not allowed
    NotAllowed, // 15
    /// The thing can be grabbed
    Grab, // 16
    /// You are grabbing
    Grabbing, // 17
    /// Something can be scrolled in any direction
    AllScroll, // 18
    /// Horizontal resize ↔
    ResizeHorizontal, // 19
    /// Diagonal resize ↗↙ (NE-SW)
    ResizeNeSw, // 20
    /// Diagonal resize ↘↖ (NW-SE)
    ResizeNwSe, // 21
    /// Vertical resize ↕
    ResizeVertical, // 22
    /// Resize East →
    ResizeEast, // 23
    /// Resize South-East ↘
    ResizeSouthEast, // 24
    /// Resize South ↓
    ResizeSouth, // 25
    /// Resize South-West ↙
    ResizeSouthWest, // 26
    /// Resize West ←
    ResizeWest, // 27
    /// Resize North-West ↖
    ResizeNorthWest, // 28
    /// Resize North ↑
    ResizeNorth, // 29
    /// Resize North-East ↗
    ResizeNorthEast, // 30
    /// Resize column (left-right with vertical bars)
    ResizeColumn, // 31
    /// Resize row (up-down with horizontal bars)
    ResizeRow, // 32
    /// Zoom in (+)
    ZoomIn, // 33
    /// Zoom out (−)
    ZoomOut, // 34
}

/// Current cursor type (determined by hover context)
static CURSOR_TYPE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0); // 0 = Default

pub fn set_cursor_type(ct: CursorType) {
    let val = match ct {
        CursorType::Default => 0,
        CursorType::None => 1,
        CursorType::ContextMenu => 2,
        CursorType::Help => 3,
        CursorType::PointingHand => 4,
        CursorType::Progress => 5,
        CursorType::Wait => 6,
        CursorType::Cell => 7,
        CursorType::Crosshair => 8,
        CursorType::Text => 9,
        CursorType::VerticalText => 10,
        CursorType::Alias => 11,
        CursorType::Copy => 12,
        CursorType::Move => 13,
        CursorType::NoDrop => 14,
        CursorType::NotAllowed => 15,
        CursorType::Grab => 16,
        CursorType::Grabbing => 17,
        CursorType::AllScroll => 18,
        CursorType::ResizeHorizontal => 19,
        CursorType::ResizeNeSw => 20,
        CursorType::ResizeNwSe => 21,
        CursorType::ResizeVertical => 22,
        CursorType::ResizeEast => 23,
        CursorType::ResizeSouthEast => 24,
        CursorType::ResizeSouth => 25,
        CursorType::ResizeSouthWest => 26,
        CursorType::ResizeWest => 27,
        CursorType::ResizeNorthWest => 28,
        CursorType::ResizeNorth => 29,
        CursorType::ResizeNorthEast => 30,
        CursorType::ResizeColumn => 31,
        CursorType::ResizeRow => 32,
        CursorType::ZoomIn => 33,
        CursorType::ZoomOut => 34,
    };
    CURSOR_TYPE.store(val, core::sync::atomic::Ordering::Relaxed);
}

pub fn get_cursor_type() -> CursorType {
    match CURSOR_TYPE.load(core::sync::atomic::Ordering::Relaxed) {
        0 => CursorType::Default,
        1 => CursorType::None,
        2 => CursorType::ContextMenu,
        3 => CursorType::Help,
        4 => CursorType::PointingHand,
        5 => CursorType::Progress,
        6 => CursorType::Wait,
        7 => CursorType::Cell,
        8 => CursorType::Crosshair,
        9 => CursorType::Text,
        10 => CursorType::VerticalText,
        11 => CursorType::Alias,
        12 => CursorType::Copy,
        13 => CursorType::Move,
        14 => CursorType::NoDrop,
        15 => CursorType::NotAllowed,
        16 => CursorType::Grab,
        17 => CursorType::Grabbing,
        18 => CursorType::AllScroll,
        19 => CursorType::ResizeHorizontal,
        20 => CursorType::ResizeNeSw,
        21 => CursorType::ResizeNwSe,
        22 => CursorType::ResizeVertical,
        23 => CursorType::ResizeEast,
        24 => CursorType::ResizeSouthEast,
        25 => CursorType::ResizeSouth,
        26 => CursorType::ResizeSouthWest,
        27 => CursorType::ResizeWest,
        28 => CursorType::ResizeNorthWest,
        29 => CursorType::ResizeNorth,
        30 => CursorType::ResizeNorthEast,
        31 => CursorType::ResizeColumn,
        32 => CursorType::ResizeRow,
        33 => CursorType::ZoomIn,
        34 => CursorType::ZoomOut,
        _ => CursorType::Default,
    }
}

/// Check if a position is over any desktop icon
pub fn is_over_desktop_icon(mx: i32, my: i32) -> bool {
    let desktop = DESKTOP.lock();
    for icon in desktop.icons.iter() {
        let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
        if icon_rect.contains(mx, my) {
            return true;
        }
    }
    false
}

/// Update cursor type based on current mouse position and window state
pub fn update_cursor_for_position(mx: i32, my: i32) {
    use window::ResizeEdge;

    let wm = window::WINDOW_MANAGER.lock();

    // If actively dragging, show move cursor
    if wm.any_dragging() {
        drop(wm);
        set_cursor_type(CursorType::Grabbing);
        return;
    }

    // If actively resizing, keep the resize cursor
    if wm.any_resizing() {
        // Don't change — the resize cursor was already set
        return;
    }

    // Check for title bar button hover FIRST (before resize edges).
    // Buttons sit in the top-right corner where resize edges overlap.
    // Without this priority, the cursor shows resize arrows over buttons.
    if let Some(wid) = wm.window_at(mx, my) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            let on_close = win.closeable && win.close_button_rect().contains(mx, my);
            let on_max = win.maximizable && win.maximize_button_rect().contains(mx, my);
            let on_min = win.minimizable && win.minimize_button_rect().contains(mx, my);
            if on_close || on_max || on_min {
                drop(wm);
                set_cursor_type(CursorType::PointingHand);
                return;
            }
        }
    }

    // Check for resize edge hover
    let (_, edge) = wm.resize_edge_at(mx, my);
    drop(wm);

    let ct = match edge {
        ResizeEdge::Left => CursorType::ResizeWest,
        ResizeEdge::Right => CursorType::ResizeEast,
        ResizeEdge::Top => CursorType::ResizeNorth,
        ResizeEdge::Bottom => CursorType::ResizeSouth,
        ResizeEdge::TopLeft => CursorType::ResizeNorthWest,
        ResizeEdge::BottomRight => CursorType::ResizeSouthEast,
        ResizeEdge::TopRight => CursorType::ResizeNorthEast,
        ResizeEdge::BottomLeft => CursorType::ResizeSouthWest,
        ResizeEdge::None => {
            // Check if hovering over a desktop icon — show hand cursor
            if is_over_desktop_icon(mx, my) {
                CursorType::PointingHand
            } else {
                CursorType::Default
            }
        }
    };
    set_cursor_type(ct);
}

/// Save/restore area dimensions.
/// Must cover the largest cursor footprint across ALL cursor types:
///  - Arrow/Hand: 16×20 shape + 1px shadow = 17×21 starting at (x, y)
///  - Resize cursors: ±7 pixels centered on (x, y) = 15×15 centered
///  - Zoom cursors: ~16×16 + lens radius starting at (x, y)
///  - Move/AllScroll: ±8 centered = 17×17 centered
///    We use a generous area with a negative offset so it covers everything.
pub const CURSOR_SAVE_PAD: i32 = 10; // pixels before the cursor position
pub const CURSOR_SAVE_W: usize = 32; // total save width
pub const CURSOR_SAVE_H: usize = 34; // total save height

/// Saved pixels underneath the cursor (CURSOR_SAVE_W × CURSOR_SAVE_H × 4 bytes BGRA)
static CURSOR_BG: Mutex<[u8; CURSOR_SAVE_W * CURSOR_SAVE_H * 4]> =
    Mutex::new([0u8; CURSOR_SAVE_W * CURSOR_SAVE_H * 4]);

/// Whether we have a valid saved background
static CURSOR_BG_VALID: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Last saved area origin (NOT the cursor position — offset by CURSOR_SAVE_PAD)
static LAST_SAVE_X: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
static LAST_SAVE_Y: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

/// Last drawn cursor position (for change detection)
static LAST_CURSOR_X: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
static LAST_CURSOR_Y: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

/// Get last cursor position
pub fn last_cursor_pos() -> (i32, i32) {
    (
        LAST_CURSOR_X.load(core::sync::atomic::Ordering::Relaxed),
        LAST_CURSOR_Y.load(core::sync::atomic::Ordering::Relaxed),
    )
}

/// Set last cursor position
pub fn set_last_cursor_pos(x: i32, y: i32) {
    LAST_CURSOR_X.store(x, core::sync::atomic::Ordering::Relaxed);
    LAST_CURSOR_Y.store(y, core::sync::atomic::Ordering::Relaxed);
}

/// Save the framebuffer pixels that will be covered by the cursor.
/// The save area starts at (x - CURSOR_SAVE_PAD, y - CURSOR_SAVE_PAD) to
/// cover all cursor types including resize cursors that draw centered on (x,y).
pub fn save_cursor_background(fb: &FrameBuffer, x: i32, y: i32) {
    let sx = x - CURSOR_SAVE_PAD;
    let sy = y - CURSOR_SAVE_PAD;
    // Remember the save origin for restore
    LAST_SAVE_X.store(sx, core::sync::atomic::Ordering::Relaxed);
    LAST_SAVE_Y.store(sy, core::sync::atomic::Ordering::Relaxed);

    let mut bg = CURSOR_BG.lock();
    let bpp = fb.bytes_per_pixel;
    for row in 0..CURSOR_SAVE_H {
        let py = sy + row as i32;
        if py < 0 || py as usize >= fb.height {
            let dst_start = row * CURSOR_SAVE_W * 4;
            for col in 0..CURSOR_SAVE_W {
                let dst = dst_start + col * 4;
                bg[dst] = 0;
                bg[dst + 1] = 0;
                bg[dst + 2] = 0;
                bg[dst + 3] = 255;
            }
            continue;
        }
        for col in 0..CURSOR_SAVE_W {
            let px = sx + col as i32;
            let dst = (row * CURSOR_SAVE_W + col) * 4;
            if px >= 0 && (px as usize) < fb.width {
                let src = (py as usize) * fb.pitch + (px as usize) * bpp;
                if src + 3 < fb.buffer.len() {
                    bg[dst] = fb.buffer[src];
                    bg[dst + 1] = fb.buffer[src + 1];
                    bg[dst + 2] = fb.buffer[src + 2];
                    bg[dst + 3] = if bpp >= 4 { fb.buffer[src + 3] } else { 255 };
                    continue;
                }
            }
            bg[dst] = 0;
            bg[dst + 1] = 0;
            bg[dst + 2] = 0;
            bg[dst + 3] = 255;
        }
    }
    CURSOR_BG_VALID.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// Restore the framebuffer pixels that were saved before the cursor was drawn.
/// Uses the save origin (LAST_SAVE_X/Y), not the cursor position.
pub fn restore_cursor_background(fb: &mut FrameBuffer) {
    if !CURSOR_BG_VALID.load(core::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let ox = LAST_SAVE_X.load(core::sync::atomic::Ordering::Relaxed);
    let oy = LAST_SAVE_Y.load(core::sync::atomic::Ordering::Relaxed);
    let bg = CURSOR_BG.lock();
    let bpp = fb.bytes_per_pixel;
    for row in 0..CURSOR_SAVE_H {
        let py = oy + row as i32;
        if py < 0 || py as usize >= fb.height {
            continue;
        }
        for col in 0..CURSOR_SAVE_W {
            let px = ox + col as i32;
            if px >= 0 && (px as usize) < fb.width {
                let src = (row * CURSOR_SAVE_W + col) * 4;
                let dst = (py as usize) * fb.pitch + (px as usize) * bpp;
                if dst + 3 < fb.buffer.len() {
                    fb.buffer[dst] = bg[src];
                    fb.buffer[dst + 1] = bg[src + 1];
                    fb.buffer[dst + 2] = bg[src + 2];
                    if bpp >= 4 {
                        fb.buffer[dst + 3] = bg[src + 3];
                    }
                }
            }
        }
    }
}

/// Draw the mouse cursor — full-featured matching egui::CursorIcon
///
/// Palette for bitmap cursors: 0=transparent, 1=black border, 2=white fill, 3=anti-alias edge
///
/// NOTE: Cursor type is determined by the input handler (process_mouse_byte)
/// which calls update_cursor_for_position() on mouse move. We do NOT call it
/// here to avoid locking WINDOW_MANAGER/DESKTOP during the fast cursor-only path.
pub fn draw_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    let cursor_type = get_cursor_type();
    match cursor_type {
        CursorType::Default => draw_arrow_cursor(fb, x, y),
        CursorType::None => { /* invisible — draw nothing */ }
        CursorType::ContextMenu => draw_context_menu_cursor(fb, x, y),
        CursorType::Help => draw_help_cursor(fb, x, y),
        CursorType::PointingHand => draw_hand_cursor(fb, x, y),
        CursorType::Progress => draw_progress_cursor(fb, x, y),
        CursorType::Wait => draw_wait_cursor(fb, x, y),
        CursorType::Cell => draw_cell_cursor(fb, x, y),
        CursorType::Crosshair => draw_crosshair_cursor(fb, x, y),
        CursorType::Text => draw_text_cursor(fb, x, y),
        CursorType::VerticalText => draw_vertical_text_cursor(fb, x, y),
        CursorType::Alias => draw_alias_cursor(fb, x, y),
        CursorType::Copy => draw_copy_cursor(fb, x, y),
        CursorType::Move => draw_move_cursor(fb, x, y),
        CursorType::NoDrop => draw_no_drop_cursor(fb, x, y),
        CursorType::NotAllowed => draw_not_allowed_cursor(fb, x, y),
        CursorType::Grab => draw_grab_cursor(fb, x, y),
        CursorType::Grabbing => draw_grabbing_cursor(fb, x, y),
        CursorType::AllScroll => draw_all_scroll_cursor(fb, x, y),
        CursorType::ResizeHorizontal => draw_resize_h_cursor(fb, x, y),
        CursorType::ResizeNeSw => draw_resize_diag_trbl_cursor(fb, x, y),
        CursorType::ResizeNwSe => draw_resize_diag_tlbr_cursor(fb, x, y),
        CursorType::ResizeVertical => draw_resize_v_cursor(fb, x, y),
        CursorType::ResizeEast => draw_resize_east_cursor(fb, x, y),
        CursorType::ResizeSouthEast => draw_resize_diag_tlbr_cursor(fb, x, y),
        CursorType::ResizeSouth => draw_resize_south_cursor(fb, x, y),
        CursorType::ResizeSouthWest => draw_resize_diag_trbl_cursor(fb, x, y),
        CursorType::ResizeWest => draw_resize_west_cursor(fb, x, y),
        CursorType::ResizeNorthWest => draw_resize_diag_tlbr_cursor(fb, x, y),
        CursorType::ResizeNorth => draw_resize_north_cursor(fb, x, y),
        CursorType::ResizeNorthEast => draw_resize_diag_trbl_cursor(fb, x, y),
        CursorType::ResizeColumn => draw_resize_column_cursor(fb, x, y),
        CursorType::ResizeRow => draw_resize_row_cursor(fb, x, y),
        CursorType::ZoomIn => draw_zoom_in_cursor(fb, x, y),
        CursorType::ZoomOut => draw_zoom_out_cursor(fb, x, y),
    }
}

/// Helper: draw a bitmap cursor with shadow from a 2D array.
/// Palette: 0=transparent, 1=border, 2=fill, 3=AA edge (50% border blend),
///          4=light gray fill, 5=mid gray fill
/// Colors are determined by the active cursor theme.
fn draw_bitmap_cursor<const W: usize, const H: usize>(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    data: &[[u8; W]; H],
) {
    use super::framebuffer::Pixel;
    let (fill_color, border_color) = cursor_colors();

    // Draw drop shadow first (offset +1,+1, semi-transparent black)
    for (row, line) in data.iter().enumerate() {
        for (col, &pixel) in line.iter().enumerate() {
            if pixel == 1 || pixel == 2 || pixel == 4 || pixel == 5 {
                let px = x + col as i32 + 1;
                let py = y + row as i32 + 1;
                if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                    let ux = px as usize;
                    let uy = py as usize;
                    let off = uy * fb.pitch + ux * fb.bytes_per_pixel;
                    if off + 2 < fb.buffer.len() {
                        let ob = fb.buffer[off] as u16;
                        let og = fb.buffer[off + 1] as u16;
                        let or_ = fb.buffer[off + 2] as u16;
                        fb.buffer[off] = (ob * 70 / 100) as u8;
                        fb.buffer[off + 1] = (og * 70 / 100) as u8;
                        fb.buffer[off + 2] = (or_ * 70 / 100) as u8;
                    }
                }
            }
        }
    }

    // Derive sub-colors from fill
    let light_gray = Pixel::lerp(fill_color, border_color, 60);
    let mid_gray = Pixel::lerp(fill_color, border_color, 100);

    // Draw cursor shape (on top of shadow)
    for (row, line) in data.iter().enumerate() {
        for (col, &pixel) in line.iter().enumerate() {
            if pixel == 0 {
                continue;
            }
            let px = x + col as i32;
            let py = y + row as i32;
            if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                let ux = px as usize;
                let uy = py as usize;
                match pixel {
                    1 => fb.set_pixel(ux, uy, border_color),
                    2 => fb.set_pixel(ux, uy, fill_color),
                    3 => {
                        // AA edge: 50% blend toward border color
                        let off = uy * fb.pitch + ux * fb.bytes_per_pixel;
                        if off + 2 < fb.buffer.len() {
                            let ob = fb.buffer[off] as u16;
                            let og = fb.buffer[off + 1] as u16;
                            let or_ = fb.buffer[off + 2] as u16;
                            fb.buffer[off] =
                                ((ob * 128 + border_color.b as u16 * 128 + 128) >> 8) as u8;
                            fb.buffer[off + 1] =
                                ((og * 128 + border_color.g as u16 * 128 + 128) >> 8) as u8;
                            fb.buffer[off + 2] =
                                ((or_ * 128 + border_color.r as u16 * 128 + 128) >> 8) as u8;
                        }
                    }
                    4 => fb.set_pixel(ux, uy, light_gray),
                    5 => fb.set_pixel(ux, uy, mid_gray),
                    _ => {}
                }
            }
        }
    }
}

/// Helper: draw a bitmap cursor centered on (x,y).
fn draw_bitmap_cursor_centered<const W: usize, const H: usize>(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    data: &[[u8; W]; H],
) {
    let ox = x - (W as i32) / 2;
    let oy = y - (H as i32) / 2;
    draw_bitmap_cursor(fb, ox, oy, data);
}

// ═══════════════════════════════════════════════════════════════════════
// 1. Default — Standard arrow cursor  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_arrow_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=transparent 1=black 2=white 3=AA
    #[rustfmt::skip]
    const D: [[u8; 15]; 21] = [
        [1,3,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,1,3,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,2,1,3,0,0,0,0,0,0,0,0,0,0,0],
        [1,2,2,1,3,0,0,0,0,0,0,0,0,0,0],
        [1,2,2,2,1,3,0,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,1,3,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,1,3,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,2,1,3,0,0,0,0,0,0],
        [1,2,2,2,2,2,2,2,1,3,0,0,0,0,0],
        [1,2,2,2,2,2,2,2,2,1,3,0,0,0,0],
        [1,2,2,2,2,2,2,2,2,2,1,3,0,0,0],
        [1,2,2,2,2,2,2,2,2,2,2,1,3,0,0],
        [1,2,2,2,2,2,2,2,2,2,2,2,1,3,0],
        [1,2,2,2,2,2,2,1,1,1,1,1,1,1,0],
        [1,2,2,2,2,1,2,2,1,3,0,0,0,0,0],
        [1,2,2,2,1,0,1,2,2,1,3,0,0,0,0],
        [1,2,2,1,3,0,1,2,2,1,3,0,0,0,0],
        [1,2,1,3,0,0,0,1,2,2,1,3,0,0,0],
        [1,1,3,0,0,0,0,1,2,2,1,3,0,0,0],
        [1,3,0,0,0,0,0,0,1,1,1,3,0,0,0],
        [3,0,0,0,0,0,0,0,0,3,0,0,0,0,0],
    ];
    draw_bitmap_cursor(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 2. ContextMenu — Arrow + tiny menu  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_context_menu_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use super::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    let g = Pixel::rgb(100, 100, 100);
    // 9×10 menu box at offset (10, 12)
    let mx = x + 10;
    let my = y + 12;
    for dx in 0..=8 {
        fb.blend_pixel((mx + dx) as usize, my as usize, b);
        fb.blend_pixel((mx + dx) as usize, (my + 9) as usize, b);
    }
    for dy in 0..=9 {
        fb.blend_pixel(mx as usize, (my + dy) as usize, b);
        fb.blend_pixel((mx + 8) as usize, (my + dy) as usize, b);
    }
    for dy in 1..9 {
        for dx in 1..8 {
            fb.blend_pixel((mx + dx) as usize, (my + dy) as usize, w);
        }
    }
    for dx in 2..7 {
        fb.blend_pixel((mx + dx) as usize, (my + 2) as usize, g);
        fb.blend_pixel((mx + dx) as usize, (my + 4) as usize, g);
        fb.blend_pixel((mx + dx) as usize, (my + 6) as usize, g);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3. Help — Arrow + question mark badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_help_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use super::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    let blue = Pixel::rgb(40, 100, 210);
    // Blue filled circle r=5 at offset (11,13)
    let qx = x + 11;
    let qy = y + 13;
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 <= 25 {
                fb.blend_pixel((qx + dx) as usize, (qy + dy) as usize, blue);
            }
        }
    }
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 > 20 && d2 <= 30 {
                fb.blend_pixel((qx + dx) as usize, (qy + dy) as usize, b);
            }
        }
    }
    // "?" glyph (5 px wide)
    fb.blend_pixel((qx - 1) as usize, (qy - 3) as usize, w);
    fb.blend_pixel(qx as usize, (qy - 4) as usize, w);
    fb.blend_pixel((qx + 1) as usize, (qy - 3) as usize, w);
    fb.blend_pixel((qx + 1) as usize, (qy - 2) as usize, w);
    fb.blend_pixel(qx as usize, (qy - 1) as usize, w);
    fb.blend_pixel(qx as usize, qy as usize, w);
    fb.blend_pixel(qx as usize, (qy + 2) as usize, w);
    fb.blend_pixel(qx as usize, (qy + 3) as usize, w);
}

// ═══════════════════════════════════════════════════════════════════════
// 4. PointingHand — Modern pointing-finger hand  (hotspot: finger tip)
//    Larger 17×22 bitmap — close to macOS / GTK pointer hand.
// ═══════════════════════════════════════════════════════════════════════
fn draw_hand_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=transparent 1=black 2=white 3=AA 4=light-gray 5=mid-gray
    #[rustfmt::skip]
    const D: [[u8; 17]; 22] = [
        [0,0,0,0,0,0,1,1,3,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,3,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,1,1,0,1,1,3,0,0],
        [0,0,0,0,0,1,2,2,1,2,2,1,2,2,1,3,0],
        [0,0,0,0,0,1,2,2,1,2,2,1,2,2,1,0,0],
        [0,0,1,1,0,1,2,2,2,2,2,1,2,2,1,1,0],
        [0,1,2,2,1,1,2,2,2,2,2,2,2,2,1,2,1],
        [0,1,2,2,1,2,2,2,2,2,2,2,2,2,1,2,1],
        [3,1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,0,1,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,0,1,2,2,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,1,1,1,1,1,3,0,0,0,0],
    ];
    // hotspot is the finger-tip: column 7, row 0 → draw at (x-7, y)
    draw_bitmap_cursor(fb, x - 7, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Progress — Arrow + spinning circle  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_progress_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use super::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let blue = Pixel::rgb(50, 130, 240);
    let light = Pixel::rgb(180, 210, 255);
    // Spinning disc r=4 at (12, 16)
    let cx = x + 12;
    let cy = y + 16;
    for dy in -4..=4i32 {
        for dx in -4..=4i32 {
            let d2 = dx * dx + dy * dy;
            if d2 <= 16 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, light);
            }
        }
    }
    // Quarter-arc in blue (upper-right)
    for dy in -4..=0i32 {
        for dx in 0..=4i32 {
            let d2 = dx * dx + dy * dy;
            if (8..=18).contains(&d2) {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, blue);
            }
        }
    }
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 > 16 && d2 <= 28 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, b);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 6. Wait — Hourglass / busy  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_wait_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=trans 1=black 2=white 3=AA 4=light-gray 5=mid-gray
    #[rustfmt::skip]
    const D: [[u8; 13]; 17] = [
        [1,1,1,1,1,1,1,1,1,1,1,1,1],
        [0,1,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,1,4,4,4,4,4,4,4,1,0,0],
        [0,0,0,1,4,4,4,4,4,1,0,0,0],
        [0,0,0,1,5,5,5,5,5,1,0,0,0],
        [0,0,0,0,1,5,5,5,1,0,0,0,0],
        [0,0,0,0,0,1,5,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,1,0,0,0,0,0],
        [0,0,0,0,1,2,2,2,1,0,0,0,0],
        [0,0,0,0,1,2,2,2,1,0,0,0,0],
        [0,0,0,1,2,2,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,2,2,1,0,0],
        [0,0,1,2,2,2,2,2,2,2,1,0,0],
        [0,1,2,2,2,2,2,2,2,2,2,1,0],
        [1,1,1,1,1,1,1,1,1,1,1,1,1],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 7. Cell — Thick plus for table-cell selection  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_cell_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 15]; 15] = [
        [0,0,0,0,0,0,1,1,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [1,1,1,1,1,1,1,2,1,1,1,1,1,1,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,1,1,1,1,1,1,2,1,1,1,1,1,1,1],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 8. Crosshair — Thin precision cross with gap  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_crosshair_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,1,1,1,1,1,0,0,1,0,0,1,1,1,1,1,1],
        [1,2,2,2,2,2,0,1,0,1,0,2,2,2,2,2,1],
        [1,1,1,1,1,1,0,0,1,0,0,1,1,1,1,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 9. Text — I-beam cursor  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_text_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 9]; 19] = [
        [0,1,1,3,0,3,1,1,0],
        [1,3,0,1,1,1,0,3,1],
        [0,0,0,0,1,0,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,0,1,0,0,0,0],
        [1,3,0,1,1,1,0,3,1],
        [0,1,1,3,0,3,1,1,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 10. VerticalText — Horizontal I-beam  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_vertical_text_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 19]; 9] = [
        [0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0],
        [1,3,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,3,1],
        [1,0,0,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,1],
        [3,1,1,2,2,2,2,2,2,2,2,2,2,2,2,2,1,1,3],
        [0,1,0,1,1,1,1,1,1,1,1,1,1,1,1,1,0,1,0],
        [3,1,1,2,2,2,2,2,2,2,2,2,2,2,2,2,1,1,3],
        [1,0,0,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,1],
        [1,3,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,3,1],
        [0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 11. Alias — Arrow + curved-arrow badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_alias_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use super::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    // Small shortcut arrow 8×8 at (9, 14)
    let ax = x + 9;
    let ay = y + 14;
    // Curved shaft
    for dx in 0..=4i32 {
        fb.blend_pixel((ax + dx) as usize, (ay - 3) as usize, b);
    }
    fb.blend_pixel((ax + 5) as usize, (ay - 2) as usize, b);
    fb.blend_pixel((ax + 5) as usize, (ay - 1) as usize, b);
    fb.blend_pixel((ax + 5) as usize, ay as usize, b);
    fb.blend_pixel((ax + 4) as usize, (ay + 1) as usize, b);
    for dx in 0..=3i32 {
        fb.blend_pixel((ax + dx) as usize, (ay + 2) as usize, b);
    }
    // Fill
    for dx in 1..=3i32 {
        fb.blend_pixel((ax + dx) as usize, (ay - 2) as usize, w);
    }
    fb.blend_pixel((ax + 4) as usize, (ay - 1) as usize, w);
    fb.blend_pixel((ax + 4) as usize, ay as usize, w);
    for dx in 1..=3i32 {
        fb.blend_pixel((ax + dx) as usize, (ay + 1) as usize, w);
    }
    // Arrowhead at bottom-left
    fb.blend_pixel((ax - 1) as usize, (ay + 1) as usize, b);
    fb.blend_pixel(ax as usize, (ay + 3) as usize, b);
    fb.blend_pixel((ax - 1) as usize, (ay + 3) as usize, b);
    fb.blend_pixel((ax - 2) as usize, (ay + 2) as usize, b);
}

// ═══════════════════════════════════════════════════════════════════════
// 12. Copy — Arrow + green "+" badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_copy_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use super::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    let g = Pixel::rgb(30, 180, 30);
    // Filled green circle r=5 at (12,16)
    let cx = x + 12;
    let cy = y + 16;
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            if dx * dx + dy * dy <= 25 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, g);
            }
        }
    }
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 > 20 && d2 <= 30 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, b);
            }
        }
    }
    // "+" sign
    for d in -3..=3i32 {
        fb.blend_pixel((cx + d) as usize, cy as usize, w);
        fb.blend_pixel(cx as usize, (cy + d) as usize, w);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 13. Move — Four-directional arrow cross  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_move_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,1,1,0,0,1,2,2,2,1,0,0,1,1,0,0],
        [0,1,2,2,1,1,1,2,2,2,1,1,1,2,2,1,0],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,1,2,2,1,1,1,2,2,2,1,1,1,2,2,1,0],
        [0,0,1,1,0,0,1,2,2,2,1,0,0,1,1,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 14. NoDrop — Arrow + red ⊘ badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_no_drop_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use super::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let r = Pixel::rgb(210, 30, 30);
    let cx = x + 12;
    let cy = y + 16;
    // Red ring
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if (13..=28).contains(&d2) {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, r);
            }
        }
    }
    // Black outline
    for dy in -6..=6i32 {
        for dx in -6..=6i32 {
            let d2 = dx * dx + dy * dy;
            if (28..=40).contains(&d2) {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, b);
            }
        }
    }
    // Diagonal slash
    for i in -4..=4i32 {
        fb.blend_pixel((cx + i) as usize, (cy - i) as usize, r);
        fb.blend_pixel((cx + i + 1) as usize, (cy - i) as usize, r);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 15. NotAllowed — Red circle ⊘  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_not_allowed_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0],
        [0,0,0,3,1,1,1,1,1,1,1,1,1,3,0,0,0],
        [0,0,3,1,1,2,2,2,2,2,1,1,1,3,0,0,0],
        [0,3,1,1,2,2,2,2,2,1,1,2,1,1,3,0,0],
        [0,1,1,2,2,2,2,2,1,1,2,2,2,1,0,0,0],
        [3,1,2,2,2,2,2,1,1,2,2,2,2,2,1,3,0],
        [1,1,2,2,2,2,1,1,2,2,2,2,2,2,1,1,0],
        [1,1,2,2,2,1,1,2,2,2,2,2,2,2,1,1,0],
        [1,1,2,2,1,1,2,2,2,2,2,1,1,2,1,1,0],
        [1,1,2,2,2,2,2,2,2,2,1,1,2,2,1,1,0],
        [1,1,2,2,2,2,2,2,2,1,1,2,2,2,1,1,0],
        [3,1,2,2,2,2,2,2,1,1,2,2,2,2,1,3,0],
        [0,1,1,2,2,2,2,1,1,2,2,2,2,1,1,0,0],
        [0,3,1,1,2,2,1,1,2,2,2,2,1,1,3,0,0],
        [0,0,3,1,1,1,1,2,2,2,2,1,1,3,0,0,0],
        [0,0,0,3,1,1,1,1,1,1,1,1,1,3,0,0,0],
        [0,0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0],
    ];
    // Override: 1=red-dark border, 2=pink fill — custom palette
    use super::framebuffer::Pixel;
    let cx = x;
    let cy = y;
    let ox = cx - 8;
    let oy = cy - 8;
    let red = Pixel::rgb(200, 30, 30);
    let pink = Pixel::rgb(240, 110, 110);
    let blk = Pixel::rgb(0, 0, 0);
    // shadow
    for (row, line) in D.iter().enumerate() {
        for (col, &p) in line.iter().enumerate() {
            if p == 1 || p == 2 {
                let px = ox + col as i32 + 1;
                let py = oy + row as i32 + 1;
                if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                    let ux = px as usize;
                    let uy = py as usize;
                    let off = uy * fb.pitch + ux * fb.bytes_per_pixel;
                    if off + 2 < fb.buffer.len() {
                        fb.buffer[off] = (fb.buffer[off] as u16 * 70 / 100) as u8;
                        fb.buffer[off + 1] = (fb.buffer[off + 1] as u16 * 70 / 100) as u8;
                        fb.buffer[off + 2] = (fb.buffer[off + 2] as u16 * 70 / 100) as u8;
                    }
                }
            }
        }
    }
    for (row, line) in D.iter().enumerate() {
        for (col, &p) in line.iter().enumerate() {
            if p == 0 {
                continue;
            }
            let px = ox + col as i32;
            let py = oy + row as i32;
            if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                let ux = px as usize;
                let uy = py as usize;
                match p {
                    1 => fb.set_pixel(ux, uy, red),
                    2 => fb.set_pixel(ux, uy, pink),
                    3 => fb.blend_pixel(ux, uy, Pixel::new(200, 30, 30, 128)),
                    _ => {}
                }
            }
        }
    }
    // Solid dark diagonal band
    for i in -5..=5i32 {
        for t in -1..=1i32 {
            let px = (cx + i) as usize;
            let py = (cy - i + t) as usize;
            if px < fb.width && py < fb.height {
                fb.set_pixel(px, py, red);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 16. Grab — Open hand  (hotspot: center-ish)
// ═══════════════════════════════════════════════════════════════════════
fn draw_grab_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 18]; 19] = [
        [0,0,0,0,1,1,0,0,1,1,0,0,1,1,0,0,0,0],
        [0,0,0,1,2,2,1,1,2,2,1,1,2,2,1,0,0,0],
        [0,0,0,1,2,2,1,2,2,2,1,2,2,2,1,3,0,0],
        [0,0,0,1,2,2,1,2,2,2,1,2,2,2,1,2,1,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,1,2,2,1,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,1,2,2,1,0],
        [1,1,3,0,0,1,2,2,2,2,2,2,2,2,2,2,1,0],
        [1,2,2,1,0,1,2,2,2,2,2,2,2,2,2,2,1,0],
        [1,2,2,1,0,1,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,1,2,2,1,2,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,1,2,2,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 17. Grabbing — Closed fist  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_grabbing_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 16] = [
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,1,1,0,1,1,0,1,1,0,0,0,0,0,0],
        [0,0,1,2,2,1,2,2,1,2,2,1,1,0,0,0,0],
        [0,0,1,2,2,1,2,2,1,2,2,1,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,2,2,2,1,2,2,1,0,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 18. AllScroll — Four-way arrows with center dot  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_all_scroll_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,1,1,0,0,0,0,0,0,0,0,0,1,1,0,0],
        [0,1,2,2,1,0,0,1,1,1,0,0,1,2,2,1,0],
        [1,2,2,2,1,0,0,1,2,1,0,0,1,2,2,2,1],
        [0,1,2,2,1,0,0,1,1,1,0,0,1,2,2,1,0],
        [0,0,1,1,0,0,0,0,0,0,0,0,0,1,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 19. ResizeHorizontal — Double horizontal arrow ↔  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_h_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 19]; 11] = [
        [0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,1,2,1,0,0,0,0,0,0,0,0,0,1,2,1,0,0],
        [0,1,2,2,1,0,0,0,0,0,0,0,0,0,1,2,2,1,0],
        [1,2,2,2,1,1,1,1,1,1,1,1,1,1,1,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,1,1,1,1,1,1,1,1,1,1,1,2,2,2,1],
        [0,1,2,2,1,0,0,0,0,0,0,0,0,0,1,2,2,1,0],
        [0,0,1,2,1,0,0,0,0,0,0,0,0,0,1,2,1,0,0],
        [0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 20. ResizeNeSw — Diagonal ↗↙  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_diag_trbl_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 15]; 15] = [
        [0,0,0,0,0,0,0,0,1,1,1,1,1,1,1],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,2,1],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,1,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,1,0,0],
        [0,0,0,0,0,0,0,1,1,2,2,1,0,0,0],
        [0,0,0,0,0,0,1,2,1,1,1,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,1,2,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,1,1,1,0,0,0,0,0,0,0,0,0],
        [0,0,0,1,2,2,1,1,0,0,0,0,0,0,0],
        [0,0,1,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,1,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [1,1,1,1,1,1,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 21. ResizeNwSe — Diagonal ↘↖  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_diag_tlbr_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 15]; 15] = [
        [1,1,1,1,1,1,1,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,1,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,1,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,1,2,2,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,1,1,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,1,1,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,2,2,1,0,0,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,1,0,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,1,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,2,1],
        [0,0,0,0,0,0,0,0,1,1,1,1,1,1,1],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 22. ResizeVertical — Double vertical arrow ↕  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_v_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 11]; 19] = [
        [0,0,0,0,0,1,0,0,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,0,0,1,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 23. ResizeEast — Right arrow →  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_east_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 13]; 11] = [
        [0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,0,1,1,0,0],
        [1,1,0,0,0,0,0,0,0,1,2,1,0],
        [1,2,1,1,1,1,1,1,1,1,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,1,1,1,1,1,1,1,1,2,2,1],
        [1,1,0,0,0,0,0,0,0,1,2,1,0],
        [0,0,0,0,0,0,0,0,0,1,1,0,0],
        [0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 25. ResizeSouth — Down arrow ↓  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_south_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 11]; 13] = [
        [0,0,1,1,1,1,1,1,1,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,0,0,1,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 27. ResizeWest — Left arrow ←  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_west_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 13]; 11] = [
        [0,0,0,1,0,0,0,0,0,0,0,0,0],
        [0,0,1,1,0,0,0,0,0,0,0,0,0],
        [0,1,2,1,0,0,0,0,0,0,0,1,1],
        [1,2,2,1,1,1,1,1,1,1,1,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,1,1,1,1,1,1,1,1,2,1],
        [0,1,2,1,0,0,0,0,0,0,0,1,1],
        [0,0,1,1,0,0,0,0,0,0,0,0,0],
        [0,0,0,1,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 29. ResizeNorth — Up arrow ↑  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_north_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 11]; 13] = [
        [0,0,0,0,0,1,0,0,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,1,1,1,1,1,1,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 31. ResizeColumn — col-resize: ↔ with vertical bars  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_column_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,1,0,0,0,0,1,0,0,0,0,1,0,0,0],
        [0,0,1,2,1,0,0,0,1,0,0,0,1,2,1,0,0],
        [0,1,2,2,1,0,0,0,1,0,0,0,1,2,2,1,0],
        [1,2,2,2,1,1,1,0,1,0,1,1,1,2,2,2,1],
        [1,2,2,2,2,2,2,0,1,0,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,0,1,0,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,0,1,0,2,2,2,2,2,2,1],
        [1,2,2,2,1,1,1,0,1,0,1,1,1,2,2,2,1],
        [0,1,2,2,1,0,0,0,1,0,0,0,1,2,2,1,0],
        [0,0,1,2,1,0,0,0,1,0,0,0,1,2,1,0,0],
        [0,0,0,1,0,0,0,0,1,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 32. ResizeRow — row-resize: ↕ with horizontal bars  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_row_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,1,1,1,1,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,1,1,1,1,2,2,2,1,1,1,1,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
        [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,1,1,1,1,2,2,2,1,1,1,1,0,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,1,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 33. ZoomIn — Magnifying glass with "+"  (hotspot: lens center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_zoom_in_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=trans 1=black 2=white 3=AA 4=light-gray(glass-fill) 5=mid-gray
    #[rustfmt::skip]
    const D: [[u8; 17]; 20] = [
        [0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,0,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,3,0,0,0],
        [0,1,4,4,4,4,1,1,1,4,4,4,4,1,0,0,0],
        [3,1,4,4,4,4,1,2,1,4,4,4,4,1,3,0,0],
        [1,4,4,4,4,4,1,2,1,4,4,4,4,4,1,0,0],
        [1,4,4,1,1,1,1,2,1,1,1,1,4,4,1,0,0],
        [1,4,4,1,2,2,2,2,2,2,2,1,4,4,1,0,0],
        [1,4,4,1,1,1,1,2,1,1,1,1,4,4,1,0,0],
        [1,4,4,4,4,4,1,2,1,4,4,4,4,4,1,0,0],
        [3,1,4,4,4,4,1,1,1,4,4,4,4,1,3,0,0],
        [0,1,4,4,4,4,4,4,4,4,4,4,4,1,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,1,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,1,1,0,0],
        [0,0,0,0,3,1,1,1,1,1,3,0,1,2,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor(fb, x - 7, y - 7, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 34. ZoomOut — Magnifying glass with "−"  (hotspot: lens center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_zoom_out_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 20] = [
        [0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,0,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,3,0,0,0],
        [0,1,4,4,4,4,4,4,4,4,4,4,4,1,0,0,0],
        [3,1,4,4,4,4,4,4,4,4,4,4,4,1,3,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [1,4,4,1,2,2,2,2,2,2,2,1,4,4,1,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [3,1,4,4,4,4,4,4,4,4,4,4,4,1,3,0,0],
        [0,1,4,4,4,4,4,4,4,4,4,4,4,1,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,1,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,1,1,0,0],
        [0,0,0,0,3,1,1,1,1,1,3,0,1,2,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor(fb, x - 7, y - 7, &D);
}

/// Timer tick handler - update clock and request redraw
pub fn on_timer_tick(ticks: u64) {
    // Capture old clock text for change detection
    let old_clock = taskbar::get_clock_text();
    taskbar::update_clock(ticks);
    let new_clock = taskbar::get_clock_text();

    // Tick notification animations — check if any are actively animating
    let mut notifs = super::notifications::NOTIFICATIONS.lock();
    let has_active = notifs
        .notifications
        .iter()
        .any(|n| !n.dismissed && (n.anim_progress < 255 || n.sliding_out));
    notifs.tick();
    drop(notifs);

    // Only request a full redraw if something actually changed
    if old_clock != new_clock || has_active {
        super::request_redraw();
    }
}

/// Handle desktop click
pub fn handle_click(x: i32, y: i32) {
    // Close start menu if open and clicking outside
    if startmenu::is_visible() {
        if let Some(item) = startmenu::handle_click(x, y) {
            crate::serial_println!("[KnoxOS] Start menu: Opening {}", item.name);
            open_application(&item.name, item.icon_type);
            super::request_redraw();
            return;
        }
        startmenu::close();
        super::request_redraw();
        return;
    }

    let mut desktop = DESKTOP.lock();

    // Check if clicking on an icon
    let mut clicked_icon: Option<(usize, bool)> = None; // (index, was_already_selected)
    for (i, icon) in desktop.icons.iter().enumerate() {
        let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
        if icon_rect.contains(x, y) {
            clicked_icon = Some((i, icon.selected));
            break;
        }
    }

    match clicked_icon {
        Some((idx, _was_selected)) => {
            // Select this icon
            for (i, icon) in desktop.icons.iter_mut().enumerate() {
                icon.selected = i == idx;
            }
            desktop.selected_icon = Some(idx);

            // Start drag tracking — the actual drag movement starts when
            // handle_icon_drag() detects the mouse has moved enough
            desktop.dragging_icon = Some(idx);
            desktop.drag_offset_x = x - desktop.icons[idx].x;
            desktop.drag_offset_y = y - desktop.icons[idx].y;
            desktop.drag_x = desktop.icons[idx].x;
            desktop.drag_y = desktop.icons[idx].y;
        }
        None => {
            // Clicked on empty desktop — deselect all and start rubber band
            for icon in desktop.icons.iter_mut() {
                icon.selected = false;
            }
            desktop.selected_icon = None;
            desktop.rubber_band = Some(RubberBand {
                start_x: x,
                start_y: y,
                end_x: x,
                end_y: y,
            });
        }
    }

    drop(desktop);
    super::request_redraw();
}

/// Handle desktop double-click - open the clicked icon
pub fn handle_double_click(x: i32, y: i32) {
    // Cancel any drag in progress
    {
        let mut desktop = DESKTOP.lock();
        desktop.dragging_icon = None;
    }

    let desktop = DESKTOP.lock();

    for icon in &desktop.icons {
        let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
        if icon_rect.contains(x, y) {
            crate::serial_println!("[KnoxOS] Opening: {}", icon.name);
            let name = icon.name.clone();
            let icon_type = icon.icon_type;
            drop(desktop);
            open_application(&name, icon_type);
            super::request_redraw();
            return;
        }
    }
}

/// Check if a desktop icon is currently being dragged
pub fn is_dragging_icon() -> bool {
    DESKTOP.lock().dragging_icon.is_some()
}

/// Handle icon drag movement. Called from input.rs handle_drag() when
/// a desktop icon drag is active. Updates the drag position.
pub fn handle_icon_drag(mx: i32, my: i32) {
    let mut desktop = DESKTOP.lock();
    if let Some(_idx) = desktop.dragging_icon {
        let old_x = desktop.drag_x;
        let old_y = desktop.drag_y;
        desktop.drag_x = mx - desktop.drag_offset_x;
        desktop.drag_y = my - desktop.drag_offset_y;

        // Push damage for old and new positions
        let old_rect = Rect::new(old_x - 2, old_y - 2, ICON_WIDTH + 4, ICON_HEIGHT + 4);
        let new_rect = Rect::new(
            desktop.drag_x - 2,
            desktop.drag_y - 2,
            ICON_WIDTH + 4,
            ICON_HEIGHT + 4,
        );
        drop(desktop);
        super::push_damage(old_rect);
        super::push_damage(new_rect);
    }
}

/// Handle icon drop — snap to nearest grid cell, or perform file operation
/// if a Document/file icon is dropped onto a Folder icon.
/// Called from input.rs handle_mouse_up().
pub fn handle_icon_drop(mx: i32, my: i32) {
    let mut desktop = DESKTOP.lock();
    if let Some(idx) = desktop.dragging_icon {
        // Final drop position
        let drop_x = mx - desktop.drag_offset_x;
        let drop_y = my - desktop.drag_offset_y;

        // Snap to nearest grid cell
        let col = ((drop_x - ICON_GRID_X + ICON_GRID_SPACING_X / 2) / ICON_GRID_SPACING_X).max(0);
        let row = ((drop_y - ICON_GRID_Y + ICON_GRID_SPACING_Y / 2) / ICON_GRID_SPACING_Y).max(0);

        let snapped_x = ICON_GRID_X + col * ICON_GRID_SPACING_X;
        let snapped_y = ICON_GRID_Y + row * ICON_GRID_SPACING_Y;

        // ── File drop onto folder icon ───────────────────────
        // Check if we dropped a file/document icon onto a folder icon
        let dragged_type = desktop.icons[idx].icon_type;
        let dragged_name = desktop.icons[idx].name.clone();
        let target_icon = desktop
            .icons
            .iter()
            .enumerate()
            .find(|(i, ic)| *i != idx && ic.x == snapped_x && ic.y == snapped_y);

        if let Some((target_idx, _)) = target_icon {
            let target_type = desktop.icons[target_idx].icon_type;
            let target_name = desktop.icons[target_idx].name.clone();

            // If dragging a Document onto a Folder, move the file into the folder
            if dragged_type == IconType::Document && target_type == IconType::Folder {
                let desktop_dir = "/home/user/Desktop";
                let src_path = alloc::format!("{}/{}", desktop_dir, dragged_name);
                // Map folder name to its path
                let dst_dir = match target_name.as_str() {
                    "Documents" => "/home/user/Documents",
                    "Downloads" => "/home/user/Downloads",
                    "Music" => "/home/user/Music",
                    "Pictures" => "/home/user/Pictures",
                    "Videos" => "/home/user/Videos",
                    _ => {
                        // Try as a subfolder on Desktop
                        let fallback = alloc::format!("{}/{}", desktop_dir, target_name);
                        // leak to get 'static str — acceptable in kernel
                        // Use the stack buffer approach instead
                        desktop.dragging_icon = None;
                        drop(desktop);
                        match crate::vfs::move_file_dispatch(&src_path, &fallback) {
                            Ok(()) => {
                                super::notifications::info(
                                    "Desktop",
                                    &alloc::format!("Moved {} → {}", dragged_name, target_name),
                                );
                                // Remove the dragged icon from the desktop
                                let mut d2 = DESKTOP.lock();
                                d2.icons.remove(idx);
                                drop(d2);
                            }
                            Err(_) => {
                                super::notifications::error(
                                    "Desktop",
                                    &alloc::format!("Failed to move {}", dragged_name),
                                );
                            }
                        }
                        super::request_redraw();
                        return;
                    }
                };

                desktop.dragging_icon = None;
                drop(desktop);

                match crate::vfs::move_file_dispatch(&src_path, dst_dir) {
                    Ok(()) => {
                        super::notifications::info(
                            "Desktop",
                            &alloc::format!("Moved {} → {}", dragged_name, target_name),
                        );
                        // Remove the dragged icon from the desktop
                        let mut d2 = DESKTOP.lock();
                        d2.icons.remove(idx);
                        drop(d2);
                    }
                    Err(_) => {
                        super::notifications::error(
                            "Desktop",
                            &alloc::format!("Failed to move {}", dragged_name),
                        );
                    }
                }
                super::request_redraw();
                return;
            }
        }

        // ── Normal icon repositioning (no file operation) ────
        // Check if another icon already occupies this grid cell
        let occupied = desktop
            .icons
            .iter()
            .enumerate()
            .any(|(i, ic)| i != idx && ic.x == snapped_x && ic.y == snapped_y);

        if occupied {
            // If target cell is occupied, swap positions with that icon
            if let Some(other_idx) = desktop
                .icons
                .iter()
                .position(|ic| ic.x == snapped_x && ic.y == snapped_y)
            {
                let orig_x = desktop.icons[idx].x;
                let orig_y = desktop.icons[idx].y;
                desktop.icons[other_idx].x = orig_x;
                desktop.icons[other_idx].y = orig_y;
            }
        }

        desktop.icons[idx].x = snapped_x;
        desktop.icons[idx].y = snapped_y;
        desktop.dragging_icon = None;

        crate::serial_println!("[KnoxOS] Icon dropped at grid ({}, {})", col, row,);
    } else {
        desktop.dragging_icon = None;
    }
    drop(desktop);
    super::request_redraw();
}

/// Cancel icon drag without moving
pub fn cancel_icon_drag() {
    let mut desktop = DESKTOP.lock();
    desktop.dragging_icon = None;
}

/// Check if rubber band selection is active
pub fn is_rubber_band_active() -> bool {
    DESKTOP.lock().rubber_band.is_some()
}

/// Update rubber band selection rectangle as mouse moves.
/// Selects all icons that intersect the rubber band.
pub fn handle_rubber_band_drag(mx: i32, my: i32) {
    let mut desktop = DESKTOP.lock();
    if let Some(ref mut rb) = desktop.rubber_band {
        let old_rect = rb.to_rect();

        rb.end_x = mx;
        rb.end_y = my;

        let new_rect = rb.to_rect();

        // Select icons that intersect the rubber band rectangle
        let band_rect = new_rect;
        for icon in desktop.icons.iter_mut() {
            let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
            icon.selected = icon_rect.intersects(&band_rect);
        }

        // Push damage for old and new rubber band areas
        drop(desktop);
        super::push_damage(Rect::new(
            old_rect.x - 2,
            old_rect.y - 2,
            old_rect.width + 4,
            old_rect.height + 4,
        ));
        super::push_damage(Rect::new(
            new_rect.x - 2,
            new_rect.y - 2,
            new_rect.width + 4,
            new_rect.height + 4,
        ));
    }
}

/// Finish rubber band selection (mouse released).
pub fn finish_rubber_band() {
    let mut desktop = DESKTOP.lock();
    if let Some(rb) = desktop.rubber_band.take() {
        // Final selection — icons that intersect
        let band_rect = rb.to_rect();
        let mut first_selected = None;
        for (i, icon) in desktop.icons.iter_mut().enumerate() {
            let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
            icon.selected = icon_rect.intersects(&band_rect);
            if icon.selected && first_selected.is_none() {
                first_selected = Some(i);
            }
        }
        desktop.selected_icon = first_selected;

        let count = desktop.icons.iter().filter(|i| i.selected).count();
        crate::serial_println!("[KnoxOS] Rubber band selection: {} icons selected", count);
    }
    drop(desktop);
    super::request_redraw();
}

/// Open an application by creating a new window
pub fn open_application(name: &str, icon_type: IconType) {
    // Route apps with dedicated open() functions
    match name {
        "Task Manager" => {
            super::task_manager::open();
            return;
        }
        "Calculator" => {
            super::calculator::open();
            return;
        }
        "Image Viewer" => {
            super::image_viewer::open();
            return;
        }
        "Log Viewer" | "Logs" => {
            super::log_viewer::open();
            return;
        }
        "Calendar" => {
            super::calendar_app::open();
            return;
        }
        "Bluetooth" | "Bluetooth Manager" => {
            super::bt_manager::open();
            return;
        }
        "Software Updater" | "Updates" => {
            super::software_updater::open();
            return;
        }
        "Software Center" | "App Store" => {
            super::software_center::open();
            return;
        }
        "Disk Utility" | "Disks" => {
            super::disk_utility::open();
            return;
        }
        "Setup Wizard" => {
            super::setup_wizard::open();
            return;
        }
        _ => {}
    }

    let mut wm = window::WINDOW_MANAGER.lock();

    // Cascade offset: 26px per window (matches browser)
    let cascade = wm.windows.len() as i32;
    let offset = cascade * 26;

    let win = match icon_type {
        IconType::MyPC => {
            let mut w = window::Window::new("Files", 200 + offset, 80 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
        IconType::Terminal => {
            let mut w = window::Window::new("Terminal", 280 + offset, 100 + offset, 860, 540);
            w.content_color = Pixel::rgb(12, 12, 12);
            w
        }
        IconType::AIBrain => {
            let mut w =
                window::Window::new("AI Assistant - KnoxOS", 260 + offset, 70 + offset, 920, 620);
            w.content_color = Pixel::rgb(20, 20, 30);
            w
        }
        IconType::Folder => {
            // Open folder with Unix path in title
            let folder_path = match name {
                "Documents" => "/home/user/Documents",
                "Downloads" => "/home/user/Downloads",
                "Music" => "/home/user/Music",
                "Pictures" => "/home/user/Pictures",
                "Videos" => "/home/user/Videos",
                "Desktop" => "/home/user/Desktop",
                _ => "/home/user",
            };
            let title = alloc::format!("Files - {}", folder_path);
            let mut w = window::Window::new(&title, 220 + offset, 90 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
        IconType::Globe => {
            // If Vivaldi is installed, launch Vivaldi processes and brand the window
            let vivaldi_state = crate::vivaldi::status();
            let title = if vivaldi_state == crate::vivaldi::VivaldiState::Installed
                || vivaldi_state == crate::vivaldi::VivaldiState::Running
                || vivaldi_state == crate::vivaldi::VivaldiState::Stopped
            {
                // Launch Vivaldi if not already running
                if vivaldi_state != crate::vivaldi::VivaldiState::Running {
                    let _ = crate::vivaldi::launch(None);
                }
                "Vivaldi Browser"
            } else {
                "Browser"
            };
            let mut w = window::Window::new(title, 160 + offset, 50 + offset, 1100, 720);
            w.content_color = Pixel::rgb(255, 255, 255);
            w
        }
        IconType::Settings => {
            let mut w = window::Window::new("Settings", 240 + offset, 70 + offset, 900, 620);
            w.content_color = Pixel::rgb(24, 24, 28);
            w
        }
        IconType::Trash => {
            let title = alloc::format!("Files - {}", TRASH_DIR);
            let mut w = window::Window::new(&title, 220 + offset, 90 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
        _ => {
            let mut w = window::Window::new(name, 220 + offset, 90 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
    };

    let wid = win.id;
    let is_terminal = win.content_type == window::WindowContentType::Terminal;
    let is_browser = win.content_type == window::WindowContentType::Browser;
    let is_editor = win.content_type == window::WindowContentType::TextEditor;
    let is_ai = win.content_type == window::WindowContentType::AIAssistant;
    let vivaldi_branded = win.title.contains("Vivaldi");
    let title = alloc::string::String::from(name);
    super::window_events::register_window(wid);
    wm.add_window(win);
    drop(wm);

    // Create a per-window terminal instance if this is a terminal window
    if is_terminal {
        crate::terminal::create_for_window(wid);
        // Initialize with a single tab
        let mut wm2 = window::WINDOW_MANAGER.lock();
        if let Some(win) = wm2.windows.iter_mut().find(|w| w.id == wid) {
            win.terminal_tabs = alloc::vec![wid];
            win.terminal_active_tab = 0;
        }
        drop(wm2);
    }

    // Create a per-window browser instance if this is a browser window
    if is_browser {
        super::browser::create_for_window(wid, vivaldi_branded);
    }

    // Create a per-window editor state if this is a text editor window
    if is_editor {
        super::editor::new_empty(wid);
    }

    // Create a per-window AI assistant state if this is an AI window
    if is_ai {
        super::ai_assistant::create_for_window(wid);
    }

    taskbar::add_entry(wid, &title);
    taskbar::set_active(wid);

    // Play window open sound
    super::sounds::window_open();
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT MENU — Right-click desktop menu
// ═══════════════════════════════════════════════════════════════════════════

const CONTEXT_MENU_WIDTH: u32 = 200;
const CONTEXT_ITEM_HEIGHT: u32 = 28;
const CONTEXT_SEPARATOR_HEIGHT: u32 = 12;

/// Show the desktop context menu at the given position
pub fn show_context_menu(x: i32, y: i32) {
    let mut menu = CONTEXT_MENU.lock();
    menu.visible = true;

    // Calculate menu height
    let mut h: u32 = 8; // padding
    for item in &menu.items {
        h += if item.separator {
            CONTEXT_SEPARATOR_HEIGHT
        } else {
            CONTEXT_ITEM_HEIGHT
        };
    }

    // Clamp position to screen bounds
    let (sw, sh) = super::cached_screen_size();
    let (sw, sh) = (sw as usize, sh as usize);
    let mx = if x + CONTEXT_MENU_WIDTH as i32 > sw as i32 {
        sw as i32 - CONTEXT_MENU_WIDTH as i32 - 4
    } else {
        x
    };
    let my = if y + h as i32 > sh as i32 - super::scale::taskbar_height() as i32 {
        sh as i32 - super::scale::taskbar_height() as i32 - h as i32 - 4
    } else {
        y
    };

    menu.x = mx;
    menu.y = my;
    drop(menu);
    super::request_redraw();
}

/// Close the desktop context menu
pub fn close_context_menu() {
    let mut menu = CONTEXT_MENU.lock();
    if menu.visible {
        menu.visible = false;
        drop(menu);
        super::request_redraw();
    }
}

/// Draw the context menu if visible
fn draw_context_menu(fb: &mut FrameBuffer) {
    let menu = CONTEXT_MENU.lock();
    if !menu.visible {
        return;
    }

    // Calculate menu height
    let mut h: u32 = 8;
    for item in &menu.items {
        h += if item.separator {
            CONTEXT_SEPARATOR_HEIGHT
        } else {
            CONTEXT_ITEM_HEIGHT
        };
    }

    let mx = menu.x;
    let my = menu.y;

    // Shadow (rounded) — AA
    fb.fill_rounded_rect_aa(
        Rect::new(mx + 3, my + 3, CONTEXT_MENU_WIDTH, h),
        Pixel::new(0, 0, 0, 90),
        6,
    );

    // Background (rounded) — AA
    fb.fill_rounded_rect_aa(
        Rect::new(mx, my, CONTEXT_MENU_WIDTH, h),
        Pixel::new(38, 38, 38, 250),
        6,
    );

    // Border (rounded AA)
    fb.draw_rounded_rect(
        Rect::new(mx, my, CONTEXT_MENU_WIDTH, h),
        Pixel::rgb(70, 70, 70),
        6,
        1,
    );

    // Items
    let mut iy = my + 4;
    let mouse = super::input::MOUSE.lock();
    let mouse_x = mouse.x;
    let mouse_y = mouse.y;
    drop(mouse);

    for item in &menu.items {
        if item.separator {
            fb.draw_hline(
                mx + 12,
                iy + CONTEXT_SEPARATOR_HEIGHT as i32 / 2,
                CONTEXT_MENU_WIDTH - 24,
                Pixel::rgb(60, 60, 60),
            );
            iy += CONTEXT_SEPARATOR_HEIGHT as i32;
        } else {
            // Hover highlight with rounded corners
            let item_rect = Rect::new(mx + 4, iy, CONTEXT_MENU_WIDTH - 8, CONTEXT_ITEM_HEIGHT);
            if item_rect.contains(mouse_x, mouse_y) {
                fb.fill_rounded_rect_aa(item_rect, colors::SELECTION, 4);
            }

            // Small icon for each menu item via icon_theme (16×16)
            let icon_x = mx + 8;
            let icon_y = iy + (CONTEXT_ITEM_HEIGHT as i32 - 16) / 2;
            match item.action {
                ContextAction::OpenTerminal => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "utilities-x-terminal",
                    );
                }
                ContextAction::OpenExplorer => {
                    icon_theme::draw_tiny_icon(fb, icon_x, icon_y, IconCategory::Places, "folder");
                }
                ContextAction::OpenBrowser => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "web-browser",
                    );
                }
                ContextAction::Refresh => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Actions,
                        "view-refresh",
                    );
                }
                ContextAction::NewFile => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Actions,
                        "document-new",
                    );
                }
                ContextAction::NewFolder => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Actions,
                        "folder-new",
                    );
                }
                ContextAction::TakeScreenshot | ContextAction::TakeScreenshotRegion => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "accessories-screenshot",
                    );
                }
                ContextAction::ChangeWallpaper => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "accessories-painting",
                    );
                }
                ContextAction::DisplaySettings => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "org.gnome.Settings",
                    );
                }
                _ => {}
            }

            font_engine::draw_ui_text(
                fb,
                mx + 30,
                iy + (CONTEXT_ITEM_HEIGHT as i32 - 12) / 2,
                &item.label,
                12,
                colors::WHITE,
            );
            iy += CONTEXT_ITEM_HEIGHT as i32;
        }
    }
}

/// Handle a click on the context menu, returns true if consumed
pub fn handle_context_menu_click(x: i32, y: i32) -> bool {
    let menu = CONTEXT_MENU.lock();
    if !menu.visible {
        return false;
    }

    let mx = menu.x;
    let mut iy = menu.y + 4;

    for item in &menu.items {
        if item.separator {
            iy += CONTEXT_SEPARATOR_HEIGHT as i32;
            continue;
        }

        let item_rect = Rect::new(mx + 2, iy, CONTEXT_MENU_WIDTH - 4, CONTEXT_ITEM_HEIGHT);
        if item_rect.contains(x, y) {
            let action = item.action;
            drop(menu);
            close_context_menu();

            match action {
                ContextAction::OpenTerminal => {
                    open_application("Terminal", IconType::Terminal);
                }
                ContextAction::OpenExplorer => {
                    open_application("Files", IconType::Folder);
                }
                ContextAction::OpenBrowser => {
                    open_application("Browser", IconType::Globe);
                }
                ContextAction::NewFile => {
                    create_desktop_file();
                }
                ContextAction::NewFolder => {
                    create_desktop_folder();
                }
                ContextAction::Refresh => {
                    super::request_redraw();
                }
                ContextAction::TakeScreenshot => {
                    super::screenshot::take_screenshot(super::screenshot::CaptureMode::FullScreen);
                }
                ContextAction::TakeScreenshotRegion => {
                    super::screenshot::take_screenshot(super::screenshot::CaptureMode::Region);
                }
                ContextAction::ChangeWallpaper => {
                    // Open Settings on Personalization tab
                    open_application("Settings", IconType::Settings);
                    super::settings::SETTINGS_STATE.lock().active_tab =
                        super::settings::SettingsTab::Personalization;
                }
                ContextAction::DisplaySettings => {
                    open_application("Settings", IconType::Settings);
                    super::settings::SETTINGS_STATE.lock().active_tab =
                        super::settings::SettingsTab::Display;
                }
                _ => {}
            }
            return true;
        }
        iy += CONTEXT_ITEM_HEIGHT as i32;
    }

    drop(menu);
    close_context_menu();
    false
}

// ═══════════════════════════════════════════════════════════════════════════
// DESKTOP FILE/FOLDER CREATION (9.29)
// ═══════════════════════════════════════════════════════════════════════════

/// Create a new file on the Desktop directory and add a desktop icon
fn create_desktop_file() {
    use alloc::format;
    let desktop_path = "/home/user/Desktop";

    // Find a unique name
    let mut name = String::from("New File.txt");
    let mut counter = 1u32;
    {
        let vfs = crate::vfs::VFS.lock();
        loop {
            let path = format!("{}/{}", desktop_path, name);
            if vfs.resolve_path(&path).is_none() {
                break;
            }
            counter += 1;
            name = format!("New File ({}).txt", counter);
        }
    }

    // Create the file in VFS
    {
        let mut vfs = crate::vfs::VFS.lock();
        let path = format!("{}/{}", desktop_path, name);
        vfs.write_file(&path, b"");
    }

    // Add a desktop icon for the new file
    {
        let mut desktop = DESKTOP.lock();
        let idx = desktop.icons.len();
        let col = idx % 6;
        let row = idx / 6;
        desktop.icons.push(DesktopIcon {
            name: name.clone(),
            icon_type: IconType::Document,
            x: ICON_GRID_X + (col as i32) * ICON_GRID_SPACING_X,
            y: ICON_GRID_Y + (row as i32) * ICON_GRID_SPACING_Y,
            selected: false,
            is_shortcut: false,
            path: String::new(),
        });
    }

    // Show a notification
    super::notifications::info("Desktop", &format!("Created: {}", name));
    super::request_redraw();
}

/// Create a new folder on the Desktop directory and add a desktop icon
fn create_desktop_folder() {
    use alloc::format;
    let desktop_path = "/home/user/Desktop";

    // Find a unique name
    let mut name = String::from("New Folder");
    let mut counter = 1u32;
    {
        let vfs = crate::vfs::VFS.lock();
        loop {
            let path = format!("{}/{}", desktop_path, name);
            if vfs.resolve_path(&path).is_none() {
                break;
            }
            counter += 1;
            name = format!("New Folder ({})", counter);
        }
    }

    // Create the folder in VFS
    {
        let mut vfs = crate::vfs::VFS.lock();
        let path = format!("{}/{}", desktop_path, name);
        let _ = vfs.mkdir(&path, 0o755);
    }

    // Add a desktop icon for the new folder
    {
        let mut desktop = DESKTOP.lock();
        let idx = desktop.icons.len();
        let col = idx % 6;
        let row = idx / 6;
        desktop.icons.push(DesktopIcon {
            name: name.clone(),
            icon_type: IconType::Folder,
            x: ICON_GRID_X + (col as i32) * ICON_GRID_SPACING_X,
            y: ICON_GRID_Y + (row as i32) * ICON_GRID_SPACING_Y,
            selected: false,
            is_shortcut: false,
            path: String::new(),
        });
    }

    // Show a notification
    super::notifications::info("Desktop", &format!("Created: {}", name));
    super::request_redraw();
}

// ─── File Drop from Explorer to Desktop (9.27) ──────────────────────

/// Accept a file being dropped onto the desktop from an explorer window.
/// Copies the file to /home/user/Desktop and adds a desktop icon.
pub fn accept_file_drop(src_path: &str) {
    use alloc::format;
    let desktop_path = "/home/user/Desktop";
    let name = src_path.rsplit('/').next().unwrap_or("dropped_file");

    // Copy the file to Desktop
    let dst = format!("{}/{}", desktop_path, name);
    match crate::vfs::copy_file_dispatch(src_path, &dst) {
        Ok(()) => {
            // Add a desktop icon
            let icon_type = {
                let vfs = crate::vfs::VFS.lock();
                if let Some(ino) = vfs.resolve_path(src_path) {
                    if let Some(inode) = vfs.get_inode(ino) {
                        if inode.file_type == crate::vfs::FileType::Directory {
                            IconType::Folder
                        } else {
                            IconType::Document
                        }
                    } else {
                        IconType::Document
                    }
                } else {
                    IconType::Document
                }
            };

            let mut desktop = DESKTOP.lock();
            let idx = desktop.icons.len();
            let col = idx % 6;
            let row = idx / 6;
            desktop.icons.push(DesktopIcon {
                name: String::from(name),
                icon_type,
                x: ICON_GRID_X + (col as i32) * ICON_GRID_SPACING_X,
                y: ICON_GRID_Y + (row as i32) * ICON_GRID_SPACING_Y,
                selected: false,
                is_shortcut: false,
                path: String::new(),
            });
            drop(desktop);
            super::notifications::info("Desktop", &format!("Copied {} to Desktop", name));
        }
        Err(_) => {
            super::notifications::error("Desktop", &format!("Failed to copy {} to Desktop", name));
        }
    }
    super::request_redraw();
}

// ═══════════════════════════════════════════════════════════════════════════
// SCREEN MAGNIFICATION LENS (17.9)
// ═══════════════════════════════════════════════════════════════════════════

/// Apply a magnification lens around the cursor position.
/// Reads pixels from the backbuffer, scales them up, and writes back.
fn apply_zoom_lens(fb: &mut FrameBuffer, cx: i32, cy: i32) {
    let zoom = super::accessibility::zoom_level() as u32;
    if zoom <= 100 {
        return;
    }
    let scale = zoom as f32 / 100.0; // e.g. 150 → 1.5×
    let lens_radius = 120i32; // pixel radius of the lens circle

    let w = fb.width as i32;
    let h = fb.height as i32;

    // We iterate over every pixel in the lens area and sample from the source
    // position (closer to cursor center = more magnified).
    // To avoid reading stale data we read from backbuffer into a temp buffer
    // first, then write the magnified result.
    use alloc::vec;
    let diam = (lens_radius * 2) as usize;
    let mut buf = vec![Pixel::rgb(0, 0, 0); diam * diam];

    // Read source pixels into temp buffer
    for dy in -lens_radius..lens_radius {
        for dx in -lens_radius..lens_radius {
            let dist_sq = dx * dx + dy * dy;
            let r_sq = lens_radius * lens_radius;
            if dist_sq > r_sq {
                continue;
            }

            // Source coordinates: map back through the magnification
            let src_x = cx + (dx as f32 / scale) as i32;
            let src_y = cy + (dy as f32 / scale) as i32;

            let pixel = if src_x >= 0 && src_x < w && src_y >= 0 && src_y < h {
                fb.get_pixel(src_x as usize, src_y as usize)
            } else {
                Pixel::rgb(0, 0, 0)
            };

            let bx = (dx + lens_radius) as usize;
            let by = (dy + lens_radius) as usize;
            buf[by * diam + bx] = pixel;
        }
    }

    // Write magnified pixels back + draw circular border
    for dy in -lens_radius..lens_radius {
        for dx in -lens_radius..lens_radius {
            let dist_sq = dx * dx + dy * dy;
            let r_sq = lens_radius * lens_radius;
            if dist_sq > r_sq {
                continue;
            }

            let px = cx + dx;
            let py = cy + dy;
            if px < 0 || px >= w || py < 0 || py >= h {
                continue;
            }

            let bx = (dx + lens_radius) as usize;
            let by = (dy + lens_radius) as usize;

            // Draw border ring (2px thick)
            let outer_r = lens_radius - 1;
            let inner_r = lens_radius - 3;
            if dist_sq >= inner_r * inner_r {
                fb.set_pixel(px as usize, py as usize, Pixel::rgb(0, 200, 255));
            } else {
                fb.set_pixel(px as usize, py as usize, buf[by * diam + bx]);
            }
        }
    }
}
