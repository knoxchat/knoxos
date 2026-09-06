use super::framebuffer::Rect;
use super::scale;
use super::window::*;
use super::window_attrs::WindowLevel;
/// Window layout algorithms — tiling, snapping, cascading, PIP, resize
/// Extracted from window.rs for modularity.
use alloc::vec::Vec;

impl WindowManager {
    /// Re-fit all windows after a display resolution change.
    pub fn refit_all_windows(&mut self, screen_width: u32, screen_height: u32) {
        let usable_h = screen_height.saturating_sub(scale::taskbar_height());
        for w in self.windows.iter_mut() {
            w.dragging = false;
            w.resizing = ResizeEdge::None;

            match w.state {
                WindowState::Maximized => {
                    w.rect = Rect::new(0, 0, screen_width, usable_h);
                }
                WindowState::SnappedLeft => {
                    w.rect = Rect::new(0, 0, screen_width / 2, usable_h);
                }
                WindowState::SnappedRight => {
                    w.rect = Rect::new(screen_width as i32 / 2, 0, screen_width / 2, usable_h);
                }
                WindowState::SnappedTopLeft => {
                    w.rect = Rect::new(0, 0, screen_width / 2, usable_h / 2);
                }
                WindowState::SnappedTopRight => {
                    w.rect = Rect::new(screen_width as i32 / 2, 0, screen_width / 2, usable_h / 2);
                }
                WindowState::SnappedBottomLeft => {
                    w.rect = Rect::new(0, usable_h as i32 / 2, screen_width / 2, usable_h / 2);
                }
                WindowState::SnappedBottomRight => {
                    w.rect = Rect::new(
                        screen_width as i32 / 2,
                        usable_h as i32 / 2,
                        screen_width / 2,
                        usable_h / 2,
                    );
                }
                WindowState::Normal | WindowState::Minimized => {
                    if w.rect.width > screen_width {
                        w.rect.width = screen_width;
                    }
                    if w.rect.height > usable_h {
                        w.rect.height = usable_h;
                    }
                    w.enforce_min_size();
                    w.clamp_to_screen(screen_width as i32, screen_height as i32);
                }
            }
        }
    }

    /// Snap window to left half of screen
    pub fn snap_left(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            if w.state == WindowState::Normal {
                w.pre_snap_rect = w.rect;
                w.saved_rect = w.rect;
            }
            w.state = WindowState::SnappedLeft;
            w.rect = Rect::new(0, 0, screen_width / 2, usable_h);
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Snap window to right half of screen
    pub fn snap_right(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            if w.state == WindowState::Normal {
                w.pre_snap_rect = w.rect;
                w.saved_rect = w.rect;
            }
            w.state = WindowState::SnappedRight;
            w.rect = Rect::new(screen_width as i32 / 2, 0, screen_width / 2, usable_h);
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Snap window to top-left quarter of screen
    pub fn snap_top_left(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            if w.state == WindowState::Normal {
                w.pre_snap_rect = w.rect;
                w.saved_rect = w.rect;
            }
            w.state = WindowState::SnappedTopLeft;
            w.rect = Rect::new(0, 0, screen_width / 2, usable_h / 2);
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Snap window to top-right quarter of screen
    pub fn snap_top_right(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            if w.state == WindowState::Normal {
                w.pre_snap_rect = w.rect;
                w.saved_rect = w.rect;
            }
            w.state = WindowState::SnappedTopRight;
            w.rect = Rect::new(screen_width as i32 / 2, 0, screen_width / 2, usable_h / 2);
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Snap window to bottom-left quarter of screen
    pub fn snap_bottom_left(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            if w.state == WindowState::Normal {
                w.pre_snap_rect = w.rect;
                w.saved_rect = w.rect;
            }
            w.state = WindowState::SnappedBottomLeft;
            w.rect = Rect::new(0, usable_h as i32 / 2, screen_width / 2, usable_h / 2);
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Snap window to bottom-right quarter of screen
    pub fn snap_bottom_right(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            if w.state == WindowState::Normal {
                w.pre_snap_rect = w.rect;
                w.saved_rect = w.rect;
            }
            w.state = WindowState::SnappedBottomRight;
            w.rect = Rect::new(
                screen_width as i32 / 2,
                usable_h as i32 / 2,
                screen_width / 2,
                usable_h / 2,
            );
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Toggle PIP (Picture-in-Picture) mode for a window.
    pub fn toggle_pip(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            if w.window_level == WindowLevel::AlwaysOnTop
                && w.rect.width <= 400
                && w.rect.height <= 300
            {
                // Restore from PIP
                w.window_level = WindowLevel::Normal;
                w.state = WindowState::Normal;
                w.rect = w.saved_rect;
                w.enforce_min_size();
                w.clamp_to_screen(screen_width as i32, screen_height as i32);
            } else {
                // Enter PIP mode
                w.saved_rect = w.rect;
                w.pre_snap_rect = w.rect;
                w.window_level = WindowLevel::AlwaysOnTop;
                w.state = WindowState::Normal;
                let pip_w: u32 = 320;
                let pip_h: u32 = 240;
                let pip_x = screen_width as i32 - pip_w as i32 - 16;
                let pip_y = screen_height as i32 - taskbar_h as i32 - pip_h as i32 - 16;
                w.rect = Rect::new(pip_x, pip_y, pip_w, pip_h);
            }
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Unsnap/restore a window to its pre-snap position
    pub fn unsnap(&mut self, id: WindowId) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            if matches!(
                w.state,
                WindowState::SnappedLeft
                    | WindowState::SnappedRight
                    | WindowState::SnappedTopLeft
                    | WindowState::SnappedTopRight
                    | WindowState::SnappedBottomLeft
                    | WindowState::SnappedBottomRight
            ) {
                let old_rect = w.rect;
                w.state = WindowState::Normal;
                w.rect = w.pre_snap_rect;
                w.start_transition_animation(old_rect, w.rect);
            }
        }
    }

    /// Apply resize delta to a window being resized
    pub fn apply_resize(
        &mut self,
        id: WindowId,
        edge: ResizeEdge,
        mouse_x: i32,
        mouse_y: i32,
        screen_height: i32,
    ) {
        let (screen_w, _) = super::screen_size();
        let mw = scaled_min_width() as i32;
        let mh = scaled_min_height() as i32;
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let r = &mut w.rect;
            let taskbar_bottom = screen_height - scale::taskbar_height() as i32;
            let mouse_x = mouse_x.max(0).min(screen_w as i32);
            let mouse_y = mouse_y.max(0).min(taskbar_bottom);

            match edge {
                ResizeEdge::Right => {
                    let new_w = (mouse_x - r.x).max(mw) as u32;
                    r.width = new_w;
                }
                ResizeEdge::Bottom => {
                    let new_h = (mouse_y.min(taskbar_bottom) - r.y).max(mh) as u32;
                    r.height = new_h;
                }
                ResizeEdge::Left => {
                    let right = r.x + r.width as i32;
                    let new_x = mouse_x.min(right - mw);
                    r.width = (right - new_x) as u32;
                    r.x = new_x;
                }
                ResizeEdge::Top => {
                    let bottom = r.y + r.height as i32;
                    let new_y = mouse_y.max(0).min(bottom - mh);
                    r.height = (bottom - new_y) as u32;
                    r.y = new_y;
                }
                ResizeEdge::TopLeft => {
                    let right = r.x + r.width as i32;
                    let bottom = r.y + r.height as i32;
                    let new_x = mouse_x.min(right - mw);
                    let new_y = mouse_y.max(0).min(bottom - mh);
                    r.width = (right - new_x) as u32;
                    r.height = (bottom - new_y) as u32;
                    r.x = new_x;
                    r.y = new_y;
                }
                ResizeEdge::TopRight => {
                    let bottom = r.y + r.height as i32;
                    let new_y = mouse_y.max(0).min(bottom - mh);
                    let new_w = (mouse_x - r.x).max(mw) as u32;
                    r.width = new_w;
                    r.height = (bottom - new_y) as u32;
                    r.y = new_y;
                }
                ResizeEdge::BottomLeft => {
                    let right = r.x + r.width as i32;
                    let new_x = mouse_x.min(right - mw);
                    let new_h = (mouse_y.min(taskbar_bottom) - r.y).max(mh) as u32;
                    r.width = (right - new_x) as u32;
                    r.height = new_h;
                    r.x = new_x;
                }
                ResizeEdge::BottomRight => {
                    let new_w = (mouse_x - r.x).max(mw) as u32;
                    let new_h = (mouse_y.min(taskbar_bottom) - r.y).max(mh) as u32;
                    r.width = new_w;
                    r.height = new_h;
                }
                ResizeEdge::None => {}
            }
            w.enforce_min_size();
        }
    }

    /// Tile all visible windows in a grid layout
    pub fn tile_windows(&mut self, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        let visible: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.is_visible())
            .map(|w| w.id)
            .collect();

        let n = visible.len();
        if n == 0 {
            return;
        }

        let cols = match n {
            1 => 1u32,
            2 => 2,
            3..=4 => 2,
            5..=6 => 3,
            _ => (libm::ceilf(libm::sqrtf(n as f32))) as u32,
        };
        let rows = (n as u32).div_ceil(cols);

        let tile_w = screen_width / cols;
        let tile_h = usable_h / rows;

        for (i, wid) in visible.iter().enumerate() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == *wid) {
                let col = (i as u32) % cols;
                let row = (i as u32) / cols;
                w.saved_rect = w.rect;
                w.pre_snap_rect = w.rect;
                w.state = WindowState::Normal;
                w.rect = Rect::new((col * tile_w) as i32, (row * tile_h) as i32, tile_w, tile_h);
                w.enforce_min_size();
            }
        }
    }

    /// Cascade all visible windows from top-left
    pub fn cascade_windows(&mut self, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        let visible: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.is_visible())
            .map(|w| w.id)
            .collect();

        let default_w = (screen_width * 2 / 3).max(MIN_WIDTH);
        let default_h = (usable_h * 2 / 3).max(MIN_HEIGHT);

        for (i, wid) in visible.iter().enumerate() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == *wid) {
                let offset = (i as i32) * CASCADE_OFFSET;
                w.state = WindowState::Normal;
                w.rect = Rect::new(
                    40 + offset,
                    40 + offset,
                    default_w.min(screen_width - offset as u32),
                    default_h.min(usable_h - offset as u32),
                );
                w.enforce_min_size();
                w.clamp_to_screen(screen_width as i32, screen_height as i32);
            }
        }
    }

    /// Set the tiling mode and immediately re-tile
    pub fn set_tiling_mode(&mut self, mode: TilingMode, screen_width: u32, screen_height: u32) {
        self.tiling_mode = mode;
        self.apply_tiling(screen_width, screen_height);
        crate::serial_println!("[wm] Tiling mode: {:?}", mode);
    }

    /// Apply current tiling layout to all visible windows
    pub fn apply_tiling(&mut self, screen_width: u32, screen_height: u32) {
        match self.tiling_mode {
            TilingMode::Floating => {}
            TilingMode::Grid => {
                self.tile_windows(screen_width, screen_height);
            }
            TilingMode::MasterStack => {
                self.tile_master_stack(screen_width, screen_height);
            }
            TilingMode::Monocle => {
                self.tile_monocle(screen_width, screen_height);
            }
            TilingMode::Columns => {
                self.tile_columns(screen_width, screen_height);
            }
        }
    }

    /// Master-stack tiling: one large master on left, stack on right
    pub fn tile_master_stack(&mut self, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        let visible: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.is_visible())
            .map(|w| w.id)
            .collect();

        let n = visible.len();
        if n == 0 {
            return;
        }

        let master_w = (screen_width as f32 * self.master_ratio) as u32;
        let stack_w = screen_width - master_w;
        let gap = 4u32;

        if let Some(w) = self.windows.iter_mut().find(|w| w.id == visible[0]) {
            w.saved_rect = w.rect;
            w.pre_snap_rect = w.rect;
            w.state = WindowState::Normal;
            w.rect = Rect::new(
                gap as i32,
                gap as i32,
                master_w - gap * 2,
                usable_h - gap * 2,
            );
            w.enforce_min_size();
        }

        if n > 1 {
            let stack_count = (n - 1) as u32;
            let stack_h = (usable_h - gap * (stack_count + 1)) / stack_count;

            for (i, wid) in visible[1..].iter().enumerate() {
                if let Some(w) = self.windows.iter_mut().find(|w| w.id == *wid) {
                    w.saved_rect = w.rect;
                    w.pre_snap_rect = w.rect;
                    w.state = WindowState::Normal;
                    w.rect = Rect::new(
                        (master_w + gap) as i32,
                        (gap + (i as u32) * (stack_h + gap)) as i32,
                        stack_w - gap * 2,
                        stack_h,
                    );
                    w.enforce_min_size();
                }
            }
        }
    }

    /// Monocle tiling: one maximized window visible at a time
    pub fn tile_monocle(&mut self, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        let visible: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.is_visible())
            .map(|w| w.id)
            .collect();

        let focused = self.focused_window;

        for wid in &visible {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == *wid) {
                w.saved_rect = w.rect;
                w.pre_snap_rect = w.rect;
                w.state = WindowState::Normal;
                w.rect = Rect::new(0, 0, screen_width, usable_h);
                w.visible = !(focused.is_some() && focused != Some(*wid));
            }
        }
    }

    /// Columns tiling: equal-width vertical columns
    pub fn tile_columns(&mut self, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        let visible: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.is_visible())
            .map(|w| w.id)
            .collect();

        let n = visible.len();
        if n == 0 {
            return;
        }

        let gap = 4u32;
        let col_w = (screen_width - gap * (n as u32 + 1)) / n as u32;

        for (i, wid) in visible.iter().enumerate() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == *wid) {
                w.saved_rect = w.rect;
                w.pre_snap_rect = w.rect;
                w.state = WindowState::Normal;
                w.rect = Rect::new(
                    (gap + (i as u32) * (col_w + gap)) as i32,
                    gap as i32,
                    col_w,
                    usable_h - gap * 2,
                );
                w.enforce_min_size();
            }
        }
    }

    /// Adjust master ratio for master-stack layout
    pub fn adjust_master_ratio(&mut self, delta: f32, screen_width: u32, screen_height: u32) {
        self.master_ratio = (self.master_ratio + delta).clamp(0.2, 0.8);
        if self.tiling_mode == TilingMode::MasterStack {
            self.tile_master_stack(screen_width, screen_height);
        }
    }
}
