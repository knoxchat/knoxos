/// Window construction, scrolling, screen clamping, chrome rects, and hit testing
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use crate::gui::colors;
use crate::gui::framebuffer::Rect;
use crate::gui::scale;
use crate::gui::window_attrs::{WindowAttributes, WindowButtons, WindowLevel};

use super::decorations::{
    scaled_btn_gap, scaled_btn_height, scaled_btn_margin_right, scaled_btn_width,
    scaled_min_height, scaled_min_visible, scaled_min_width, scaled_resize_border,
    scaled_title_bar_height,
};
use super::types::{
    BORDER_WIDTH, CURRENT_WORKSPACE, ExplorerSort, MAX_SCROLL_Y, NEXT_WINDOW_ID, ResizeEdge,
    WORKSPACE_ALL, Window, WindowContentType, WindowState,
};

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
