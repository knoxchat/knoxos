use alloc::collections::BTreeMap;
/// Window Manager — "Aurora" Design
/// Frosted glass windows with holographic accent edges, pill controls on the left,
/// ambient glow on focused windows, and large corner radius for a futuristic feel.
/// Nothing like Windows, macOS, or Linux — designed for an AI-native experience.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;

use super::colors;
use super::fonts;

const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};

/// Global atomic current workspace index (for is_visible checks without needing WM lock)
pub static CURRENT_WORKSPACE: AtomicU8 = AtomicU8::new(0);
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window_attrs::{WindowAttributes, WindowButtons, WindowLevel};

/// Window identifier
pub type WindowId = u32;

static NEXT_WINDOW_ID: Mutex<WindowId> = Mutex::new(1);

/// Window state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    /// Snapped to left half of screen
    SnappedLeft,
    /// Snapped to right half of screen
    SnappedRight,
    /// Quarter-snapped to top-left corner
    SnappedTopLeft,
    /// Quarter-snapped to top-right corner
    SnappedTopRight,
    /// Quarter-snapped to bottom-left corner
    SnappedBottomLeft,
    /// Quarter-snapped to bottom-right corner
    SnappedBottomRight,
}

/// Edge/corner for window resizing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ResizeEdge {
    pub fn is_resizing(&self) -> bool {
        !matches!(self, ResizeEdge::None)
    }
}

/// Resize grab zone width in pixels
pub const RESIZE_BORDER: i32 = 5;

/// Type of window content to render
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowContentType {
    Empty,
    Terminal,
    FileExplorer,
    Browser,
    AIAssistant,
    TextEditor,
    Settings,
    ArchiveViewer,
    DiskUtility,
    BluetoothManager,
    CalendarApp,
    LogViewer,
    SoftwareUpdater,
    SoftwareCenter,
    SetupWizard,
    TaskManager,
    Calculator,
    ImageViewer,
}

/// A window in the desktop environment
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub rect: Rect,
    pub saved_rect: Rect,
    pub state: WindowState,
    pub focused: bool,
    pub dragging: bool,
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
    pub resizable: bool,
    pub closeable: bool,
    pub minimizable: bool,
    pub maximizable: bool,
    pub content_color: Pixel,
    pub content_type: WindowContentType,
    /// Current resize edge being dragged
    pub resizing: ResizeEdge,
    /// Pre-snap rect for restoring from snap state
    pub pre_snap_rect: Rect,
    /// Scroll position for content (used by file explorer, browser, settings, etc.)
    pub scroll_y: i32,
    /// Maximum scroll extent (computed from content height)
    pub max_scroll_y: i32,
    /// Whether the window is visible (not hidden by "show desktop")
    pub visible: bool,
    /// Z-order index (higher = on top, updated by WindowManager)
    pub z_order: u32,
    /// Scrollbar drag state: true when user is dragging the scrollbar thumb
    pub scrollbar_dragging: bool,
    /// The Y-coordinate where the scrollbar drag started (screen-space)
    pub scrollbar_drag_start_y: i32,
    /// The scroll_y value when the drag started
    pub scrollbar_drag_start_scroll: i32,

    // ── winit-inspired extended properties ────────────────────────────
    /// Z-ordering level (AlwaysOnBottom / Normal / AlwaysOnTop).
    pub window_level: WindowLevel,
    /// Minimum content size constraint (if set).
    pub min_surface_size: Option<(u32, u32)>,
    /// Maximum content size constraint (if set).
    pub max_surface_size: Option<(u32, u32)>,
    /// Whether the window has decorations (title bar, border).
    pub decorations: bool,
    /// Whether the window supports transparent content.
    pub transparent: bool,
    /// Whether the window has a blurred background effect.
    pub blur: bool,

    // ── Animation state (P8.12) ──────────────────────────────────────
    /// Current animation in progress (None = static, no animation)
    pub anim: Option<WindowAnimation>,

    // ── File Explorer state (Sprint 3) ───────────────────────────────
    /// Currently selected file/folder index in the explorer list (-1 = none)
    pub explorer_selected: i32,
    /// Explorer sort mode
    pub explorer_sort: ExplorerSort,
    /// Sort ascending (true) or descending (false)
    pub explorer_sort_asc: bool,
    /// Navigation history stack (paths)
    pub explorer_history: Vec<String>,
    /// Current position in history stack
    pub explorer_history_idx: usize,
    /// Whether the address bar is being edited
    pub explorer_editing_path: bool,
    /// Address bar text buffer (when editing)
    pub explorer_path_buf: String,
    /// Address bar cursor position
    pub explorer_path_cursor: usize,
    /// File explorer context menu state
    pub explorer_ctx_menu: Option<ExplorerContextMenu>,
    /// File explorer rename mode: Some(index) if renaming an entry
    pub explorer_renaming: Option<usize>,
    /// Rename text buffer
    pub explorer_rename_buf: String,
    /// Show hidden files (dotfiles)
    pub explorer_show_hidden: bool,

    // ── File Explorer search state (9.64) ────────────────────────────
    /// Whether search mode is active in the file explorer
    pub explorer_search_active: bool,
    /// Search query text buffer
    pub explorer_search_query: String,
    /// Search cursor position
    pub explorer_search_cursor: usize,

    // ── File Explorer preview panel (9.65) ───────────────────────────
    /// Whether the preview panel is shown (toggle with Ctrl+P or toolbar button)
    pub explorer_preview_visible: bool,
    /// Cached preview text content (first ~40 lines of selected file)
    pub explorer_preview_content: String,
    /// Path of the file currently previewed (to avoid re-reading)
    pub explorer_preview_path: String,

    // ── File Explorer grid/list view (9.68) ──────────────────────────
    /// true = grid view, false = list view (default)
    pub explorer_grid_view: bool,

    // ── File Explorer sidebar bookmarks ──────────────────────────────
    /// Whether the sidebar bookmarks panel is visible
    pub explorer_sidebar_visible: bool,

    // ── File Explorer thumbnail cache ────────────────────────────────
    /// Cached thumbnails for image files: path -> (bgra_bytes, width, height).
    /// None means decode was attempted but failed (avoid retrying).
    pub thumbnail_cache: BTreeMap<String, Option<(Vec<u8>, u32, u32)>>,
    /// Directory path the cache was built for (invalidate on navigation)
    pub thumbnail_cache_dir: String,

    // ── Terminal tab state (Sprint 4) ────────────────────────────────
    /// Terminal tabs — list of terminal instance IDs (window IDs in TERMINALS map)
    /// For a terminal window, the first entry is the window's own ID.
    /// Additional tabs get synthetic IDs: wid*1000 + tab_index.
    pub terminal_tabs: Vec<u32>,
    /// Currently active terminal tab index
    pub terminal_active_tab: usize,

    // ── Window grouping / tabbed windows ─────────────────────────────
    /// Group ID for tabbed window grouping (0 = ungrouped).
    /// Windows sharing the same nonzero group_id are displayed as tabs.
    pub group_id: u32,
    /// Whether this window is the active tab in its group
    pub group_active: bool,

    // ── Virtual desktop / workspace (Sprint 5) ──────────────────────
    /// Which workspace this window belongs to (0..NUM_WORKSPACES-1)
    /// A value of u8::MAX means "show on all workspaces" (sticky)
    pub workspace: u8,
}

/// File explorer sort criteria
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerSort {
    Name,
    Size,
    Type,
    Date,
}

/// File explorer right-click context menu
#[derive(Debug, Clone)]
pub struct ExplorerContextMenu {
    pub x: i32,
    pub y: i32,
    pub target_index: Option<usize>,
    pub target_path: String,
    pub items: Vec<ExplorerCtxItem>,
}

/// A single context menu item for the file explorer
#[derive(Debug, Clone)]
pub struct ExplorerCtxItem {
    pub label: String,
    pub action: ExplorerAction,
    pub enabled: bool,
}

/// Actions available in the file explorer context menu
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerAction {
    Open,
    Copy,
    Cut,
    Paste,
    Delete,
    Rename,
    NewFolder,
    NewFile,
    ToggleHidden,
    Properties,
}

/// Window animation types (P8.12)
#[derive(Clone, Copy)]
pub enum WindowAnimationType {
    /// Window appearing (scale up + fade in)
    Open,
    /// Window closing (scale down + fade out)
    Close,
    /// Window minimizing (scale to taskbar + fade out)
    Minimize,
    /// Window restoring from minimized (scale up from taskbar + fade in)
    Restore,
    /// Window transitioning rect (maximize, restore-from-max, snap)
    Transition,
}

/// Active window animation state
#[derive(Clone, Copy)]
pub struct WindowAnimation {
    pub kind: WindowAnimationType,
    /// Progress from 0.0 (start) to 1.0 (complete)
    pub progress: f32,
    /// TSC timestamp when animation started
    pub start_tsc: u64,
    /// Animation duration in TSC ticks
    pub duration_ticks: u64,
    /// Source rect (for interpolation)
    pub from_rect: Rect,
    /// Target rect (for interpolation)
    pub to_rect: Rect,
}

/// Animation duration: ~150ms (fast, responsive feel)
pub const ANIM_DURATION_MS: u64 = 150;

// ease_out_cubic moved to wm_animation.rs

// Aurora design constants (base values at 1080p):
/// Title bar height: 38px (taller for breathing room & pill controls)
pub const TITLE_BAR_HEIGHT: u32 = 38;
/// Title bar button width: 45px each (hit target for pill area)
pub const TITLE_BUTTON_WIDTH: u32 = 45;
/// Window corner radius for portal rounding
pub const PORTAL_RADIUS: u32 = 12;
/// Pill control dot radius
pub const PILL_RADIUS: u32 = 6;
/// Pill spacing between dots
pub const PILL_SPACING: i32 = 22;
/// Pill left margin from window edge
pub const PILL_MARGIN_LEFT: i32 = 16;
/// Window outline: 1px holographic border
pub const BORDER_WIDTH: u32 = 1;
/// Minimum window size: 240×120 (ensures title bar + at least some content visible)
pub const MIN_WIDTH: u32 = 240;
pub const MIN_HEIGHT: u32 = 120;
/// Maximum scroll offset to prevent infinite scrolling
pub const MAX_SCROLL_Y: i32 = 100_000;
/// Minimum visible pixels on screen (prevents dragging entirely off-screen)
pub const MIN_VISIBLE_PX: i32 = 40;
/// Default cascade offset between new windows
pub const CASCADE_OFFSET: i32 = 26;

// ─── Window Decorations Customization ────────────────────────────────

/// Customizable window decoration parameters
pub struct WindowDecorations {
    /// Shadow size in pixels (0 = no shadow, default ~12)
    pub shadow_size: u8,
    /// Window corner radius override (0 = square, default = PORTAL_RADIUS)
    pub border_radius: u8,
    /// Window content opacity (0-255, 255 = fully opaque)
    pub opacity: u8,
}

impl Default for WindowDecorations {
    fn default() -> Self {
        Self {
            shadow_size: 12,
            border_radius: PORTAL_RADIUS as u8,
            opacity: 255,
        }
    }
}

lazy_static::lazy_static! {
    /// Global window decoration settings
    pub static ref WINDOW_DECORATIONS: Mutex<WindowDecorations> =
        Mutex::new(WindowDecorations::default());
}

/// Set window shadow size (0 = no shadow, max 32)
pub fn set_shadow_size(size: u8) {
    WINDOW_DECORATIONS.lock().shadow_size = size.min(32);
}

/// Set window border radius (0 = square corners, max 32)
pub fn set_border_radius(radius: u8) {
    WINDOW_DECORATIONS.lock().border_radius = radius.min(32);
}

/// Set window opacity (0 = fully transparent, 255 = fully opaque)
pub fn set_window_opacity(opacity: u8) {
    WINDOW_DECORATIONS.lock().opacity = opacity;
}

/// Get the current effective border radius (customized or default)
pub fn effective_border_radius() -> u32 {
    WINDOW_DECORATIONS.lock().border_radius as u32
}

/// Get the current shadow size
pub fn effective_shadow_size() -> u8 {
    WINDOW_DECORATIONS.lock().shadow_size
}

// ─── Scale-aware decoration geometry ─────────────────────────────────
// These functions replace raw constant usage so that all window chrome
// remains properly sized and clickable at every display resolution.
use super::scale;

/// Scaled title bar height
#[inline]
pub fn scaled_title_bar_height() -> u32 {
    scale::title_bar_height()
}
/// Scaled button width
#[inline]
pub fn scaled_btn_width() -> i32 {
    scale::btn_width() as i32
}
/// Scaled button height
#[inline]
pub fn scaled_btn_height() -> i32 {
    scale::btn_height() as i32
}
/// Scaled gap between buttons
#[inline]
pub fn scaled_btn_gap() -> i32 {
    scale::btn_gap() as i32
}
/// Scaled button margin from right edge
#[inline]
pub fn scaled_btn_margin_right() -> i32 {
    scale::btn_margin_right() as i32
}
/// Scaled corner radius
#[inline]
pub fn scaled_portal_radius() -> u32 {
    scale::portal_radius()
}
/// Scaled resize border
#[inline]
pub fn scaled_resize_border() -> i32 {
    scale::resize_border()
}
/// Scaled minimum width
#[inline]
pub fn scaled_min_width() -> u32 {
    scale::min_width()
}
/// Scaled minimum height
#[inline]
pub fn scaled_min_height() -> u32 {
    scale::min_height()
}
/// Scaled minimum visible pixels
#[inline]
pub fn scaled_min_visible() -> i32 {
    scale::min_visible_px()
}
/// Scaled glyph half for close X
#[inline]
pub fn scaled_glyph_half() -> i32 {
    scale::glyph_half()
}
/// Scaled dash half for minimize
#[inline]
pub fn scaled_dash_half() -> i32 {
    scale::dash_half()
}
/// Scaled box half for maximize
#[inline]
pub fn scaled_max_box_half() -> i32 {
    scale::max_box_half()
}
/// Scaled title left padding
#[inline]
pub fn scaled_title_pad_left() -> i32 {
    scale::title_pad_left()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Window Rules Engine — Per-app default size, position, workspace, floating
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// A rule that matches windows by content type or title substring and
/// applies default properties when the window is created.
#[derive(Clone)]
pub struct WindowRule {
    /// Match by content type (None = match all)
    pub match_content_type: Option<WindowContentType>,
    /// Match by title substring (empty = match all)
    pub match_title: String,
    /// Override default width
    pub width: Option<u32>,
    /// Override default height
    pub height: Option<u32>,
    /// Override default x position
    pub x: Option<i32>,
    /// Override default y position
    pub y: Option<i32>,
    /// Target workspace (None = current)
    pub workspace: Option<u8>,
    /// Start maximized
    pub maximized: bool,
    /// Force floating (override tiling mode)
    pub floating: bool,
    /// Window level override
    pub always_on_top: bool,
}

impl WindowRule {
    pub fn new() -> Self {
        Self {
            match_content_type: None,
            match_title: String::new(),
            width: None,
            height: None,
            x: None,
            y: None,
            workspace: None,
            maximized: false,
            floating: false,
            always_on_top: false,
        }
    }

    /// Check if this rule matches a given window
    pub fn matches(&self, content_type: WindowContentType, title: &str) -> bool {
        let type_match = self
            .match_content_type
            .is_none_or(|ct| ct as u8 == content_type as u8);
        let title_match = self.match_title.is_empty() || title.contains(self.match_title.as_str());
        type_match && title_match
    }
}

lazy_static::lazy_static! {
    /// Global window rules list
    pub static ref WINDOW_RULES: Mutex<Vec<WindowRule>> = Mutex::new(Vec::new());
}

/// Add a window rule
pub fn add_window_rule(rule: WindowRule) {
    WINDOW_RULES.lock().push(rule);
}

/// Remove all window rules
pub fn clear_window_rules() {
    WINDOW_RULES.lock().clear();
}

/// Apply matching window rules to a window before it's added to the WM.
/// Called from add_window().
pub(crate) fn apply_window_rules(window: &mut Window) {
    let rules = WINDOW_RULES.lock();
    for rule in rules.iter() {
        if rule.matches(window.content_type, &window.title) {
            if let Some(w) = rule.width {
                window.rect.width = w;
            }
            if let Some(h) = rule.height {
                window.rect.height = h;
            }
            if let Some(x) = rule.x {
                window.rect.x = x;
            }
            if let Some(y) = rule.y {
                window.rect.y = y;
            }
            if let Some(ws) = rule.workspace {
                window.workspace = ws;
            }
            if rule.always_on_top {
                window.window_level = WindowLevel::AlwaysOnTop;
            }
            // First matching rule wins
            break;
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Persistent Window Layouts — Save/restore workspace arrangements
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Saved state of a single window in a layout
#[derive(Clone)]
pub struct SavedWindowState {
    pub title: String,
    pub content_type: WindowContentType,
    pub rect: Rect,
    pub workspace: u8,
    pub state: WindowState,
    pub window_level: WindowLevel,
}

/// A named layout that can be saved and restored
#[derive(Clone)]
pub struct WindowLayout {
    pub name: String,
    pub windows: Vec<SavedWindowState>,
}

lazy_static::lazy_static! {
    /// Saved window layouts (up to 8)
    pub static ref SAVED_LAYOUTS: Mutex<Vec<WindowLayout>> = Mutex::new(Vec::new());
}

/// Save the current window arrangement as a named layout
pub fn save_layout(name: &str) {
    let wm = WINDOW_MANAGER.lock();
    let mut layout = WindowLayout {
        name: String::from(name),
        windows: Vec::new(),
    };
    for w in &wm.windows {
        layout.windows.push(SavedWindowState {
            title: w.title.clone(),
            content_type: w.content_type,
            rect: w.rect,
            workspace: w.workspace,
            state: w.state,
            window_level: w.window_level,
        });
    }
    drop(wm);

    let mut layouts = SAVED_LAYOUTS.lock();
    // Replace existing layout with same name, or add new
    if let Some(existing) = layouts.iter_mut().find(|l| l.name == name) {
        *existing = layout;
    } else {
        if layouts.len() >= 8 {
            layouts.remove(0); // evict oldest if at capacity
        }
        layouts.push(layout);
    }
}

/// Restore a saved layout by name. Repositions existing windows to match.
pub fn restore_layout(name: &str) -> bool {
    let layouts = SAVED_LAYOUTS.lock();
    let layout = match layouts.iter().find(|l| l.name == name) {
        Some(l) => l.clone(),
        None => return false,
    };
    drop(layouts);

    let mut wm = WINDOW_MANAGER.lock();
    // Match saved windows to actual windows by content_type + title
    for saved in &layout.windows {
        if let Some(win) = wm
            .windows
            .iter_mut()
            .find(|w| w.content_type as u8 == saved.content_type as u8 && w.title == saved.title)
        {
            win.rect = saved.rect;
            win.saved_rect = saved.rect;
            win.workspace = saved.workspace;
            win.state = saved.state;
            win.window_level = saved.window_level;
        }
    }
    true
}

/// List saved layout names
pub fn list_layouts() -> Vec<String> {
    SAVED_LAYOUTS
        .lock()
        .iter()
        .map(|l| l.name.clone())
        .collect()
}

/// Delete a saved layout by name
pub fn delete_layout(name: &str) -> bool {
    let mut layouts = SAVED_LAYOUTS.lock();
    let len_before = layouts.len();
    layouts.retain(|l| l.name != name);
    layouts.len() < len_before
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Window Grouping / Tabbed Windows
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use core::sync::atomic::AtomicU32;

static NEXT_GROUP_ID: AtomicU32 = AtomicU32::new(1);

/// Allocate a new unique group ID
fn next_group_id() -> u32 {
    NEXT_GROUP_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

/// Group two windows together into a tabbed group.
/// If either already belongs to a group, the other joins that group.
/// If neither has a group, a new group is created.
pub fn group_windows(id_a: WindowId, id_b: WindowId) {
    let mut wm = WINDOW_MANAGER.lock();
    let gid_a = wm
        .windows
        .iter()
        .find(|w| w.id == id_a)
        .map(|w| w.group_id)
        .unwrap_or(0);
    let gid_b = wm
        .windows
        .iter()
        .find(|w| w.id == id_b)
        .map(|w| w.group_id)
        .unwrap_or(0);

    let gid = if gid_a != 0 {
        gid_a
    } else if gid_b != 0 {
        gid_b
    } else {
        next_group_id()
    };

    for w in wm.windows.iter_mut() {
        if w.id == id_a || w.id == id_b {
            w.group_id = gid;
        }
    }
    // Make id_b the active tab
    for w in wm.windows.iter_mut() {
        if w.group_id == gid {
            w.group_active = w.id == id_b;
        }
    }
}

/// Remove a window from its group. If only one window remains in the
/// group, ungroup it too.
pub fn ungroup_window(id: WindowId) {
    let mut wm = WINDOW_MANAGER.lock();
    let gid = wm
        .windows
        .iter()
        .find(|w| w.id == id)
        .map(|w| w.group_id)
        .unwrap_or(0);
    if gid == 0 {
        return;
    }

    // Remove from group
    if let Some(w) = wm.windows.iter_mut().find(|w| w.id == id) {
        w.group_id = 0;
        w.group_active = true;
    }

    // Count remaining in group
    let remaining: Vec<WindowId> = wm
        .windows
        .iter()
        .filter(|w| w.group_id == gid)
        .map(|w| w.id)
        .collect();

    if remaining.len() == 1 {
        // Only one left — ungroup it
        if let Some(w) = wm.windows.iter_mut().find(|w| w.id == remaining[0]) {
            w.group_id = 0;
            w.group_active = true;
        }
    } else if !remaining.is_empty() {
        // Ensure at least one is active
        let has_active = wm
            .windows
            .iter()
            .any(|w| w.group_id == gid && w.group_active);
        if !has_active {
            let first = remaining[0];
            if let Some(w) = wm.windows.iter_mut().find(|w| w.id == first) {
                w.group_active = true;
            }
        }
    }
}

/// Switch the active tab within a group
pub fn switch_group_tab(gid: u32, target_id: WindowId) {
    let mut wm = WINDOW_MANAGER.lock();
    for w in wm.windows.iter_mut() {
        if w.group_id == gid {
            w.group_active = w.id == target_id;
        }
    }
}

/// Get all windows in a group (by group id), returns (id, title, is_active)
pub fn get_group_members(gid: u32) -> Vec<(WindowId, String, bool)> {
    let wm = WINDOW_MANAGER.lock();
    wm.windows
        .iter()
        .filter(|w| w.group_id == gid)
        .map(|w| (w.id, w.title.clone(), w.group_active))
        .collect()
}

impl Window {
    pub fn new(title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut id_lock = NEXT_WINDOW_ID.lock();
        let id = *id_lock;
        *id_lock += 1;

        let content_type = if title.contains("Terminal") {
            WindowContentType::Terminal
        } else if title.contains("Files")
            || title.contains("/home")
            || title.contains("File Manager")
        {
            WindowContentType::FileExplorer
        } else if title.contains("Browser") || title.contains("Vivaldi") {
            WindowContentType::Browser
        } else if title.contains("AI") {
            WindowContentType::AIAssistant
        } else if title.contains("Editor") || title.contains("Vim") || title.contains("Monaco") {
            WindowContentType::TextEditor
        } else if title.contains("Settings") {
            WindowContentType::Settings
        } else if title.contains("Task Manager") {
            WindowContentType::TaskManager
        } else if title.contains("Calculator") {
            WindowContentType::Calculator
        } else if title.contains("Image Viewer") {
            WindowContentType::ImageViewer
        } else if title.contains("Log Viewer") || title.contains("Logs") {
            WindowContentType::LogViewer
        } else if title.contains("Calendar") {
            WindowContentType::CalendarApp
        } else if title.contains("Archive") {
            WindowContentType::ArchiveViewer
        } else if title.contains("Disk") || title.contains("Disks") {
            WindowContentType::DiskUtility
        } else if title.contains("Bluetooth") {
            WindowContentType::BluetoothManager
        } else if title.contains("Software Updater") || title.contains("Updates") {
            WindowContentType::SoftwareUpdater
        } else if title.contains("Software Center") || title.contains("App Store") {
            WindowContentType::SoftwareCenter
        } else if title.contains("Setup") || title.contains("Wizard") {
            WindowContentType::SetupWizard
        } else {
            WindowContentType::Empty
        };

        Self {
            id,
            title: String::from(title),
            rect: Rect::new(x, y, width, height),
            saved_rect: Rect::new(x, y, width, height),
            state: WindowState::Normal,
            focused: true,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            resizable: true,
            closeable: true,
            minimizable: true,
            maximizable: true,
            content_color: colors::WINDOW_BG,
            content_type,
            resizing: ResizeEdge::None,
            pre_snap_rect: Rect::new(x, y, width, height),
            scroll_y: 0,
            max_scroll_y: MAX_SCROLL_Y,
            visible: true,
            z_order: 0,
            scrollbar_dragging: false,
            scrollbar_drag_start_y: 0,
            scrollbar_drag_start_scroll: 0,
            window_level: WindowLevel::Normal,
            min_surface_size: None,
            max_surface_size: None,
            decorations: true,
            transparent: false,
            blur: false,
            anim: None,
            explorer_selected: -1,
            explorer_sort: ExplorerSort::Name,
            explorer_sort_asc: true,
            explorer_history: alloc::vec![String::from("/home/user")],
            explorer_history_idx: 0,
            explorer_editing_path: false,
            explorer_path_buf: String::new(),
            explorer_path_cursor: 0,
            explorer_ctx_menu: None,
            explorer_renaming: None,
            explorer_rename_buf: String::new(),
            explorer_show_hidden: false,
            explorer_search_active: false,
            explorer_search_query: String::new(),
            explorer_search_cursor: 0,
            explorer_preview_visible: false,
            explorer_preview_content: String::new(),
            explorer_preview_path: String::new(),
            explorer_grid_view: false,
            explorer_sidebar_visible: true,
            thumbnail_cache: BTreeMap::new(),
            thumbnail_cache_dir: String::new(),
            terminal_tabs: Vec::new(),
            terminal_active_tab: 0,
            group_id: 0,
            group_active: true,
            workspace: 0,
        }
    }

    /// Create a window from a `WindowAttributes` builder.
    ///
    /// This is the winit-inspired API for rich window configuration.
    /// If no position is specified, uses (100, 100) as default.
    pub fn from_attributes(attrs: WindowAttributes) -> Self {
        let mut id_lock = NEXT_WINDOW_ID.lock();
        let id = *id_lock;
        *id_lock += 1;

        let (x, y) = attrs.position.unwrap_or((100, 100));
        let content_type = attrs.content_type;
        let state = if attrs.maximized {
            WindowState::Maximized
        } else {
            WindowState::Normal
        };

        Self {
            id,
            title: attrs.title,
            rect: Rect::new(x, y, attrs.surface_width, attrs.surface_height),
            saved_rect: Rect::new(x, y, attrs.surface_width, attrs.surface_height),
            state,
            focused: attrs.active,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            resizable: attrs.resizable,
            closeable: attrs.enabled_buttons.contains(WindowButtons::CLOSE),
            minimizable: attrs.enabled_buttons.contains(WindowButtons::MINIMIZE),
            maximizable: attrs.enabled_buttons.contains(WindowButtons::MAXIMIZE),
            content_color: colors::WINDOW_BG,
            content_type,
            resizing: ResizeEdge::None,
            pre_snap_rect: Rect::new(x, y, attrs.surface_width, attrs.surface_height),
            scroll_y: 0,
            max_scroll_y: MAX_SCROLL_Y,
            visible: attrs.visible,
            z_order: 0,
            scrollbar_dragging: false,
            scrollbar_drag_start_y: 0,
            scrollbar_drag_start_scroll: 0,
            window_level: attrs.window_level,
            min_surface_size: attrs.min_surface_size,
            max_surface_size: attrs.max_surface_size,
            decorations: attrs.decorations,
            transparent: attrs.transparent,
            blur: attrs.blur,
            anim: None,
            explorer_selected: -1,
            explorer_sort: ExplorerSort::Name,
            explorer_sort_asc: true,
            explorer_history: alloc::vec![String::from("/home/user")],
            explorer_history_idx: 0,
            explorer_editing_path: false,
            explorer_path_buf: String::new(),
            explorer_path_cursor: 0,
            explorer_ctx_menu: None,
            explorer_renaming: None,
            explorer_rename_buf: String::new(),
            explorer_show_hidden: false,
            explorer_search_active: false,
            explorer_search_query: String::new(),
            explorer_search_cursor: 0,
            explorer_preview_visible: false,
            explorer_preview_content: String::new(),
            explorer_preview_path: String::new(),
            explorer_grid_view: false,
            explorer_sidebar_visible: true,
            thumbnail_cache: BTreeMap::new(),
            thumbnail_cache_dir: String::new(),
            terminal_tabs: Vec::new(),
            terminal_active_tab: 0,
            group_id: 0,
            group_active: true,
            workspace: 0,
        }
    }

    /// Clamp scroll_y to valid range [0, max_scroll_y]
    pub fn clamp_scroll(&mut self) {
        if self.scroll_y < 0 {
            self.scroll_y = 0;
        }
        if self.scroll_y > self.max_scroll_y {
            self.scroll_y = self.max_scroll_y;
        }
    }

    /// Set scroll position with automatic clamping
    pub fn set_scroll(&mut self, y: i32) {
        self.scroll_y = y;
        self.clamp_scroll();
    }

    /// Scroll by delta with clamping
    pub fn scroll_by(&mut self, delta: i32) {
        self.scroll_y += delta;
        self.clamp_scroll();
    }

    /// Get the bounding rect including shadow area (for damage tracking)
    pub fn damage_rect(&self) -> Rect {
        let pad = 20i32;
        Rect::new(
            self.rect.x - pad,
            self.rect.y - pad,
            self.rect.width + pad as u32 * 2,
            self.rect.height + pad as u32 * 2,
        )
    }

    /// Check if this window is effectively visible (not minimized, visible flag set,
    /// and on the current workspace).
    /// Windows with active animations are still visible during the animation.
    /// Windows with workspace == WORKSPACE_ALL are visible on all workspaces.
    pub fn is_visible(&self) -> bool {
        if self.anim.is_some() {
            return self.visible; // show during animations even if minimized
        }
        let on_workspace = self.workspace == WORKSPACE_ALL
            || self.workspace == CURRENT_WORKSPACE.load(Ordering::Relaxed);
        self.visible && self.state != WindowState::Minimized && on_workspace
    }

    // Animation methods moved to wm_animation.rs

    /// Ensure the window stays at least partially on screen — scale-aware
    pub fn clamp_to_screen(&mut self, screen_w: i32, screen_h: i32) {
        let taskbar_h = scale::taskbar_height() as i32;
        let usable_h = screen_h - taskbar_h;
        let mvp = scaled_min_visible();
        let tb = scaled_title_bar_height() as i32;

        // Ensure at least MIN_VISIBLE_PX of the window is visible on each axis
        if self.rect.x + (self.rect.width as i32) < mvp {
            self.rect.x = mvp - self.rect.width as i32;
        }
        if self.rect.x > screen_w - mvp {
            self.rect.x = screen_w - mvp;
        }
        // Top: don't let title bar go above screen
        if self.rect.y < 0 {
            self.rect.y = 0;
        }
        // Bottom: keep at least title bar visible above taskbar
        if self.rect.y > usable_h - tb {
            self.rect.y = usable_h - tb;
        }
    }

    /// Enforce minimum and maximum size constraints — scale-aware
    pub fn enforce_min_size(&mut self) {
        // Use per-window min constraint if set, otherwise use global scaled minimum
        let (mw, mh) = self
            .min_surface_size
            .unwrap_or_else(|| (scaled_min_width(), scaled_min_height()));
        if self.rect.width < mw {
            self.rect.width = mw;
        }
        if self.rect.height < mh {
            self.rect.height = mh;
        }
        // Enforce maximum constraint if set
        if let Some((max_w, max_h)) = self.max_surface_size {
            if self.rect.width > max_w {
                self.rect.width = max_w;
            }
            if self.rect.height > max_h {
                self.rect.height = max_h;
            }
        }
    }

    /// Get the title bar rectangle — scale-aware
    pub fn title_bar_rect(&self) -> Rect {
        Rect::new(
            self.rect.x,
            self.rect.y,
            self.rect.width,
            scaled_title_bar_height(),
        )
    }

    /// Get the close button rectangle (rightmost) — scale-aware
    pub fn close_button_rect(&self) -> Rect {
        let btn_w = scaled_btn_width();
        let btn_h = scaled_btn_height();
        let margin = scaled_btn_margin_right();
        let top_pad = (scaled_title_bar_height() as i32 - btn_h) / 2;
        let bx = self.rect.x + self.rect.width as i32 - btn_w - margin;
        Rect::new(bx, self.rect.y + top_pad, btn_w as u32, btn_h as u32)
    }

    /// Get the maximize button rectangle (middle) — scale-aware
    pub fn maximize_button_rect(&self) -> Rect {
        let btn_w = scaled_btn_width();
        let btn_h = scaled_btn_height();
        let margin = scaled_btn_margin_right();
        let gap = scaled_btn_gap();
        let top_pad = (scaled_title_bar_height() as i32 - btn_h) / 2;
        let close_x = self.rect.x + self.rect.width as i32 - btn_w - margin;
        let bx = close_x - btn_w - gap;
        Rect::new(bx, self.rect.y + top_pad, btn_w as u32, btn_h as u32)
    }

    /// Get the minimize button rectangle (leftmost of three) — scale-aware
    pub fn minimize_button_rect(&self) -> Rect {
        let btn_w = scaled_btn_width();
        let btn_h = scaled_btn_height();
        let margin = scaled_btn_margin_right();
        let gap = scaled_btn_gap();
        let top_pad = (scaled_title_bar_height() as i32 - btn_h) / 2;
        let close_x = self.rect.x + self.rect.width as i32 - btn_w - margin;
        let max_x = close_x - btn_w - gap;
        let bx = max_x - btn_w - gap;
        Rect::new(bx, self.rect.y + top_pad, btn_w as u32, btn_h as u32)
    }

    /// Get the content area rectangle — scale-aware
    pub fn content_rect(&self) -> Rect {
        let tb = scaled_title_bar_height();
        Rect::new(
            self.rect.x + BORDER_WIDTH as i32,
            self.rect.y + tb as i32,
            self.rect.width - BORDER_WIDTH * 2,
            self.rect.height.saturating_sub(tb + BORDER_WIDTH),
        )
    }

    // draw() and draw_window_icon() moved to wm_chrome.rs

    /// Draw window-specific content
    pub(crate) fn draw_content(&mut self, fb: &mut FrameBuffer) {
        let content = self.content_rect();

        match self.content_type {
            WindowContentType::Terminal => self.draw_terminal_content(fb, content),
            WindowContentType::FileExplorer => self.draw_file_explorer_content(fb, content),
            WindowContentType::Browser => self.draw_browser_content(fb, content),
            WindowContentType::AIAssistant => {
                super::ai_assistant::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::TextEditor => self.draw_editor_content(fb, content),
            WindowContentType::Settings => self.draw_settings_content(fb, content),
            WindowContentType::ArchiveViewer => {
                super::archive_manager::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::DiskUtility => {
                super::disk_utility::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::BluetoothManager => {
                super::bt_manager::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::CalendarApp => {
                super::calendar_app::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::LogViewer => {
                super::log_viewer::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::SoftwareUpdater => {
                super::software_updater::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::SoftwareCenter => {
                super::software_center::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::SetupWizard => {
                super::setup_wizard::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::TaskManager => {
                super::task_manager::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::Calculator => {
                super::calculator::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::ImageViewer => {
                super::image_viewer::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::Empty => {}
        }
    }

    fn draw_terminal_content(&self, fb: &mut FrameBuffer, content: Rect) {
        // ── Terminal tab bar (only if multiple tabs) ──
        let tab_bar_h: i32 = if self.terminal_tabs.len() > 1 { 26 } else { 0 };

        if tab_bar_h > 0 {
            let tab_bg = Pixel::rgb(30, 30, 30);
            let tab_active_bg = Pixel::rgb(12, 12, 12);
            let tab_text = Pixel::rgb(180, 180, 180);
            let tab_active_text = Pixel::rgb(255, 255, 255);
            let tab_border = Pixel::rgb(50, 50, 50);

            // Tab bar background
            fb.fill_rect(
                Rect::new(content.x, content.y, content.width, tab_bar_h as u32),
                tab_bg,
            );
            // Bottom border
            fb.draw_hline(
                content.x,
                content.y + tab_bar_h - 1,
                content.width,
                tab_border,
            );

            let tab_w = 140i32.min(content.width as i32 / self.terminal_tabs.len().max(1) as i32);
            for (i, _tid) in self.terminal_tabs.iter().enumerate() {
                let tx = content.x + (i as i32) * tab_w;
                let is_active = i == self.terminal_active_tab;

                if is_active {
                    fb.fill_rect(
                        Rect::new(tx, content.y, tab_w as u32, tab_bar_h as u32),
                        tab_active_bg,
                    );
                }

                let label = alloc::format!("Shell {}", i + 1);
                let text_color = if is_active { tab_active_text } else { tab_text };
                fonts::draw_string_compact(fb, tx + 8, content.y + 6, &label, text_color, 1);

                // Tab close button (x) for non-first tabs
                if self.terminal_tabs.len() > 1 {
                    let cx = tx + tab_w - 18;
                    let cy = content.y + 6;
                    fonts::draw_string_compact(fb, cx, cy, "×", Pixel::rgb(120, 120, 120), 1);
                }

                // Tab separator
                if i > 0 {
                    fb.draw_vline(tx, content.y + 4, (tab_bar_h - 8) as u32, tab_border);
                }
            }

            // "+" button after last tab
            let plus_x = content.x + (self.terminal_tabs.len() as i32) * tab_w;
            if plus_x + 24 < content.x + content.width as i32 {
                fonts::draw_string_compact(
                    fb,
                    plus_x + 8,
                    content.y + 6,
                    "+",
                    Pixel::rgb(100, 100, 100),
                    1,
                );
            }
        }

        // ── Terminal content area (below tab bar) ──
        let term_area = Rect::new(
            content.x,
            content.y + tab_bar_h,
            content.width,
            content.height.saturating_sub(tab_bar_h as u32),
        );

        // Determine which terminal ID to render
        let term_id = if !self.terminal_tabs.is_empty() {
            self.terminal_tabs
                .get(self.terminal_active_tab)
                .copied()
                .unwrap_or(self.id)
        } else {
            self.id
        };

        crate::terminal::render_for_window(term_id, fb, term_area);
    }

    fn draw_file_explorer_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        use super::explorer;

        let current_path = explorer::current_path(&self.title);

        // === Read and sort directory entries using explorer module ===
        let mut entries = explorer::read_entries(&current_path, self.explorer_show_hidden);
        explorer::sort_entries(&mut entries, self.explorer_sort, self.explorer_sort_asc);

        // === Apply search filter if search mode is active ===
        if self.explorer_search_active && !self.explorer_search_query.is_empty() {
            entries = explorer::filter_entries(&entries, &self.explorer_search_query);
        }

        // === Navigation bar (35px) ===
        let nav_h: u32 = 35;
        fb.fill_rect(
            Rect::new(content.x, content.y, content.width, nav_h),
            Pixel::rgb(40, 40, 40),
        );
        fb.draw_hline(
            content.x,
            content.y + nav_h as i32,
            content.width,
            Pixel::rgb(60, 60, 60),
        );

        // Navigation buttons (back/forward/up) with active/dim states
        {
            let can_back = self.explorer_history_idx > 0;
            let can_forward = self.explorer_history_idx + 1 < self.explorer_history.len();
            let can_up = current_path != "/";

            let btn_active = Pixel::rgb(180, 185, 195);
            let btn_dim = Pixel::rgb(80, 85, 95);
            let btn_y = content.y + 8;

            // Back arrow (◄)
            let back_c = if can_back { btn_active } else { btn_dim };
            let bx = content.x + 10;
            fb.draw_line_aa(bx + 6, btn_y, bx, btn_y + 6, back_c);
            fb.draw_line_aa(bx, btn_y + 6, bx + 6, btn_y + 12, back_c);
            fb.draw_line_aa(bx + 7, btn_y, bx + 1, btn_y + 6, back_c);
            fb.draw_line_aa(bx + 1, btn_y + 6, bx + 7, btn_y + 12, back_c);

            // Forward arrow (►)
            let fwd_c = if can_forward { btn_active } else { btn_dim };
            let fx = content.x + 28;
            fb.draw_line_aa(fx, btn_y, fx + 6, btn_y + 6, fwd_c);
            fb.draw_line_aa(fx + 6, btn_y + 6, fx, btn_y + 12, fwd_c);
            fb.draw_line_aa(fx + 1, btn_y, fx + 7, btn_y + 6, fwd_c);
            fb.draw_line_aa(fx + 7, btn_y + 6, fx + 1, btn_y + 12, fwd_c);

            // Up arrow (▲)
            let up_c = if can_up { btn_active } else { btn_dim };
            let ux = content.x + 52;
            fb.draw_line_aa(ux, btn_y + 8, ux + 6, btn_y + 2, up_c);
            fb.draw_line_aa(ux + 6, btn_y + 2, ux + 12, btn_y + 8, up_c);
            fb.draw_line_aa(ux, btn_y + 9, ux + 6, btn_y + 3, up_c);
            fb.draw_line_aa(ux + 6, btn_y + 3, ux + 12, btn_y + 9, up_c);
        }

        // Address bar (editable)
        let path_bar_w = content.width.saturating_sub(90);
        let path_bar_rect = Rect::new(content.x + 80, content.y + 6, path_bar_w, 22);

        if self.explorer_editing_path {
            // Editing mode — cyan border, show buffer text with cursor
            fb.fill_rounded_rect_aa(path_bar_rect, Pixel::rgb(20, 20, 20), 4);
            fb.draw_rounded_rect(path_bar_rect, Pixel::rgb(0, 200, 220), 4, 1);
            let max_w = path_bar_w.saturating_sub(16);
            let display = fonts::truncate_with_ellipsis(&self.explorer_path_buf, max_w, 1);
            fonts::draw_string_compact(
                fb,
                content.x + 88,
                content.y + 11,
                &display,
                Pixel::rgb(230, 230, 230),
                1,
            );
            // Blinking cursor (approximate position)
            let cursor_x =
                content.x + 88 + (self.explorer_path_cursor as i32 * 6).min(max_w as i32 - 2);
            fb.fill_rect(
                Rect::new(cursor_x, content.y + 9, 1, 14),
                Pixel::rgb(0, 200, 220),
            );
        } else {
            // Normal mode — display current path
            fb.fill_rounded_rect_aa(path_bar_rect, Pixel::rgb(25, 25, 25), 4);
            fb.draw_rounded_rect(path_bar_rect, Pixel::rgb(60, 60, 60), 4, 1);
            let max_path_w = path_bar_w.saturating_sub(16);
            let display_path = fonts::truncate_with_ellipsis(&current_path, max_path_w, 1);
            fonts::draw_string_compact(
                fb,
                content.x + 88,
                content.y + 11,
                &display_path,
                Pixel::rgb(200, 200, 200),
                1,
            );
        }

        // === Search bar (Ctrl+F) — shown between nav bar and column headers ===
        let search_bar_h: u32 = if self.explorer_search_active { 28 } else { 0 };
        if self.explorer_search_active {
            let sb_y = content.y + nav_h as i32;
            fb.fill_rect(
                Rect::new(content.x, sb_y, content.width, search_bar_h),
                Pixel::rgb(30, 30, 35),
            );
            fb.draw_hline(
                content.x,
                sb_y + search_bar_h as i32 - 1,
                content.width,
                Pixel::rgb(55, 55, 60),
            );

            // Search icon (magnifying glass)
            let icon_x = content.x + 12;
            let icon_y = sb_y + 6;
            fb.draw_circle_aa(icon_x + 6, icon_y + 6, 5, Pixel::rgb(0, 200, 220));
            fb.draw_line_aa(
                icon_x + 10,
                icon_y + 10,
                icon_x + 14,
                icon_y + 14,
                Pixel::rgb(0, 200, 220),
            );

            // Search text field
            let search_field_rect = Rect::new(
                content.x + 30,
                sb_y + 4,
                content.width.saturating_sub(44),
                20,
            );
            fb.fill_rounded_rect_aa(search_field_rect, Pixel::rgb(20, 20, 24), 4);
            fb.draw_rounded_rect(search_field_rect, Pixel::rgb(0, 200, 220), 4, 1);

            if self.explorer_search_query.is_empty() {
                fonts::draw_string_compact(
                    fb,
                    content.x + 38,
                    sb_y + 9,
                    "Search files...",
                    Pixel::rgb(100, 100, 110),
                    1,
                );
            } else {
                let max_sw = search_field_rect.width.saturating_sub(16);
                let display = fonts::truncate_with_ellipsis(&self.explorer_search_query, max_sw, 1);
                fonts::draw_string_compact(
                    fb,
                    content.x + 38,
                    sb_y + 9,
                    &display,
                    Pixel::rgb(230, 230, 230),
                    1,
                );
            }
            // Cursor
            let cur_x = content.x
                + 38
                + (self.explorer_search_cursor as i32 * 6).min(search_field_rect.width as i32 - 16);
            fb.fill_rect(Rect::new(cur_x, sb_y + 7, 1, 14), Pixel::rgb(0, 200, 220));
        }

        // === Sidebar bookmarks panel (left side) ===
        let sidebar_w: u32 = if self.explorer_sidebar_visible {
            160
        } else {
            0
        };
        let sidebar_x = content.x;
        let sidebar_y = content.y + nav_h as i32 + search_bar_h as i32;
        let sidebar_h = content.height.saturating_sub(nav_h + search_bar_h);

        if self.explorer_sidebar_visible {
            // Sidebar background
            fb.fill_rect(
                Rect::new(sidebar_x, sidebar_y, sidebar_w, sidebar_h),
                Pixel::rgb(28, 28, 32),
            );
            // Right border
            fb.draw_vline(
                sidebar_x + sidebar_w as i32 - 1,
                sidebar_y,
                sidebar_h,
                Pixel::rgb(55, 55, 60),
            );

            let bookmark_items: [(&str, &str, Pixel); 7] = [
                ("Home", "/home/user", Pixel::rgb(0, 180, 255)),
                ("Desktop", "/home/user/Desktop", Pixel::rgb(180, 140, 255)),
                (
                    "Documents",
                    "/home/user/Documents",
                    Pixel::rgb(255, 180, 60),
                ),
                (
                    "Downloads",
                    "/home/user/Downloads",
                    Pixel::rgb(100, 220, 100),
                ),
                ("Pictures", "/home/user/Pictures", Pixel::rgb(255, 120, 180)),
                ("Music", "/home/user/Music", Pixel::rgb(255, 100, 100)),
                ("Root /", "/", Pixel::rgb(160, 160, 160)),
            ];

            // Section header
            fonts::draw_string_bold_compact(
                fb,
                sidebar_x + 10,
                sidebar_y + 6,
                "Bookmarks",
                Pixel::rgb(120, 120, 130),
                1,
            );

            for (i, &(label, path, icon_color)) in bookmark_items.iter().enumerate() {
                let by = sidebar_y + 24 + (i as i32 * 26);
                let item_rect = Rect::new(sidebar_x + 4, by, sidebar_w - 8, 24);

                // Highlight if current path matches
                let is_current = current_path == path;
                if is_current {
                    fb.fill_rounded_rect_aa(item_rect, Pixel::new(0, 180, 220, 25), 4);
                }

                // Folder icon dot
                fb.fill_circle_aa(sidebar_x + 14, by + 12, 4, icon_color);

                // Label
                let label_color = if is_current {
                    Pixel::rgb(0, 200, 220)
                } else {
                    Pixel::rgb(180, 180, 185)
                };
                fonts::draw_string_compact(fb, sidebar_x + 24, by + 6, label, label_color, 1);
            }

            // Separator
            let sep_y = sidebar_y + 24 + 7 * 26 + 4;
            fb.draw_hline(sidebar_x + 8, sep_y, sidebar_w - 16, Pixel::rgb(50, 50, 55));

            // Devices section header
            fonts::draw_string_bold_compact(
                fb,
                sidebar_x + 10,
                sep_y + 6,
                "Devices",
                Pixel::rgb(120, 120, 130),
                1,
            );

            // Show root filesystem as a device
            let dev_y = sep_y + 22;
            let dev_rect = Rect::new(sidebar_x + 4, dev_y, sidebar_w - 8, 24);
            let is_root_current = current_path == "/";
            if is_root_current {
                fb.fill_rounded_rect_aa(dev_rect, Pixel::new(0, 180, 220, 25), 4);
            }
            // Drive icon
            fb.fill_rounded_rect_aa(
                Rect::new(sidebar_x + 10, dev_y + 6, 10, 8),
                Pixel::rgb(140, 140, 150),
                2,
            );
            fonts::draw_string_compact(
                fb,
                sidebar_x + 24,
                dev_y + 6,
                "Filesystem",
                Pixel::rgb(160, 160, 165),
                1,
            );
        }

        // Adjust content area for sidebar offset
        let list_content_x = content.x + sidebar_w as i32;
        let list_content_w = content.width.saturating_sub(sidebar_w);

        // === Column headers with sort indicators ===
        let header_y = content.y + nav_h as i32 + search_bar_h as i32;
        fb.fill_rect(
            Rect::new(list_content_x, header_y, list_content_w, 20),
            Pixel::rgb(35, 35, 35),
        );
        fb.draw_hline(
            list_content_x,
            header_y + 19,
            list_content_w,
            Pixel::rgb(55, 55, 55),
        );

        let name_col = list_content_x + 12;
        let date_col = list_content_x + list_content_w as i32 - 275;
        let size_col = list_content_x + list_content_w as i32 - 180;
        let type_col = list_content_x + list_content_w as i32 - 100;
        let perms_col = list_content_x + list_content_w as i32 - 365;

        // Helper: draw sort arrow indicator after label
        let draw_sort_indicator = |fb: &mut FrameBuffer, x: i32, y: i32, ascending: bool| {
            let c = Pixel::rgb(0, 180, 220);
            if ascending {
                // ▲
                fb.draw_line_aa(x, y + 6, x + 3, y + 1, c);
                fb.draw_line_aa(x + 3, y + 1, x + 6, y + 6, c);
            } else {
                // ▼
                fb.draw_line_aa(x, y + 1, x + 3, y + 6, c);
                fb.draw_line_aa(x + 3, y + 6, x + 6, y + 1, c);
            }
        };

        // Name header
        let name_active = self.explorer_sort == ExplorerSort::Name;
        let name_c = if name_active {
            Pixel::rgb(0, 200, 220)
        } else {
            Pixel::rgb(160, 160, 160)
        };
        fonts::draw_string_bold_compact(fb, name_col, header_y + 4, "Name", name_c, 1);
        if name_active {
            draw_sort_indicator(fb, name_col + 32, header_y + 4, self.explorer_sort_asc);
        }

        // Perms header
        if list_content_w > 450 {
            fonts::draw_string_bold_compact(
                fb,
                perms_col,
                header_y + 4,
                "Perms",
                Pixel::rgb(160, 160, 160),
                1,
            );
        }

        // Date header
        if list_content_w > 350 {
            let date_active = self.explorer_sort == ExplorerSort::Date;
            let date_c = if date_active {
                Pixel::rgb(0, 200, 220)
            } else {
                Pixel::rgb(160, 160, 160)
            };
            fonts::draw_string_bold_compact(fb, date_col, header_y + 4, "Modified", date_c, 1);
            if date_active {
                draw_sort_indicator(fb, date_col + 52, header_y + 4, self.explorer_sort_asc);
            }
        }

        // Size header
        if list_content_w > 250 {
            let size_active = self.explorer_sort == ExplorerSort::Size;
            let size_c = if size_active {
                Pixel::rgb(0, 200, 220)
            } else {
                Pixel::rgb(160, 160, 160)
            };
            fonts::draw_string_bold_compact(fb, size_col, header_y + 4, "Size", size_c, 1);
            if size_active {
                draw_sort_indicator(fb, size_col + 28, header_y + 4, self.explorer_sort_asc);
            }
        }

        // Type header
        let type_active = self.explorer_sort == ExplorerSort::Type;
        let type_c = if type_active {
            Pixel::rgb(0, 200, 220)
        } else {
            Pixel::rgb(160, 160, 160)
        };
        fonts::draw_string_bold_compact(fb, type_col, header_y + 4, "Type", type_c, 1);
        if type_active {
            draw_sort_indicator(fb, type_col + 28, header_y + 4, self.explorer_sort_asc);
        }

        // === File/folder entries with scroll support ===
        let entries_y = content.y + nav_h as i32 + search_bar_h as i32 + 24;
        let status_h: u32 = 23;
        let list_h = content
            .height
            .saturating_sub(nav_h + 20 + status_h + search_bar_h);
        let item_h = 28i32;

        // Preview panel width (0 when hidden, ~40% when visible)
        let preview_w: u32 = if self.explorer_preview_visible {
            (list_content_w * 2 / 5)
                .max(200)
                .min(list_content_w.saturating_sub(300))
        } else {
            0
        };
        let list_w = list_content_w.saturating_sub(preview_w);

        let folder_color = Pixel::rgb(0, 131, 213);
        let file_color = Pixel::rgb(160, 180, 200);
        let symlink_color = Pixel::rgb(0, 200, 200);
        let device_color = Pixel::rgb(200, 200, 0);
        let exec_color = Pixel::rgb(88, 255, 0);
        let selected_bg = Pixel::new(0, 180, 220, 30);

        // Generate thumbnails for image files in grid view
        if self.explorer_grid_view {
            if self.thumbnail_cache_dir != current_path {
                self.thumbnail_cache.clear();
                self.thumbnail_cache_dir = current_path.clone();
            }
            // Lazily load thumbnails for visible entries (limit per frame to avoid stalls)
            let mut loads_this_frame = 0u32;
            let cols = (list_w.saturating_sub(20) / 90u32).max(1);
            let scroll_row_off = (self.scroll_y / 80i32).max(0) as usize;
            let visible_rows = (list_h / 80u32).max(1) as usize;
            let start_idx = scroll_row_off * cols as usize;
            let end_idx = ((scroll_row_off + visible_rows + 1) * cols as usize).min(entries.len());
            for entry in entries[start_idx..end_idx].iter() {
                if entry.is_dir || loads_this_frame >= 3 {
                    continue;
                }
                let lower = entry.name.to_ascii_lowercase();
                let is_img = lower.ends_with(".png")
                    || lower.ends_with(".jpg")
                    || lower.ends_with(".jpeg")
                    || lower.ends_with(".bmp");
                if !is_img {
                    continue;
                }
                let file_path = if current_path == "/" {
                    alloc::format!("/{}", entry.name)
                } else {
                    alloc::format!("{}/{}", current_path, entry.name)
                };
                if self.thumbnail_cache.contains_key(&file_path) {
                    continue;
                }
                // Load and decode
                if let Some(data) = crate::vfs::read_file_dispatch(&file_path) {
                    if let Some(img) = super::image::decode(&data) {
                        // Scale down to thumbnail: max 48x36
                        let thumb_max_w = 48u32;
                        let thumb_max_h = 36u32;
                        let scale = {
                            let sw = thumb_max_w as f32 / img.width.max(1) as f32;
                            let sh = thumb_max_h as f32 / img.height.max(1) as f32;
                            if sw < sh { sw } else { sh }
                        };
                        let tw = ((img.width as f32 * scale) as u32).max(1).min(thumb_max_w);
                        let th = ((img.height as f32 * scale) as u32).max(1).min(thumb_max_h);
                        // Convert pixels to BGRA byte buffer
                        let mut bgra = Vec::with_capacity((img.width * img.height * 4) as usize);
                        for p in &img.pixels {
                            bgra.push(p.b);
                            bgra.push(p.g);
                            bgra.push(p.r);
                            bgra.push(p.a);
                        }
                        self.thumbnail_cache
                            .insert(file_path, Some((bgra, img.width, img.height)));
                    } else {
                        self.thumbnail_cache.insert(file_path, None);
                    }
                } else {
                    self.thumbnail_cache.insert(file_path, None);
                }
                loads_this_frame += 1;
            }
        }

        let vfs = crate::vfs::VFS.lock();

        if self.explorer_grid_view {
            // ══════ GRID VIEW ══════
            let cell_w = 90u32;
            let cell_h = 80u32;
            let cols = (list_w.saturating_sub(20) / cell_w).max(1);
            let total_rows = (entries.len() as u32).div_ceil(cols);
            let visible_rows = (list_h / cell_h).max(1);
            let scroll_row_offset = (self.scroll_y / cell_h as i32).max(0) as u32;

            self.max_scroll_y = ((total_rows * cell_h) as i32 - list_h as i32).max(0);

            for (vi, entry) in entries.iter().enumerate() {
                let row = vi as u32 / cols;
                let col = vi as u32 % cols;
                if row < scroll_row_offset {
                    continue;
                }
                if row > scroll_row_offset + visible_rows {
                    break;
                }

                let cx = list_content_x + 8 + (col * cell_w) as i32;
                let cy = entries_y + ((row - scroll_row_offset) * cell_h) as i32;

                if cy + cell_h as i32 <= entries_y || cy >= entries_y + list_h as i32 {
                    continue;
                }

                let is_selected = self.explorer_selected == vi as i32;
                if is_selected {
                    fb.fill_rounded_rect_aa(
                        Rect::new(cx, cy, cell_w - 4, cell_h - 4),
                        selected_bg,
                        6,
                    );
                }

                // Icon (centered, larger) — or thumbnail for image files
                let icon_cx = cx + cell_w as i32 / 2 - 10;
                let icon_cy = cy + 8;
                if entry.is_dir {
                    fb.fill_rounded_rect_aa(Rect::new(icon_cx, icon_cy, 10, 5), folder_color, 1);
                    fb.fill_rounded_rect_aa(
                        Rect::new(icon_cx - 2, icon_cy + 4, 24, 18),
                        folder_color,
                        3,
                    );
                    fb.fill_rounded_rect_aa(
                        Rect::new(icon_cx - 2, icon_cy + 10, 24, 12),
                        colors::lighten(folder_color, 30),
                        3,
                    );
                } else {
                    // Check for cached thumbnail
                    let file_path = if current_path == "/" {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", current_path, entry.name)
                    };
                    let mut drew_thumb = false;
                    if let Some(Some((bgra, src_w, src_h))) = self.thumbnail_cache.get(&file_path) {
                        // Scale to fit 48x36 area centered in the cell
                        let thumb_max_w = 48u32;
                        let thumb_max_h = 36u32;
                        let scale_w = thumb_max_w as f32 / (*src_w).max(1) as f32;
                        let scale_h = thumb_max_h as f32 / (*src_h).max(1) as f32;
                        let scale = if scale_w < scale_h { scale_w } else { scale_h };
                        let tw = ((*src_w as f32 * scale) as u32).max(1).min(thumb_max_w);
                        let th = ((*src_h as f32 * scale) as u32).max(1).min(thumb_max_h);
                        let tx = cx + (cell_w as i32 - tw as i32) / 2;
                        let ty = cy + 4;
                        // Draw a subtle border
                        fb.fill_rounded_rect_aa(
                            Rect::new(tx - 1, ty - 1, tw + 2, th + 2),
                            Pixel::rgb(60, 60, 64),
                            2,
                        );
                        fb.blit_bgra_scaled(tx, ty, tw, th, bgra, *src_w, *src_h);
                        drew_thumb = true;
                    }
                    if !drew_thumb {
                        fb.fill_rounded_rect_aa(
                            Rect::new(icon_cx, icon_cy, 20, 24),
                            Pixel::rgb(48, 52, 58),
                            3,
                        );
                        fb.fill_rect(
                            Rect::new(icon_cx + 3, icon_cy + 6, 14, 1),
                            Pixel::rgb(80, 90, 100),
                        );
                        fb.fill_rect(
                            Rect::new(icon_cx + 3, icon_cy + 10, 14, 1),
                            Pixel::rgb(70, 80, 90),
                        );
                        fb.fill_rect(
                            Rect::new(icon_cx + 3, icon_cy + 14, 10, 1),
                            Pixel::rgb(70, 80, 90),
                        );
                    }
                }

                // Name (centered, truncated)
                let max_name_chars = (cell_w / 6) as usize;
                let name_display = if entry.name.len() > max_name_chars {
                    alloc::format!("{}…", &entry.name[..max_name_chars.saturating_sub(1)])
                } else {
                    entry.name.clone()
                };
                let name_px = name_display.len() as i32 * 6;
                let name_x = cx + (cell_w as i32 - name_px) / 2;
                let name_color = if entry.is_dir {
                    Pixel::rgb(80, 180, 255)
                } else if entry.name.starts_with('.') {
                    Pixel::rgb(120, 120, 120)
                } else {
                    colors::WHITE
                };
                fonts::draw_string_compact(
                    fb,
                    name_x,
                    cy + cell_h as i32 - 22,
                    &name_display,
                    name_color,
                    1,
                );
            }
        } else {
            // ══════ LIST VIEW (original) ══════
            let visible_items = (list_h as i32 / item_h) as usize;
            let scroll_item_offset = (self.scroll_y / item_h.max(1)) as usize;

            for (vi, entry) in entries
                .iter()
                .enumerate()
                .skip(scroll_item_offset)
                .take(visible_items + 1)
            {
                let ey = entries_y + (vi as i32 - scroll_item_offset as i32) * item_h;
                if ey + item_h <= entries_y {
                    continue;
                }
                if ey >= entries_y + list_h as i32 {
                    break;
                }

                // Selected item highlight
                let is_selected = self.explorer_selected == vi as i32;
                if is_selected {
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 4, ey, list_w.saturating_sub(18), item_h as u32),
                        selected_bg,
                        4,
                    );
                } else if vi % 2 == 1 {
                    // Alternating row background
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 4, ey, list_w.saturating_sub(18), item_h as u32),
                        Pixel::new(255, 255, 255, 6),
                        4,
                    );
                }

                // Draw icons (same as before)
                if entry.is_dir {
                    let ix = content.x + 10;
                    let iy = ey + 4;
                    fb.fill_rounded_rect_aa(Rect::new(ix, iy, 7, 4), folder_color, 1);
                    fb.fill_rounded_rect_aa(Rect::new(ix, iy + 3, 18, 13), folder_color, 2);
                    fb.fill_rounded_rect_aa(
                        Rect::new(ix, iy + 7, 18, 9),
                        colors::lighten(folder_color, 30),
                        2,
                    );
                } else if entry.kind == "Symlink" {
                    let sym_c = symlink_color;
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 12, ey + 3, 14, 16),
                        Pixel::rgb(35, 38, 42),
                        2,
                    );
                    fb.draw_line_aa(content.x + 12, ey + 5, content.x + 12, ey + 18, sym_c);
                    fb.draw_line_aa(content.x + 12, ey + 18, content.x + 25, ey + 18, sym_c);
                    fb.draw_line_aa(content.x + 25, ey + 5, content.x + 25, ey + 18, sym_c);
                    fb.draw_line_aa(content.x + 12, ey + 5, content.x + 21, ey + 5, sym_c);
                    fb.draw_line_aa(content.x + 16, ey + 14, content.x + 22, ey + 10, sym_c);
                    fb.draw_line_aa(content.x + 22, ey + 10, content.x + 19, ey + 10, sym_c);
                    fb.draw_line_aa(content.x + 22, ey + 10, content.x + 22, ey + 13, sym_c);
                } else if entry.kind == "Char Device" || entry.kind == "Block Device" {
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 11, ey + 3, 16, 16),
                        Pixel::rgb(40, 40, 30),
                        3,
                    );
                    fb.fill_circle_aa(content.x + 19, ey + 11, 5, Pixel::rgb(50, 50, 35));
                    fb.fill_circle_aa(content.x + 19, ey + 11, 3, device_color);
                    fb.fill_circle_aa(content.x + 19, ey + 11, 1, Pixel::rgb(40, 40, 30));
                } else {
                    let ix = content.x + 12;
                    let iy = ey + 3;
                    fb.fill_rounded_rect_aa(Rect::new(ix, iy, 14, 17), Pixel::rgb(48, 52, 58), 2);
                    fb.fill_rect(Rect::new(ix + 2, iy + 4, 10, 11), Pixel::rgb(42, 46, 52));
                    let fold_x = ix + 10;
                    let fold_y = iy;
                    fb.fill_rect(Rect::new(fold_x, fold_y, 4, 4), Pixel::rgb(60, 64, 70));
                    fb.draw_line_aa(fold_x, fold_y, fold_x, fold_y + 4, file_color);
                    fb.draw_line_aa(fold_x, fold_y + 4, fold_x + 4, fold_y, file_color);
                    fb.fill_rect(Rect::new(ix + 3, iy + 6, 7, 1), Pixel::rgb(80, 90, 100));
                    fb.fill_rect(Rect::new(ix + 3, iy + 9, 8, 1), Pixel::rgb(70, 80, 90));
                    fb.fill_rect(Rect::new(ix + 3, iy + 12, 5, 1), Pixel::rgb(70, 80, 90));
                }

                // Name text — check if in rename mode
                let is_renaming = self.explorer_renaming == Some(vi);

                if is_renaming {
                    // Rename mode: draw editable text field
                    let rename_x = content.x + 36;
                    let rename_w = 200u32.min(content.width.saturating_sub(50));
                    fb.fill_rounded_rect_aa(
                        Rect::new(rename_x - 2, ey + 4, rename_w, 18),
                        Pixel::rgb(20, 20, 20),
                        3,
                    );
                    fb.draw_rounded_rect(
                        Rect::new(rename_x - 2, ey + 4, rename_w, 18),
                        Pixel::rgb(0, 200, 220),
                        3,
                        1,
                    );
                    fonts::draw_string_compact(
                        fb,
                        rename_x + 2,
                        ey + 8,
                        &self.explorer_rename_buf,
                        Pixel::rgb(240, 240, 240),
                        1,
                    );
                    // Cursor
                    let cur_x = rename_x + 2 + self.explorer_rename_buf.len() as i32 * 6;
                    fb.fill_rect(Rect::new(cur_x, ey + 6, 1, 14), Pixel::rgb(0, 200, 220));
                } else {
                    // Normal name rendering with color by type
                    let name_color = if entry.is_dir {
                        Pixel::rgb(80, 180, 255)
                    } else if entry.kind == "Symlink" {
                        Pixel::rgb(0, 220, 220)
                    } else if entry.kind == "Char Device" || entry.kind == "Block Device" {
                        Pixel::rgb(220, 220, 0)
                    } else if entry.kind == "FIFO" || entry.kind == "Socket" {
                        Pixel::rgb(220, 0, 220)
                    } else if entry.name.starts_with('.') {
                        Pixel::rgb(120, 120, 120)
                    } else {
                        // Check if executable
                        if entry.permissions & 0o111 != 0 {
                            exec_color
                        } else {
                            colors::WHITE
                        }
                    };

                    let max_name_w = if content.width > 450 {
                        (perms_col - name_col - 36) as u32
                    } else if content.width > 350 {
                        (date_col - name_col - 36) as u32
                    } else if content.width > 250 {
                        (size_col - name_col - 36) as u32
                    } else {
                        (type_col - name_col - 36) as u32
                    };
                    let display_name = fonts::truncate_with_ellipsis(&entry.name, max_name_w, 1);
                    fonts::draw_string_compact(
                        fb,
                        content.x + 36,
                        ey + 8,
                        &display_name,
                        name_color,
                        1,
                    );
                }

                // Permissions column
                if content.width > 450 {
                    let perms_str = Self::format_permissions(entry.permissions, entry.is_dir);
                    fonts::draw_string_compact(
                        fb,
                        perms_col,
                        ey + 8,
                        &perms_str,
                        Pixel::rgb(100, 100, 100),
                        1,
                    );
                }

                // Date modified column
                if content.width > 350 && !entry.date_display.is_empty() {
                    fonts::draw_string_compact(
                        fb,
                        date_col,
                        ey + 8,
                        &entry.date_display,
                        Pixel::rgb(110, 115, 120),
                        1,
                    );
                }

                // Size
                if content.width > 250 && !entry.size.is_empty() {
                    fonts::draw_string_compact(
                        fb,
                        size_col,
                        ey + 8,
                        &entry.size,
                        Pixel::rgb(120, 120, 120),
                        1,
                    );
                }

                // Type
                fonts::draw_string_compact(
                    fb,
                    type_col,
                    ey + 8,
                    &entry.kind,
                    Pixel::rgb(120, 120, 120),
                    1,
                );
            }
        } // end list/grid view branch

        drop(vfs);

        // === Scrollbar ===
        let scroll_track_x = content.x + list_w as i32 - 10;
        let scroll_track_y = entries_y;
        let scroll_track_h = list_h;
        fb.fill_rounded_rect_aa(
            Rect::new(scroll_track_x, scroll_track_y, 8, scroll_track_h),
            colors::SCROLLBAR_TRACK,
            4,
        );
        let total_items_h = entries.len() as u32 * item_h as u32;
        self.max_scroll_y = (total_items_h as i32 - scroll_track_h as i32).max(0);
        if total_items_h > scroll_track_h {
            let visible_ratio = scroll_track_h as f32 / total_items_h as f32;
            let thumb_h = ((visible_ratio * scroll_track_h as f32) as u32).max(20);
            let scroll_ratio = self.scroll_y as f32 / (total_items_h - scroll_track_h) as f32;
            let thumb_y =
                scroll_track_y + (scroll_ratio.min(1.0) * (scroll_track_h - thumb_h) as f32) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(scroll_track_x + 1, thumb_y, 6, thumb_h),
                colors::SCROLLBAR_THUMB,
                3,
            );
        } else {
            let thumb_h = scroll_track_h.clamp(10, 20);
            fb.fill_rounded_rect_aa(
                Rect::new(scroll_track_x + 1, scroll_track_y + 2, 6, thumb_h),
                colors::SCROLLBAR_THUMB,
                3,
            );
        }

        // === Preview panel (right side, when visible) ===
        if self.explorer_preview_visible && preview_w > 0 {
            let pv_x = content.x + list_w as i32;
            let pv_y = content.y + nav_h as i32 + search_bar_h as i32;
            let pv_h = content
                .height
                .saturating_sub(nav_h + status_h + search_bar_h);

            // Separator line
            fb.draw_vline(pv_x, pv_y, pv_h, Pixel::rgb(60, 60, 60));

            // Preview background
            fb.fill_rect(
                Rect::new(pv_x + 1, pv_y, preview_w.saturating_sub(1), pv_h),
                Pixel::rgb(22, 22, 26),
            );

            // Preview header bar (24px)
            let pv_header_h: u32 = 24;
            fb.fill_rect(
                Rect::new(pv_x + 1, pv_y, preview_w.saturating_sub(1), pv_header_h),
                Pixel::rgb(35, 35, 40),
            );
            fb.draw_hline(
                pv_x + 1,
                pv_y + pv_header_h as i32,
                preview_w.saturating_sub(1),
                Pixel::rgb(55, 55, 60),
            );

            // Title: filename or "No Preview"
            if !self.explorer_preview_path.is_empty() {
                // Extract filename from path
                let fname = self
                    .explorer_preview_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&self.explorer_preview_path);
                let max_title_w = preview_w.saturating_sub(20);
                let title = fonts::truncate_with_ellipsis(fname, max_title_w, 1);
                fonts::draw_string_bold_compact(
                    fb,
                    pv_x + 8,
                    pv_y + 6,
                    &title,
                    Pixel::rgb(200, 210, 220),
                    1,
                );
            } else {
                fonts::draw_string_compact(
                    fb,
                    pv_x + 8,
                    pv_y + 6,
                    "No file selected",
                    Pixel::rgb(100, 100, 110),
                    1,
                );
            }

            // Preview content area
            let pv_content_y = pv_y + pv_header_h as i32 + 4;
            let pv_content_h = pv_h.saturating_sub(pv_header_h + 8);

            if self.explorer_preview_content.is_empty() {
                // Empty state
                if !self.explorer_preview_path.is_empty() {
                    fonts::draw_string_compact(
                        fb,
                        pv_x + 12,
                        pv_content_y + 8,
                        "(empty file)",
                        Pixel::rgb(80, 80, 90),
                        1,
                    );
                } else {
                    fonts::draw_string_compact(
                        fb,
                        pv_x + 12,
                        pv_content_y + 8,
                        "Press Ctrl+P to toggle",
                        Pixel::rgb(80, 80, 90),
                        1,
                    );
                    fonts::draw_string_compact(
                        fb,
                        pv_x + 12,
                        pv_content_y + 24,
                        "Select a file to preview",
                        Pixel::rgb(80, 80, 90),
                        1,
                    );
                }
            } else {
                // Render text content line by line
                let max_chars = ((preview_w.saturating_sub(24)) / 6) as usize; // 6px per char compact
                let line_h = 14i32; // compact line height
                let max_lines = (pv_content_h as i32 / line_h) as usize;
                let mut line_idx = 0usize;

                // Line number gutter width
                let gutter_w = 30i32;
                let text_x = pv_x + 8 + gutter_w;
                let gutter_x = pv_x + 4;

                for line in self.explorer_preview_content.lines() {
                    if line_idx >= max_lines {
                        break;
                    }
                    let ly = pv_content_y + (line_idx as i32) * line_h;

                    // Line number
                    let ln_str = alloc::format!("{:>3}", line_idx + 1);
                    fonts::draw_string_compact(
                        fb,
                        gutter_x,
                        ly,
                        &ln_str,
                        Pixel::rgb(70, 75, 85),
                        1,
                    );

                    // Truncate long lines
                    let max_text_chars = max_chars.saturating_sub(5);
                    let display_line = if line.len() > max_text_chars {
                        &line[..max_text_chars]
                    } else {
                        line
                    };
                    fonts::draw_string_compact(
                        fb,
                        text_x,
                        ly,
                        display_line,
                        Pixel::rgb(190, 195, 205),
                        1,
                    );

                    line_idx += 1;
                }

                // Show truncation indicator if file content was truncated
                if self.explorer_preview_content.len() >= 2040 {
                    let trunc_y =
                        pv_content_y + (line_idx.min(max_lines.saturating_sub(1)) as i32) * line_h;
                    if line_idx < max_lines {
                        fonts::draw_string_compact(
                            fb,
                            pv_x + 12,
                            trunc_y,
                            "--- truncated ---",
                            Pixel::rgb(100, 100, 60),
                            1,
                        );
                    }
                }
            }
        }

        // === Status bar (23px) ===
        let status_h_i = status_h as i32;
        let status_y = content.y + content.height as i32 - status_h_i;
        fb.fill_rect(
            Rect::new(content.x, status_y, content.width, status_h),
            Pixel::rgb(40, 40, 40),
        );
        fb.draw_hline(content.x, status_y, content.width, Pixel::rgb(60, 60, 60));

        // Item count + selection info
        let count_text = if self.explorer_search_active && !self.explorer_search_query.is_empty() {
            if self.explorer_selected >= 0 {
                alloc::format!(
                    "{} matches • selected: {}",
                    entries.len(),
                    self.explorer_selected + 1
                )
            } else {
                alloc::format!(
                    "{} matches for \"{}\"",
                    entries.len(),
                    self.explorer_search_query
                )
            }
        } else if self.explorer_selected >= 0 {
            alloc::format!(
                "{} items • selected: {}",
                entries.len(),
                self.explorer_selected + 1
            )
        } else {
            alloc::format!("{} items", entries.len())
        };
        fonts::draw_string_compact(
            fb,
            content.x + 8,
            status_y + 5,
            &count_text,
            Pixel::rgb(150, 150, 150),
            1,
        );

        // Hidden files indicator
        if self.explorer_show_hidden {
            fonts::draw_string_compact(
                fb,
                content.x + 8 + count_text.len() as i32 * 6 + 12,
                status_y + 5,
                "[hidden]",
                Pixel::rgb(100, 180, 200),
                1,
            );
        }

        // Preview indicator
        if self.explorer_preview_visible {
            let preview_x = content.x
                + 8
                + count_text.len() as i32 * 6
                + 12
                + if self.explorer_show_hidden {
                    8 * 6 + 8
                } else {
                    0
                };
            fonts::draw_string_compact(
                fb,
                preview_x,
                status_y + 5,
                "[preview]",
                Pixel::rgb(180, 140, 255),
                1,
            );
        }

        // Grid/list view indicator
        if self.explorer_grid_view {
            let grid_x = content.x
                + 8
                + count_text.len() as i32 * 6
                + 12
                + if self.explorer_show_hidden {
                    8 * 6 + 8
                } else {
                    0
                }
                + if self.explorer_preview_visible {
                    9 * 6 + 8
                } else {
                    0
                };
            fonts::draw_string_compact(
                fb,
                grid_x,
                status_y + 5,
                "[grid]",
                Pixel::rgb(140, 200, 140),
                1,
            );
        }

        // Path in status bar right side
        let path_short = if current_path.len() > 40 {
            alloc::format!("...{}", &current_path[current_path.len() - 37..])
        } else {
            current_path.clone()
        };
        let path_px_w = path_short.len() as u32 * 8;
        if content.width > path_px_w + 100 {
            fonts::draw_string_compact(
                fb,
                content.x + content.width as i32 - path_px_w as i32 - 16,
                status_y + 5,
                &path_short,
                Pixel::rgb(100, 100, 100),
                1,
            );
        }

        // === Context menu overlay ===
        if let Some(ref ctx) = self.explorer_ctx_menu {
            let menu_w = 180u32;
            let item_h_menu = 28i32;
            let menu_h = ctx.items.len() as u32 * item_h_menu as u32 + 8;

            // Menu background with shadow
            fb.fill_rounded_rect_aa(
                Rect::new(ctx.x + 2, ctx.y + 2, menu_w, menu_h),
                Pixel::new(0, 0, 0, 60),
                8,
            );
            fb.fill_rounded_rect_aa(
                Rect::new(ctx.x, ctx.y, menu_w, menu_h),
                Pixel::rgb(35, 38, 42),
                8,
            );
            fb.draw_rounded_rect(
                Rect::new(ctx.x, ctx.y, menu_w, menu_h),
                Pixel::rgb(60, 65, 75),
                8,
                1,
            );

            for (i, item) in ctx.items.iter().enumerate() {
                let iy = ctx.y + 4 + i as i32 * item_h_menu;
                let text_color = if item.enabled {
                    Pixel::rgb(220, 220, 220)
                } else {
                    Pixel::rgb(80, 80, 80)
                };
                fonts::draw_string_compact(fb, ctx.x + 12, iy + 7, &item.label, text_color, 1);

                // Show keyboard shortcut hints
                let shortcut = match item.action {
                    ExplorerAction::Copy => Some("Ctrl+C"),
                    ExplorerAction::Cut => Some("Ctrl+X"),
                    ExplorerAction::Paste => Some("Ctrl+V"),
                    ExplorerAction::Delete => Some("Del"),
                    ExplorerAction::Rename => Some("F2"),
                    ExplorerAction::ToggleHidden => Some("Ctrl+H"),
                    _ => None,
                };
                if let Some(sc) = shortcut {
                    let sc_w = sc.len() as i32 * 6;
                    fonts::draw_string_compact(
                        fb,
                        ctx.x + menu_w as i32 - sc_w - 12,
                        iy + 7,
                        sc,
                        Pixel::rgb(90, 95, 100),
                        1,
                    );
                }
            }
        }
    }

    /// Format a file size in human-readable form (B, KB, MB, GB)
    fn format_size(bytes: u64) -> alloc::string::String {
        if bytes == 0 {
            alloc::string::String::from("0 B")
        } else if bytes < 1024 {
            alloc::format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            let kb = bytes as f64 / 1024.0;
            if kb < 10.0 {
                alloc::format!("{:.1} KB", kb)
            } else {
                alloc::format!("{} KB", bytes / 1024)
            }
        } else if bytes < 1024 * 1024 * 1024 {
            let mb = bytes as f64 / (1024.0 * 1024.0);
            if mb < 10.0 {
                alloc::format!("{:.1} MB", mb)
            } else {
                alloc::format!("{} MB", bytes / (1024 * 1024))
            }
        } else {
            let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            alloc::format!("{:.1} GB", gb)
        }
    }

    /// Format Unix permissions as rwxrwxrwx string
    fn format_permissions(mode: u16, is_dir: bool) -> alloc::string::String {
        let mut s = alloc::string::String::with_capacity(10);
        s.push(if is_dir { 'd' } else { '-' });
        s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o100 != 0 { 'x' } else { '-' });
        s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o010 != 0 { 'x' } else { '-' });
        s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o001 != 0 { 'x' } else { '-' });
        s
    }

    /// Detect file type from filename extension
    fn file_type_from_name(name: &str) -> alloc::string::String {
        if let Some(dot_pos) = name.rfind('.') {
            let ext = &name[dot_pos + 1..];
            match ext {
                "txt" | "text" | "log" => alloc::string::String::from("Text"),
                "md" | "markdown" => alloc::string::String::from("Markdown"),
                "rs" => alloc::string::String::from("Rust"),
                "c" | "h" => alloc::string::String::from("C Source"),
                "cpp" | "cc" | "cxx" | "hpp" => alloc::string::String::from("C++"),
                "py" => alloc::string::String::from("Python"),
                "js" => alloc::string::String::from("JavaScript"),
                "ts" => alloc::string::String::from("TypeScript"),
                "html" | "htm" => alloc::string::String::from("HTML"),
                "css" => alloc::string::String::from("CSS"),
                "json" => alloc::string::String::from("JSON"),
                "xml" => alloc::string::String::from("XML"),
                "yaml" | "yml" => alloc::string::String::from("YAML"),
                "toml" => alloc::string::String::from("TOML"),
                "ini" | "cfg" | "conf" => alloc::string::String::from("Config"),
                "sh" | "bash" | "zsh" => alloc::string::String::from("Script"),
                "png" => alloc::string::String::from("PNG Image"),
                "jpg" | "jpeg" => alloc::string::String::from("JPEG Image"),
                "gif" => alloc::string::String::from("GIF Image"),
                "svg" => alloc::string::String::from("SVG Image"),
                "bmp" => alloc::string::String::from("Bitmap"),
                "pdf" => alloc::string::String::from("PDF"),
                "zip" | "gz" | "bz2" | "xz" | "tar" | "tgz" => {
                    alloc::string::String::from("Archive")
                }
                "o" | "a" | "so" | "dylib" => alloc::string::String::from("Binary"),
                "elf" | "bin" => alloc::string::String::from("Executable"),
                "deb" | "rpm" => alloc::string::String::from("Package"),
                _ => alloc::format!(".{} File", ext),
            }
        } else {
            alloc::string::String::from("File")
        }
    }

    fn draw_browser_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        // Get browser state for this window
        let browsers = super::browser::BROWSERS.lock();
        let state = browsers.get(&self.id);

        // Detect if Vivaldi is installed/running
        let vivaldi_active = state.map(|s| s.vivaldi).unwrap_or_else(|| {
            let st = crate::vivaldi::status();
            st == crate::vivaldi::VivaldiState::Running
                || st == crate::vivaldi::VivaldiState::Installed
                || self.title.contains("Vivaldi")
        });

        // Colors based on Vivaldi mode
        let accent = if vivaldi_active {
            Pixel::rgb(239, 46, 67)
        } else {
            Pixel::rgb(50, 50, 50)
        };

        // ─── Tab bar (28px) ──────────────────────────────────────────
        let tab_h: u32 = 28;
        let tab_bar_bg = if vivaldi_active {
            Pixel::rgb(51, 17, 19)
        } else {
            Pixel::rgb(38, 38, 38)
        };
        fb.fill_rect(
            Rect::new(content.x, content.y, content.width, tab_h),
            tab_bar_bg,
        );
        let tab_bg = if vivaldi_active {
            Pixel::rgb(30, 30, 32)
        } else {
            Pixel::rgb(50, 50, 50)
        };
        fb.fill_rounded_rect_aa(
            Rect::new(content.x + 4, content.y + 4, 160, tab_h - 4),
            tab_bg,
            4,
        );
        if vivaldi_active {
            fb.fill_rounded_rect_aa(Rect::new(content.x + 4, content.y + 4, 160, 2), accent, 1);
        }
        // Tab title from browser state
        let tab_title = state
            .map(|s| s.page.title.as_str())
            .unwrap_or(if vivaldi_active {
                "Speed Dial - Vivaldi"
            } else {
                "KnoxOS - Home"
            });
        // Truncate tab title to fit
        let max_tab_chars = 18;
        let display_title: &str = if tab_title.len() > max_tab_chars {
            &tab_title[..max_tab_chars]
        } else {
            tab_title
        };
        fonts::draw_string_bold_compact(
            fb,
            content.x + 12,
            content.y + 8,
            display_title,
            colors::WHITE,
            1,
        );
        // Tab close X
        let tab_close_x = content.x + 150;
        let tab_close_y = content.y + 10;
        fonts::draw_string_bold_compact(
            fb,
            tab_close_x,
            tab_close_y,
            "x",
            Pixel::rgb(140, 140, 140),
            1,
        );
        // + new tab
        fonts::draw_string_bold_compact(
            fb,
            content.x + 172,
            content.y + 8,
            "+",
            Pixel::rgb(140, 140, 140),
            1,
        );

        // ─── URL bar (35px) ──────────────────────────────────────────
        let url_h: u32 = 35;
        let url_y = content.y + tab_h as i32;
        let url_bar_bg = if vivaldi_active {
            Pixel::rgb(30, 30, 32)
        } else {
            Pixel::rgb(50, 50, 50)
        };
        fb.fill_rect(
            Rect::new(content.x, url_y, content.width, url_h),
            url_bar_bg,
        );

        // Navigation buttons: < > O
        let can_back = state.map(|s| !s.back_stack.is_empty()).unwrap_or(false);
        let can_fwd = state.map(|s| !s.forward_stack.is_empty()).unwrap_or(false);
        let back_color = if can_back {
            Pixel::rgb(200, 200, 200)
        } else {
            Pixel::rgb(80, 80, 80)
        };
        let fwd_color = if can_fwd {
            Pixel::rgb(200, 200, 200)
        } else {
            Pixel::rgb(80, 80, 80)
        };
        fonts::draw_string_bold_compact(fb, content.x + 8, url_y + 11, "<", back_color, 1);
        fonts::draw_string_bold_compact(fb, content.x + 22, url_y + 11, ">", fwd_color, 1);
        fonts::draw_string_bold_compact(
            fb,
            content.x + 40,
            url_y + 11,
            "O",
            Pixel::rgb(160, 160, 160),
            1,
        );

        // URL input field
        let url_field_x = content.x + 60;
        let url_field_y = url_y + 6;
        let url_field_w = content.width.saturating_sub(70);
        let url_focused = state
            .map(|s| s.focus == super::browser::BrowserFocus::UrlBar)
            .unwrap_or(false);
        let url_field_bg = if url_focused {
            Pixel::rgb(45, 45, 50)
        } else {
            Pixel::rgb(35, 35, 35)
        };
        let url_border = if url_focused {
            accent
        } else {
            Pixel::rgb(70, 70, 70)
        };
        fb.fill_rounded_rect_aa(
            Rect::new(url_field_x, url_field_y, url_field_w, 22),
            url_field_bg,
            4,
        );
        fb.draw_rounded_rect(
            Rect::new(url_field_x, url_field_y, url_field_w, 22),
            url_border,
            4,
            1,
        );
        // Lock icon
        let is_https = state
            .map(|s| s.url_text.starts_with("https://"))
            .unwrap_or(false);
        let lock_color = if is_https {
            Pixel::rgb(80, 180, 80)
        } else {
            Pixel::rgb(100, 100, 100)
        };
        fb.fill_rounded_rect_aa(Rect::new(url_field_x + 5, url_y + 11, 6, 6), lock_color, 1);
        fb.draw_rounded_rect(
            Rect::new(url_field_x + 6, url_y + 8, 4, 4),
            lock_color,
            1,
            1,
        );

        // URL text
        let url_text = state
            .map(|s| s.url_text.as_str())
            .unwrap_or(if vivaldi_active {
                "vivaldi://newtab"
            } else {
                "https://knoxos.local"
            });
        let url_text_x = url_field_x + 16;
        // Calculate visible portion of URL text
        let max_url_chars = ((url_field_w as i32 - 24) / 7).max(1) as usize;
        let url_cursor = state.map_or(0, |s| s.url_cursor);
        // Scroll URL text so cursor is visible
        let url_scroll = url_cursor.saturating_sub(max_url_chars);
        let visible_url = if url_scroll < url_text.len() {
            let end = (url_scroll + max_url_chars).min(url_text.len());
            &url_text[url_scroll..end]
        } else {
            ""
        };
        let url_text_color = if url_focused {
            Pixel::rgb(220, 220, 225)
        } else {
            Pixel::rgb(180, 180, 180)
        };
        fonts::draw_string_compact(fb, url_text_x, url_y + 11, visible_url, url_text_color, 1);
        // Cursor in URL bar
        if url_focused {
            let cursor_x = url_text_x + ((url_cursor - url_scroll) as i32 * 8);
            // Blinking cursor (simple: always show when focused)
            fb.fill_rect(Rect::new(cursor_x, url_y + 9, 1, 14), colors::WHITE);
        }

        // ─── Page content area ───────────────────────────────────────
        let total_chrome = tab_h + url_h;
        let clip_y = content.y + total_chrome as i32;
        let clip_h = content.height.saturating_sub(total_chrome);
        let page_y_base = clip_y - self.scroll_y;

        // Page background
        let page_bg = if vivaldi_active {
            Pixel::rgb(24, 24, 26)
        } else {
            Pixel::rgb(255, 255, 255)
        };
        fb.fill_rect(Rect::new(content.x, clip_y, content.width, clip_h), page_bg);

        // Determine what to render
        let is_speed_dial = state
            .map(|s| s.page.url == "vivaldi://newtab" || s.page.url == "vivaldi://speeddial")
            .unwrap_or(vivaldi_active);

        let is_knoxos_home = state
            .map(|s| s.page.url == "knoxos://home")
            .unwrap_or(!vivaldi_active);

        if is_speed_dial {
            // ═══ Speed Dial rendering ═══
            let page_width = content.width.saturating_sub(40);
            let page_y = page_y_base;

            // Search bar
            let search_w = page_width.min(500);
            let search_x = content.x + (content.width as i32 - search_w as i32) / 2;
            let search_y = page_y + 60;
            let search_focused = state
                .map(|s| s.focus == super::browser::BrowserFocus::SearchBar)
                .unwrap_or(false);

            if search_y >= clip_y && search_y < clip_y + clip_h as i32 - 40 {
                let sb_bg = if search_focused {
                    Pixel::rgb(50, 50, 55)
                } else {
                    Pixel::rgb(44, 44, 48)
                };
                let sb_border = if search_focused {
                    accent
                } else {
                    Pixel::rgb(70, 70, 74)
                };
                fb.fill_rounded_rect_aa(Rect::new(search_x, search_y, search_w, 36), sb_bg, 8);
                fb.draw_rounded_rect(Rect::new(search_x, search_y, search_w, 36), sb_border, 8, 1);

                let search_text = state.map(|s| s.search_text.as_str()).unwrap_or("");
                if search_text.is_empty() && !search_focused {
                    fonts::draw_string_compact(
                        fb,
                        search_x + 16,
                        search_y + 12,
                        "Search with Google or enter address",
                        Pixel::rgb(120, 120, 125),
                        1,
                    );
                } else {
                    // Draw actual search text
                    let max_search_chars = ((search_w as i32 - 32) / 7).max(1) as usize;
                    let search_cursor = state.map(|s| s.search_cursor).unwrap_or(0);
                    let vis_end = max_search_chars.min(search_text.len());
                    let vis_text = &search_text[..vis_end];
                    fonts::draw_string_compact(
                        fb,
                        search_x + 16,
                        search_y + 12,
                        vis_text,
                        Pixel::rgb(220, 220, 225),
                        1,
                    );
                    // Cursor
                    if search_focused {
                        let cx = search_x + 16 + (search_cursor as i32 * 8);
                        fb.fill_rect(Rect::new(cx, search_y + 10, 1, 16), colors::WHITE);
                    }
                }
            }

            // Speed dial grid
            let tile_w: u32 = 140;
            let tile_h: u32 = 100;
            let tile_gap: i32 = 20;
            let grid_cols = 3i32;
            let grid_w = grid_cols * (tile_w as i32 + tile_gap) - tile_gap;
            let grid_x = content.x + (content.width as i32 - grid_w) / 2;
            let grid_y = page_y + 130;

            let speed_dial_colors = [
                Pixel::rgb(0, 160, 255), // KnoxOS
                Pixel::rgb(36, 41, 47),  // GitHub
                Pixel::rgb(239, 46, 67), // Vivaldi
                Pixel::rgb(60, 60, 60),  // Wikipedia
                Pixel::rgb(255, 69, 0),  // Reddit
                Pixel::rgb(255, 0, 0),   // YouTube
            ];

            // Check if mouse is hovering over a tile
            let hovered = state.map(|s| s.hovered_link).unwrap_or(None);

            for (i, ((name, _url), color)) in super::browser::SPEED_DIAL_SITES
                .iter()
                .zip(speed_dial_colors.iter())
                .enumerate()
            {
                let col = (i % grid_cols as usize) as i32;
                let row = (i / grid_cols as usize) as i32;
                let tx = grid_x + col * (tile_w as i32 + tile_gap);
                let ty = grid_y + row * (tile_h as i32 + tile_gap + 20);

                if ty >= clip_y && ty < clip_y + clip_h as i32 - tile_h as i32 {
                    let tile_bg = if Some(i) == hovered {
                        Pixel::rgb(55, 55, 60)
                    } else {
                        Pixel::rgb(40, 40, 44)
                    };
                    fb.fill_rounded_rect_aa(Rect::new(tx, ty, tile_w, tile_h), tile_bg, 8);
                    let cx = tx + tile_w as i32 / 2;
                    let cy = ty + 38;
                    fb.fill_circle_aa(cx, cy, 18, *color);
                    let first_char = &name[..1];
                    fonts::draw_string_bold_compact(
                        fb,
                        cx - 4,
                        cy - 5,
                        first_char,
                        colors::WHITE,
                        1,
                    );
                    let label_x = tx + (tile_w as i32 - name.len() as i32 * 8) / 2;
                    fonts::draw_string_compact(
                        fb,
                        label_x,
                        ty + tile_h as i32 + 6,
                        name,
                        Pixel::rgb(180, 180, 185),
                        1,
                    );
                }
            }

            // Branding
            let brand_y = grid_y + 2 * (tile_h as i32 + tile_gap + 20) + 40;
            if brand_y >= clip_y && brand_y < clip_y + clip_h as i32 {
                let brand_text = "Vivaldi 7.1.3570.39 on KnoxOS";
                let bx = content.x + (content.width as i32 - brand_text.len() as i32 * 8) / 2;
                fonts::draw_string_compact(fb, bx, brand_y, brand_text, Pixel::rgb(90, 90, 95), 1);
            }
        } else if let Some(browser_state) = state {
            // ═══ Loaded page rendering (from BrowserState lines) ═══
            let page_y = page_y_base;
            let page_width = content.width.saturating_sub(40);
            let margin_x = content.x + 20;
            let line_height: i32 = 20;
            let mut y_pos = page_y + 20;

            // Loading indicator
            if browser_state.page.loading {
                let loading_bar_w = content.width / 3;
                fb.fill_rounded_rect_aa(Rect::new(content.x, clip_y, loading_bar_w, 3), accent, 1);
            }

            for (i, line) in browser_state.page.lines.iter().enumerate() {
                if y_pos >= clip_y + clip_h as i32 {
                    break; // Below visible area
                }

                let extra_h = match line.style {
                    super::browser::LineStyle::Heading1 => 12,
                    super::browser::LineStyle::Heading2 => 6,
                    _ => 0,
                };

                if y_pos + line_height + extra_h >= clip_y {
                    // This line is visible
                    let text_color = if vivaldi_active {
                        match line.style {
                            super::browser::LineStyle::Heading1 => Pixel::rgb(240, 240, 245),
                            super::browser::LineStyle::Heading2 => Pixel::rgb(200, 200, 210),
                            super::browser::LineStyle::Heading3 => Pixel::rgb(180, 180, 190),
                            super::browser::LineStyle::Link => Pixel::rgb(100, 180, 255),
                            super::browser::LineStyle::Error => Pixel::rgb(255, 100, 100),
                            super::browser::LineStyle::Code => Pixel::rgb(160, 220, 160),
                            super::browser::LineStyle::ListItem => Pixel::rgb(180, 180, 185),
                            _ => Pixel::rgb(170, 170, 175),
                        }
                    } else {
                        match line.style {
                            super::browser::LineStyle::Heading1 => Pixel::rgb(0, 0, 0),
                            super::browser::LineStyle::Heading2 => Pixel::rgb(30, 30, 30),
                            super::browser::LineStyle::Heading3 => Pixel::rgb(50, 50, 50),
                            super::browser::LineStyle::Link => Pixel::rgb(26, 13, 171),
                            super::browser::LineStyle::Error => Pixel::rgb(200, 0, 0),
                            super::browser::LineStyle::Code => Pixel::rgb(60, 60, 60),
                            _ => Pixel::rgb(80, 80, 80),
                        }
                    };

                    let scale = match line.style {
                        super::browser::LineStyle::Heading1 => 2,
                        super::browser::LineStyle::Heading2 => 2,
                        _ => 1,
                    };

                    if line.style == super::browser::LineStyle::Blank {
                        // Blank line — just add spacing
                    } else if line.link_url.is_some() {
                        // Underline for links
                        fonts::draw_string(fb, margin_x, y_pos, &line.text, text_color, scale);
                        // Draw underline
                        let text_w = line.text.len() as i32 * 8 * scale as i32;
                        fb.fill_rect(
                            Rect::new(margin_x, y_pos + 14 * scale as i32, text_w as u32, 1),
                            text_color,
                        );
                    } else {
                        // Word wrap long lines
                        let max_chars = (page_width as i32 / (8 * scale as i32)).max(1) as usize;
                        let is_heading = matches!(
                            line.style,
                            super::browser::LineStyle::Heading1
                                | super::browser::LineStyle::Heading2
                                | super::browser::LineStyle::Heading3
                        );
                        if line.text.len() > max_chars {
                            // Multi-line rendering
                            let mut start = 0;
                            while start < line.text.len() {
                                let end = (start + max_chars).min(line.text.len());
                                // Try to break at a space
                                let break_at = if end < line.text.len() {
                                    line.text[start..end]
                                        .rfind(' ')
                                        .map(|p| start + p + 1)
                                        .unwrap_or(end)
                                } else {
                                    end
                                };
                                if y_pos >= clip_y && y_pos < clip_y + clip_h as i32 {
                                    if is_heading {
                                        fonts::draw_string_bold(
                                            fb,
                                            margin_x,
                                            y_pos,
                                            &line.text[start..break_at],
                                            text_color,
                                            scale,
                                        );
                                    } else {
                                        fonts::draw_string(
                                            fb,
                                            margin_x,
                                            y_pos,
                                            &line.text[start..break_at],
                                            text_color,
                                            scale,
                                        );
                                    }
                                }
                                y_pos += line_height;
                                start = break_at;
                            }
                            // Don't add extra spacing since we already advanced
                            y_pos += extra_h;
                            continue;
                        } else if is_heading {
                            fonts::draw_string_bold(
                                fb, margin_x, y_pos, &line.text, text_color, scale,
                            );
                        } else {
                            fonts::draw_string(fb, margin_x, y_pos, &line.text, text_color, scale);
                        }
                    }
                }

                y_pos += line_height + extra_h;
            }

            // Status bar at bottom (shows link URL on hover)
            if !browser_state.status_text.is_empty() {
                let status_y = clip_y + clip_h as i32 - 22;
                let status_text_w =
                    (browser_state.status_text.len() as u32 * 7 + 20).min(content.width);
                let status_bg = if vivaldi_active {
                    Pixel::rgb(30, 30, 34)
                } else {
                    Pixel::rgb(240, 240, 240)
                };
                fb.fill_rounded_rect_aa(
                    Rect::new(content.x, status_y, status_text_w, 22),
                    status_bg,
                    4,
                );
                fb.draw_rounded_rect(
                    Rect::new(content.x, status_y, status_text_w, 22),
                    if vivaldi_active {
                        Pixel::rgb(50, 50, 55)
                    } else {
                        Pixel::rgb(200, 200, 200)
                    },
                    4,
                    1,
                );
                let status_color = if vivaldi_active {
                    Pixel::rgb(140, 140, 145)
                } else {
                    Pixel::rgb(100, 100, 100)
                };
                fonts::draw_string_compact(
                    fb,
                    content.x + 8,
                    status_y + 5,
                    &browser_state.status_text,
                    status_color,
                    1,
                );
            }
        } else {
            // ═══ Fallback: no browser state (shouldn't happen) ═══
            let heading_y = page_y_base + 30;
            if heading_y >= clip_y && heading_y < clip_y + clip_h as i32 {
                fonts::draw_string_bold(
                    fb,
                    content.x + 20,
                    heading_y,
                    "Welcome to KnoxOS Browser",
                    Pixel::rgb(0, 0, 0),
                    2,
                );
            }
        }

        // Scrollbar
        let total_content_height = if is_speed_dial {
            500i32
        } else {
            state
                .map(|s| (s.page.lines.len() as i32 * 20).max(200) + 60)
                .unwrap_or(320)
        };
        if total_content_height > clip_h as i32 {
            let sb_x = content.x + content.width as i32 - 8;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, clip_y, 6, clip_h),
                colors::SCROLLBAR_TRACK,
                3,
            );
            let visible_ratio = clip_h as f32 / total_content_height as f32;
            let thumb_h = ((visible_ratio * clip_h as f32) as u32).max(20).min(clip_h);
            let scroll_ratio = if total_content_height > clip_h as i32 {
                self.scroll_y as f32 / (total_content_height - clip_h as i32) as f32
            } else {
                0.0
            };
            let thumb_y = clip_y + (scroll_ratio * (clip_h - thumb_h) as f32) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, thumb_y, 6, thumb_h),
                colors::SCROLLBAR_THUMB,
                3,
            );
        }
    }

    fn draw_ai_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        // Header bar
        let header_h: u32 = 48;
        fb.fill_rect(
            Rect::new(content.x, content.y, content.width, header_h),
            Pixel::rgb(25, 28, 40),
        );
        fonts::draw_string_bold(
            fb,
            content.x + 16,
            content.y + 14,
            "KnoxOS AI Assistant",
            Pixel::rgb(100, 200, 255),
            2,
        );
        fb.draw_hline(
            content.x,
            content.y + header_h as i32 - 1,
            content.width,
            Pixel::rgb(60, 60, 80),
        );

        // Chat area with scroll support
        let chat_y = content.y + header_h as i32 - self.scroll_y;
        let chat_clip_y = content.y + header_h as i32;
        let input_h: u32 = 50;
        let chat_clip_h = content.height.saturating_sub(header_h + input_h);

        // Chat background
        fb.fill_rect(
            Rect::new(content.x, chat_clip_y, content.width, chat_clip_h),
            colors::WINDOW_BG,
        );

        // AI welcome message bubble
        let msg_y = chat_y + 16;
        let bubble_w = (content.width - 48).min(400);
        if msg_y + 10 > chat_clip_y {
            fb.fill_rounded_rect_aa(
                Rect::new(content.x + 16, msg_y, bubble_w, 80),
                Pixel::rgb(30, 35, 50),
                8,
            );

            // AI avatar indicator
            fb.fill_circle_aa(content.x + 8, msg_y + 8, 4, colors::AI_BALANCED_1);

            let welcome_text = "Hello! I'm the KnoxOS AI Assistant. I can help \
you with system tasks, answer questions, and manage your \
files. What would you like to do today?";

            fonts::draw_text_wrapped(
                fb,
                content.x + 24,
                msg_y + 8,
                bubble_w - 16,
                welcome_text,
                Pixel::rgb(200, 210, 230),
                1,
            );
        }

        // Example user message
        let user_msg_y = msg_y + 100;
        if user_msg_y + 10 > chat_clip_y && user_msg_y < chat_clip_y + chat_clip_h as i32 {
            let user_bubble_w = (content.width - 80).min(300);
            let user_bubble_x = content.x + content.width as i32 - user_bubble_w as i32 - 16;
            fb.fill_rounded_rect_aa(
                Rect::new(user_bubble_x, user_msg_y, user_bubble_w, 36),
                Pixel::rgb(0, 106, 230),
                8,
            );
            fonts::draw_text_wrapped(
                fb,
                user_bubble_x + 12,
                user_msg_y + 8,
                user_bubble_w - 24,
                "Show system information",
                colors::WHITE,
                1,
            );
        }

        // AI response with system info
        let resp_y = user_msg_y + 52;
        if resp_y + 10 > chat_clip_y && resp_y < chat_clip_y + chat_clip_h as i32 {
            let resp_bubble_w = (content.width - 48).min(400);
            fb.fill_rounded_rect_aa(
                Rect::new(content.x + 16, resp_y, resp_bubble_w, 100),
                Pixel::rgb(30, 35, 50),
                8,
            );
            fb.fill_circle_aa(content.x + 8, resp_y + 8, 4, colors::AI_BALANCED_1);
            let info_text = &alloc::format!(
                "System: KnoxOS v0.1.0\n\
CPU: {} (QEMU/KVM)\n\
Memory: 128 MB allocated\n\
Uptime: Running since boot\n\
Kernel: Rust bare-metal",
                ARCH_NAME
            );
            fonts::draw_text_wrapped(
                fb,
                content.x + 24,
                resp_y + 8,
                resp_bubble_w - 16,
                info_text,
                Pixel::rgb(200, 210, 230),
                1,
            );
        }

        // Input bar (bottom)
        let input_y = content.y + content.height as i32 - input_h as i32;
        fb.fill_rect(
            Rect::new(content.x, input_y, content.width, input_h),
            Pixel::rgb(25, 28, 40),
        );
        fb.draw_hline(content.x, input_y, content.width, Pixel::rgb(60, 60, 80));

        fb.fill_rounded_rect_aa(
            Rect::new(content.x + 12, input_y + 10, content.width - 60, 30),
            Pixel::rgb(30, 30, 40),
            6,
        );
        fb.draw_rounded_rect(
            Rect::new(content.x + 12, input_y + 10, content.width - 60, 30),
            Pixel::rgb(60, 60, 80),
            6,
            1,
        );
        fonts::draw_string_compact(
            fb,
            content.x + 20,
            input_y + 19,
            "Ask me anything...",
            Pixel::rgb(100, 100, 120),
            1,
        );

        // Send button
        let send_x = content.x + content.width as i32 - 42;
        fb.fill_rounded_rect_aa(
            Rect::new(send_x, input_y + 10, 30, 30),
            colors::AI_BALANCED_2,
            6,
        );
        fonts::draw_string_centered_bold_compact(
            fb,
            send_x,
            input_y + 10,
            30,
            30,
            "->",
            colors::WHITE,
            1,
        );

        // ── Chat scrollbar ──
        let total_chat_h = resp_y + 100 - (content.y + header_h as i32) + self.scroll_y + 16;
        let visible_chat_h = chat_clip_h as i32;
        self.max_scroll_y = (total_chat_h - visible_chat_h).max(0);
        if total_chat_h > visible_chat_h {
            let sb_w = 6i32;
            let sb_x = content.x + content.width as i32 - sb_w - 2;
            let sb_top = chat_clip_y + 2;
            let sb_track_h = chat_clip_h.saturating_sub(4);
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, sb_top, sb_w as u32, sb_track_h),
                super::colors::SCROLLBAR_TRACK,
                (sb_w / 2) as u32,
            );
            let vis_ratio = visible_chat_h as f32 / total_chat_h as f32;
            let thumb_h = ((vis_ratio * sb_track_h as f32) as u32)
                .max(16)
                .min(sb_track_h);
            let max_sc = (total_chat_h - visible_chat_h).max(1);
            let sc_ratio = (self.scroll_y as f32 / max_sc as f32).clamp(0.0, 1.0);
            let track_space = sb_track_h.saturating_sub(thumb_h) as f32;
            let thumb_y = sb_top + (sc_ratio * track_space) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h),
                super::colors::SCROLLBAR_THUMB,
                (sb_w / 2) as u32,
            );
        }
    }

    fn draw_editor_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        let total_content_h = super::editor::draw(fb, content, self.scroll_y, self.id);
        let line_h = 14i32;
        let status_h = 22i32;
        let visible_h = content.height as i32 - status_h;
        self.max_scroll_y = (total_content_h - visible_h).max(0);

        // ── Editor scrollbar ──
        if total_content_h > visible_h {
            let sb_w = 6i32;
            let minimap_offset = if content.width > 400 { 50i32 } else { 0 };
            let sb_x = content.x + content.width as i32 - minimap_offset - sb_w - 2;
            let sb_top = content.y + 2;
            let sb_track_h = (visible_h - 4).max(1) as u32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, sb_top, sb_w as u32, sb_track_h),
                super::colors::SCROLLBAR_TRACK,
                (sb_w / 2) as u32,
            );
            let vis_ratio = visible_h as f32 / total_content_h as f32;
            let thumb_h = ((vis_ratio * sb_track_h as f32) as u32)
                .max(16)
                .min(sb_track_h);
            let max_sc = (total_content_h - visible_h).max(1);
            let sc_ratio = (self.scroll_y as f32 / max_sc as f32).clamp(0.0, 1.0);
            let track_space = sb_track_h.saturating_sub(thumb_h) as f32;
            let thumb_y = sb_top + (sc_ratio * track_space) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h),
                super::colors::SCROLLBAR_THUMB,
                (sb_w / 2) as u32,
            );
        }
    }

    fn draw_settings_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        // Use the global shared settings state
        let state = super::settings::SETTINGS_STATE.lock();
        let total_h = super::settings::draw_settings(fb, content, &state, self.scroll_y);
        // Let the window know the virtual content height for scroll clamping
        let visible_h = content.height as i32;
        self.max_scroll_y = (total_h - visible_h).max(0);
    }

    /// Check if a point is within the title bar (excluding buttons)
    pub fn hit_test_titlebar(&self, x: i32, y: i32) -> bool {
        self.title_bar_rect().contains(x, y)
    }

    /// Check if a point is within the window (including resize border) — scale-aware
    pub fn hit_test(&self, x: i32, y: i32) -> bool {
        let rb = scaled_resize_border();
        let expanded = Rect::new(
            self.rect.x - rb,
            self.rect.y - rb,
            self.rect.width + (rb * 2) as u32,
            self.rect.height + (rb * 2) as u32,
        );
        expanded.contains(x, y)
    }

    /// Check if a point is strictly inside the window rect (no border expansion)
    pub fn hit_test_inner(&self, x: i32, y: i32) -> bool {
        self.rect.contains(x, y)
    }

    /// Determine which resize edge (if any) a point is on — scale-aware
    pub fn hit_test_resize_edge(&self, x: i32, y: i32) -> ResizeEdge {
        if !self.resizable
            || self.state == WindowState::Maximized
            || self.state == WindowState::SnappedLeft
            || self.state == WindowState::SnappedRight
            || self.state == WindowState::SnappedTopLeft
            || self.state == WindowState::SnappedTopRight
            || self.state == WindowState::SnappedBottomLeft
            || self.state == WindowState::SnappedBottomRight
        {
            return ResizeEdge::None;
        }

        // CRITICAL: Exclude title bar button areas from resize detection.
        // The close/maximize/minimize buttons sit near the top-right corner
        // where the resize grab zone overlaps. Without this exclusion,
        // clicking the close button triggers a TopRight resize instead.
        if (self.closeable && self.close_button_rect().contains(x, y))
            || (self.maximizable && self.maximize_button_rect().contains(x, y))
            || (self.minimizable && self.minimize_button_rect().contains(x, y))
        {
            return ResizeEdge::None;
        }

        let r = &self.rect;
        let b = scaled_resize_border();
        let on_left = x >= r.x - b && x < r.x + b;
        let on_right = x >= r.x + r.width as i32 - b && x < r.x + r.width as i32 + b;
        let on_top = y >= r.y - b && y < r.y + b;
        let on_bottom = y >= r.y + r.height as i32 - b && y < r.y + r.height as i32 + b;

        match (on_left, on_right, on_top, on_bottom) {
            (true, _, true, _) => ResizeEdge::TopLeft,
            (true, _, _, true) => ResizeEdge::BottomLeft,
            (_, true, true, _) => ResizeEdge::TopRight,
            (_, true, _, true) => ResizeEdge::BottomRight,
            (true, _, _, _) => ResizeEdge::Left,
            (_, true, _, _) => ResizeEdge::Right,
            (_, _, true, _) => ResizeEdge::Top,
            (_, _, _, true) => ResizeEdge::Bottom,
            _ => ResizeEdge::None,
        }
    }
}

/// Window manager
pub const NUM_WORKSPACES: u8 = 4;
/// Special value meaning "visible on all workspaces"
pub const WORKSPACE_ALL: u8 = u8::MAX;

/// Auto-tiling layout mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingMode {
    /// Floating mode — windows are freely positioned (default)
    Floating,
    /// Master-stack: one large master pane on left, stack of windows on right (60/40 split)
    MasterStack,
    /// Grid: all windows arranged in an even grid
    Grid,
    /// Monocle: one maximized window at a time, cycle through them
    Monocle,
    /// Columns: windows arranged in equal-width vertical columns
    Columns,
}

pub struct WindowManager {
    pub windows: Vec<Window>,
    pub focused_window: Option<WindowId>,
    /// Currently active workspace (0..NUM_WORKSPACES-1)
    pub current_workspace: u8,
    /// Automatic tiling layout mode
    pub tiling_mode: TilingMode,
    /// Master-stack ratio (0.0–1.0, default 0.6 = 60% master)
    pub master_ratio: f32,
}

// WindowManager::new() and Default impl moved to wm_core.rs
// WindowManager methods moved to wm_core.rs and wm_layout.rs

lazy_static::lazy_static! {
    pub static ref WINDOW_MANAGER: Mutex<WindowManager> = Mutex::new(WindowManager::new());
}

/// Open a browser window navigated to the given URL
pub fn open_browser_window(url: &str) {
    let mut win = Window::new("Browser", 200, 100, 900, 650);
    win.content_type = WindowContentType::Browser;
    // Store the URL in the title so the browser renderer can use it
    win.title = alloc::format!("Browser — {}", url);
    let mut wm = WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);
    super::request_redraw();
}

/// Open a file explorer window at a given path
pub fn open_file_explorer_at(path: &str) {
    let title = alloc::format!("Files — {}", path);
    let mut win = Window::new(&title, 180, 80, 800, 550);
    win.content_type = WindowContentType::FileExplorer;
    let mut wm = WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);
    super::request_redraw();
}
