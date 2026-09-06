/// Icon Theme Loader — Load icons from `./icons/` SVG theme
///
/// Provides a category-aware icon lookup system that maps icon names
/// to pre-rasterized BGRA bitmaps at multiple sizes.
///
/// The icons are rasterized at build time by `tools/svg2rgba` and embedded
/// as static BGRA arrays. This module provides the lookup layer.
///
/// Icon categories (matching `./icons/` directory structure):
///   - apps: Application icons (calculator, editor, browser, terminal, etc.)
///   - places: Folder, trash, home, desktop icons
///   - devices: Computer, drive, printer, phone icons
///   - mimetypes: File type icons (text, image, audio, video, etc.)
///   - actions: Toolbar icons (copy, paste, undo, redo, open, save, etc.)
///   - status: State icons (error, warning, info, network, battery)
///   - panel: System tray icons (volume, wifi, bluetooth, battery)
///   - categories: App category icons (games, office, development, etc.)
///   - emblems: Overlay badges (favorite, shared, read-only, etc.)
///   - animations: Animated state icons (loading spinner, etc.)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::framebuffer::{FrameBuffer, Pixel, Rect};

// ═══════════════════════════════════════════════════════════════════════
// ICON CATEGORIES
// ═══════════════════════════════════════════════════════════════════════

/// Icon category mapping to `./icons/` subdirectories
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IconCategory {
    Apps,
    Places,
    Devices,
    Mimetypes,
    Actions,
    Status,
    Panel,
    Categories,
    Emblems,
    Animations,
}

impl IconCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apps => "apps",
            Self::Places => "places",
            Self::Devices => "devices",
            Self::Mimetypes => "mimetypes",
            Self::Actions => "actions",
            Self::Status => "status",
            Self::Panel => "panel",
            Self::Categories => "categories",
            Self::Emblems => "emblems",
            Self::Animations => "animations",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ICON SIZES
// ═══════════════════════════════════════════════════════════════════════

/// Standard icon sizes (in pixels)
pub const ICON_SIZE_16: u32 = 16;
pub const ICON_SIZE_24: u32 = 24;
pub const ICON_SIZE_32: u32 = 32;
pub const ICON_SIZE_48: u32 = 48;

// ═══════════════════════════════════════════════════════════════════════
// ICON BITMAP
// ═══════════════════════════════════════════════════════════════════════

/// A rasterized icon bitmap (BGRA format)
pub struct IconBitmap {
    pub width: u32,
    pub height: u32,
    /// BGRA pixel data (4 bytes per pixel, row-major)
    pub data: &'static [u8],
}

impl IconBitmap {
    /// Draw this icon onto a framebuffer at (x, y)
    pub fn draw(&self, fb: &mut FrameBuffer, x: i32, y: i32) {
        fb.blit_bgra(x, y, self.width, self.height, self.data);
    }

    /// Draw this icon scaled to a target size
    pub fn draw_scaled(&self, fb: &mut FrameBuffer, x: i32, y: i32, target_size: u32) {
        if target_size == self.width && target_size == self.height {
            self.draw(fb, x, y);
        } else {
            fb.blit_bgra_scaled(
                x,
                y,
                target_size,
                target_size,
                self.data,
                self.width,
                self.height,
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ICON REGISTRY
// ═══════════════════════════════════════════════════════════════════════

/// Icon lookup key
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct IconKey {
    category: IconCategory,
    name: &'static str,
    size: u32,
}

/// Static icon registry — maps (category, name, size) to BGRA data
struct IconEntry {
    width: u32,
    height: u32,
    data: &'static [u8],
}

/// The icon registry — populated during init from auto-generated icon_data
lazy_static::lazy_static! {
    static ref ICON_REGISTRY: Mutex<BTreeMap<(IconCategory, &'static str, u32), IconEntry>> =
        Mutex::new(BTreeMap::new());
}

/// Register a pre-rasterized icon bitmap
pub fn register_icon(
    category: IconCategory,
    name: &'static str,
    size: u32,
    width: u32,
    height: u32,
    data: &'static [u8],
) {
    ICON_REGISTRY.lock().insert(
        (category, name, size),
        IconEntry {
            width,
            height,
            data,
        },
    );
}

/// Look up an icon by category, name, and desired size.
/// Returns the best matching icon (exact size, or nearest larger, or largest available).
pub fn get_icon(category: IconCategory, name: &str, size: u32) -> Option<IconBitmap> {
    let registry = ICON_REGISTRY.lock();

    // Try exact size
    // We need to search through entries since we can't use &str as lookup key directly
    for (&(cat, n, s), entry) in registry.iter() {
        if cat == category && n == name && s == size {
            return Some(IconBitmap {
                width: entry.width,
                height: entry.height,
                data: entry.data,
            });
        }
    }

    // Try nearest larger size
    let mut best: Option<(&IconEntry, u32)> = None;
    for (&(cat, n, s), entry) in registry.iter() {
        if cat == category && n == name && s > size {
            if best.is_none() || s < best.unwrap().1 {
                best = Some((entry, s));
            }
        }
    }
    if let Some((entry, _)) = best {
        return Some(IconBitmap {
            width: entry.width,
            height: entry.height,
            data: entry.data,
        });
    }

    // Try largest available (will be scaled up)
    let mut largest: Option<(&IconEntry, u32)> = None;
    for (&(cat, n, s), entry) in registry.iter() {
        if cat == category && n == name {
            if largest.is_none() || s > largest.unwrap().1 {
                largest = Some((entry, s));
            }
        }
    }
    largest.map(|(entry, _)| IconBitmap {
        width: entry.width,
        height: entry.height,
        data: entry.data,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// WELL-KNOWN ICON NAMES (mapping desktop types to ./icons/ files)
// ═══════════════════════════════════════════════════════════════════════

/// Well-known icon identifiers used by the desktop shell
pub mod names {
    // Places
    pub const FOLDER: &str = "folder";
    pub const FOLDER_OPEN: &str = "folder-open";
    pub const FOLDER_DOCUMENTS: &str = "folder-documents";
    pub const FOLDER_PICTURES: &str = "folder-pictures";
    pub const FOLDER_MUSIC: &str = "folder-music";
    pub const FOLDER_VIDEOS: &str = "folder-videos";
    pub const FOLDER_HOME: &str = "user-home";
    pub const DESKTOP: &str = "user-desktop";
    pub const TRASH: &str = "user-trash";
    pub const NETWORK_SERVER: &str = "network-server";

    // Devices
    pub const COMPUTER: &str = "computer";
    pub const DRIVE_HARDDISK: &str = "drive-harddisk";
    pub const DRIVE_USB: &str = "media-removable";
    pub const PRINTER: &str = "printer";

    // Apps
    pub const TERMINAL: &str = "terminal-1";
    pub const FILE_MANAGER: &str = "file-manager";
    pub const BROWSER: &str = "internet-web-browser";
    pub const TEXT_EDITOR: &str = "accessories-text-editor";
    pub const CALCULATOR: &str = "accessories-calculator";
    pub const IMAGE_VIEWER: &str = "accessories-image-viewer";
    pub const SCREENSHOT: &str = "accessories-screenshot";
    pub const MEDIA_PLAYER: &str = "multimedia-video-player";
    pub const SETTINGS: &str = "org.gnome.Settings";
    pub const SOFTWARE_CENTER: &str = "software-center";
    pub const ARCHIVE_MANAGER: &str = "archive-manager";
    pub const DISK_UTILITY: &str = "disk-utility";
    pub const BLUETOOTH: &str = "bluetooth";
    pub const SYSTEM_MONITOR: &str = "utilities-system-monitor";
    pub const APP_GRID: &str = "appgrid";
    pub const SEARCH: &str = "system-search";

    // Categories
    pub const GAMES: &str = "applications-games";
    pub const OFFICE: &str = "applications-office";
    pub const DEVELOPMENT: &str = "applications-development";
    pub const MULTIMEDIA: &str = "applications-multimedia";
    pub const SYSTEM: &str = "applications-system";

    // Mimetypes
    pub const TEXT_GENERIC: &str = "text-x-generic";
    pub const IMAGE_GENERIC: &str = "image-x-generic";
    pub const AUDIO_GENERIC: &str = "audio-x-generic";
    pub const VIDEO_GENERIC: &str = "video-x-generic";
    pub const APPLICATION_ARCHIVE: &str = "application-x-archive";
    pub const APPLICATION_PDF: &str = "application-pdf";

    // Panel (system tray)
    pub const BATTERY_FULL: &str = "battery-full";
    pub const BATTERY_GOOD: &str = "battery-good";
    pub const BATTERY_LOW: &str = "battery-low";
    pub const BATTERY_EMPTY: &str = "battery-empty";
    pub const BATTERY_CHARGING: &str = "battery-full-charged";
    pub const VOLUME_HIGH: &str = "audio-volume-high";
    pub const VOLUME_MEDIUM: &str = "audio-volume-medium";
    pub const VOLUME_LOW: &str = "audio-volume-low";
    pub const VOLUME_MUTED: &str = "audio-volume-muted";
    pub const NETWORK_WIFI: &str = "nm-device-wireless";
    pub const NETWORK_WIRED: &str = "network-idle";
    pub const NETWORK_OFFLINE: &str = "network-offline";
    pub const BLUETOOTH_ACTIVE: &str = "bluetooth-active";
    pub const BLUETOOTH_DISABLED: &str = "bluetooth-disabled";
    pub const AIRPLANE_MODE: &str = "airplane-mode";

    // Actions
    pub const EDIT_COPY: &str = "edit-copy";
    pub const EDIT_PASTE: &str = "edit-paste";
    pub const EDIT_CUT: &str = "edit-cut";
    pub const EDIT_UNDO: &str = "edit-undo";
    pub const EDIT_REDO: &str = "edit-redo";
    pub const DOCUMENT_OPEN: &str = "document-open";
    pub const DOCUMENT_SAVE: &str = "document-save";
    pub const DOCUMENT_NEW: &str = "document-new";
    pub const GO_BACK: &str = "go-previous";
    pub const GO_FORWARD: &str = "go-next";
    pub const GO_UP: &str = "go-up";
    pub const GO_HOME: &str = "go-home";
    pub const VIEW_REFRESH: &str = "view-refresh";
    pub const WINDOW_CLOSE: &str = "window-close";
    pub const ZOOM_IN: &str = "zoom-in";
    pub const ZOOM_OUT: &str = "zoom-out";
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the icon theme registry from auto-generated icon_data.
/// Call this once during GUI initialization.
pub fn init() {
    // Register all 630 icons (×4 sizes = 2520 entries) from the auto-generated icon_data.
    // The icon_data module was generated by tools/svg2rgba from all SVGs in ./icons/.
    super::icon_data::register_all();

    crate::serial_println!(
        "[IconTheme] Registered {} icons from icon_data",
        ICON_REGISTRY.lock().len()
    );
}

// ═══════════════════════════════════════════════════════════════════════
// CONVENIENCE DRAWING FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

/// Draw a desktop icon (48×48 default) at (x, y).
pub fn draw_desktop_icon(fb: &mut FrameBuffer, x: i32, y: i32, category: IconCategory, name: &str) {
    if let Some(icon) = get_icon(category, name, ICON_SIZE_48) {
        icon.draw_scaled(fb, x, y, ICON_SIZE_48);
    }
}

/// Draw a small icon (24×24) at (x, y) — for start menu, lists.
pub fn draw_small_icon(fb: &mut FrameBuffer, x: i32, y: i32, category: IconCategory, name: &str) {
    if let Some(icon) = get_icon(category, name, ICON_SIZE_24) {
        icon.draw_scaled(fb, x, y, ICON_SIZE_24);
    }
}

/// Draw a tiny icon (16×16) at (x, y) — for taskbar, menus.
pub fn draw_tiny_icon(fb: &mut FrameBuffer, x: i32, y: i32, category: IconCategory, name: &str) {
    if let Some(icon) = get_icon(category, name, ICON_SIZE_16) {
        icon.draw_scaled(fb, x, y, ICON_SIZE_16);
    }
}
