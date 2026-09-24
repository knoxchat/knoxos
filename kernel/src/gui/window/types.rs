/// Window identity, state, content types, explorer/animation types, and design constants
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU8;
use spin::Mutex;

use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::window_attrs::WindowLevel;

/// Global atomic current workspace index (for is_visible checks without needing WM lock)
pub static CURRENT_WORKSPACE: AtomicU8 = AtomicU8::new(0);

/// Window identifier
pub type WindowId = u32;

pub(super) static NEXT_WINDOW_ID: Mutex<WindowId> = Mutex::new(1);

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

/// Window manager
pub const NUM_WORKSPACES: u8 = 4;
/// Special value meaning "visible on all workspaces"
pub const WORKSPACE_ALL: u8 = u8::MAX;
