use super::framebuffer::{FrameBuffer, Rect};
use super::scale;
use super::window::*;
/// Window manager core — add, focus, close, minimize, maximize, z-ordering, workspaces
/// Extracted from window.rs for modularity.
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            focused_window: None,
            current_workspace: 0,
            tiling_mode: TilingMode::Floating,
            master_ratio: 0.6,
        }
    }

    pub fn add_window(&mut self, mut window: Window) {
        apply_window_rules(&mut window);

        let (sw, sh) = super::screen_size();
        let usable_h = sh.saturating_sub(scale::taskbar_height() as usize);
        if window.rect.width > sw as u32 {
            window.rect.width = sw as u32;
        }
        if window.rect.height > usable_h as u32 {
            window.rect.height = usable_h as u32;
        }
        window.enforce_min_size();
        window.clamp_to_screen(sw as i32, sh as i32);
        window.saved_rect = window.rect;
        window.pre_snap_rect = window.rect;
        window.z_order = self.windows.len() as u32;

        window.start_open_animation();

        let id = window.id;
        self.windows.push(window);
        self.focus_window(id);
    }

    pub fn focus_window(&mut self, id: WindowId) {
        let mut focused_title = alloc::string::String::new();
        for w in self.windows.iter_mut() {
            w.focused = w.id == id;
            if w.id == id {
                if w.state == WindowState::Minimized {
                    w.state = WindowState::Normal;
                }
                w.visible = true;
                focused_title = w.title.clone();
            }
        }
        self.focused_window = Some(id);

        if !focused_title.is_empty() {
            super::accessibility::announce_window_focus(&focused_title);
        }

        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let win = self.windows.remove(pos);
            self.windows.push(win);
        }

        for (i, w) in self.windows.iter_mut().enumerate() {
            w.z_order = i as u32;
        }
    }

    pub fn close_window(&mut self, id: WindowId) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            if w.anim
                .as_ref()
                .map(|a| matches!(a.kind, WindowAnimationType::Close))
                .unwrap_or(false)
            {
                return;
            }
            w.start_close_animation();
            if self.focused_window == Some(id) {
                self.focused_window = self
                    .windows
                    .iter()
                    .rev()
                    .filter(|w2| w2.id != id && w2.is_visible())
                    .map(|w2| w2.id)
                    .next();
            }
        } else {
            return;
        }
        super::window_events::unregister_window(id);
        super::editor::close_editor(id);
        super::task_manager::close(id);
        super::calculator::close(id);
        super::image_viewer::close(id);

        super::sounds::window_close();
    }

    /// Remove windows whose close animation has finished
    pub fn finish_closed_windows(&mut self) {
        let had_focused = self.focused_window;
        self.windows.retain(|w| {
            if let Some(ref a) = w.anim {
                if matches!(a.kind, WindowAnimationType::Close) && a.progress >= 1.0 {
                    return false;
                }
            }
            true
        });
        for (i, w) in self.windows.iter_mut().enumerate() {
            w.z_order = i as u32;
        }
        if let Some(fid) = had_focused {
            if !self.windows.iter().any(|w| w.id == fid) {
                self.focused_window = self
                    .windows
                    .iter()
                    .rev()
                    .find(|w| w.is_visible())
                    .map(|w| w.id);
                if let Some(new_fid) = self.focused_window {
                    self.focus_window(new_fid);
                } else {
                    for w in self.windows.iter_mut() {
                        w.focused = false;
                    }
                }
            }
        }
    }

    pub fn minimize_window(&mut self, id: WindowId) {
        let (sw, sh) = super::screen_size();
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.start_minimize_animation(sw as i32, sh as i32);
            w.state = WindowState::Minimized;
            w.focused = false;
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
        if let Some(next) = self.windows.iter().rev().find(|w| w.is_visible()) {
            let next_id = next.id;
            self.focus_window(next_id);
        } else {
            self.focused_window = None;
            for w in self.windows.iter_mut() {
                w.focused = false;
            }
        }
    }

    pub fn toggle_maximize(&mut self, id: WindowId, screen_width: u32, screen_height: u32) {
        let taskbar_h = scale::taskbar_height();
        let usable_h = screen_height.saturating_sub(taskbar_h);
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let old_rect = w.rect;
            match w.state {
                WindowState::Maximized => {
                    w.state = WindowState::Normal;
                    w.rect = w.saved_rect;
                    w.enforce_min_size();
                    w.clamp_to_screen(screen_width as i32, screen_height as i32);
                }
                WindowState::SnappedLeft
                | WindowState::SnappedRight
                | WindowState::SnappedTopLeft
                | WindowState::SnappedTopRight
                | WindowState::SnappedBottomLeft
                | WindowState::SnappedBottomRight => {
                    w.saved_rect = w.pre_snap_rect;
                    w.state = WindowState::Maximized;
                    w.rect = Rect::new(0, 0, screen_width, usable_h);
                }
                _ => {
                    w.saved_rect = w.rect;
                    w.state = WindowState::Maximized;
                    w.rect = Rect::new(0, 0, screen_width, usable_h);
                }
            }
            w.start_transition_animation(old_rect, w.rect);
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }

    /// Get a window by ID
    pub fn get_window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    /// Get a mutable window by ID
    pub fn get_window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    /// Get the number of visible (non-minimized) windows
    pub fn visible_count(&self) -> usize {
        self.windows.iter().filter(|w| w.is_visible()).count()
    }

    pub fn draw_all(&mut self, fb: &mut FrameBuffer) {
        let mut any_animating = false;
        for w in self.windows.iter_mut() {
            if w.is_animating() {
                let still_running = w.tick_animation();
                if still_running {
                    any_animating = true;
                    let pad = 20i32;
                    let r = w.rect;
                    super::push_damage(Rect::new(
                        r.x - pad,
                        r.y - pad,
                        r.width + pad as u32 * 2,
                        r.height + pad as u32 * 2,
                    ));
                    if let Some(ref a) = w.anim {
                        let fr = a.from_rect;
                        super::push_damage(Rect::new(
                            fr.x - pad,
                            fr.y - pad,
                            fr.width + pad as u32 * 2,
                            fr.height + pad as u32 * 2,
                        ));
                    }
                }
            }
        }
        self.finish_closed_windows();
        if any_animating {
            super::NEEDS_REDRAW.store(true, core::sync::atomic::Ordering::Relaxed);
        }

        // Z-order optimized compositing
        let mut start_idx = 0;
        let mut has_snapped_left = false;
        let mut has_snapped_right = false;

        for (i, w) in self.windows.iter().enumerate() {
            if !w.is_visible() {
                continue;
            }
            match w.state {
                WindowState::Maximized => {
                    start_idx = i;
                    has_snapped_left = false;
                    has_snapped_right = false;
                }
                WindowState::SnappedLeft => {
                    has_snapped_left = true;
                    if has_snapped_right {
                        start_idx = i.min(
                            self.windows[..i]
                                .iter()
                                .rposition(|w2| {
                                    w2.state == WindowState::SnappedRight && w2.is_visible()
                                })
                                .unwrap_or(start_idx),
                        );
                    }
                }
                WindowState::SnappedRight => {
                    has_snapped_right = true;
                    if has_snapped_left {
                        start_idx = i.min(
                            self.windows[..i]
                                .iter()
                                .rposition(|w2| {
                                    w2.state == WindowState::SnappedLeft && w2.is_visible()
                                })
                                .unwrap_or(start_idx),
                        );
                    }
                }
                _ => {}
            }
        }

        let clip = fb.clip_rect();

        for window in &mut self.windows[start_idx..] {
            if !window.is_visible() {
                continue;
            }

            if let Some(clip_rect) = clip {
                let shadow_pad = 20i32;
                let padded = Rect::new(
                    window.rect.x - shadow_pad,
                    window.rect.y - shadow_pad,
                    window.rect.width + shadow_pad as u32 * 2,
                    window.rect.height + shadow_pad as u32 * 2,
                );
                if !padded.intersects(&clip_rect) {
                    continue;
                }
            }

            window.draw(fb);
        }
    }

    pub fn window_at(&self, x: i32, y: i32) -> Option<WindowId> {
        for window in self.windows.iter().rev() {
            if window.is_visible() && window.hit_test(x, y) {
                return Some(window.id);
            }
        }
        None
    }

    pub fn window_at_inner(&self, x: i32, y: i32) -> Option<WindowId> {
        for window in self.windows.iter().rev() {
            if window.is_visible() && window.hit_test_inner(x, y) {
                return Some(window.id);
            }
        }
        None
    }

    pub fn resize_edge_at(&self, x: i32, y: i32) -> (Option<WindowId>, ResizeEdge) {
        for window in self.windows.iter().rev() {
            if window.is_visible() {
                let edge = window.hit_test_resize_edge(x, y);
                if edge.is_resizing() {
                    return (Some(window.id), edge);
                }
            }
        }
        (None, ResizeEdge::None)
    }

    pub fn any_dragging(&self) -> bool {
        self.windows.iter().any(|w| w.dragging)
    }

    pub fn any_resizing(&self) -> bool {
        self.windows.iter().any(|w| w.resizing.is_resizing())
    }

    pub fn cycle_focus(&mut self, reverse: bool) {
        let visible: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.is_visible())
            .map(|w| w.id)
            .collect();

        if visible.is_empty() {
            return;
        }

        let current_idx = self
            .focused_window
            .and_then(|fid| visible.iter().position(|&id| id == fid));

        let next_id = match current_idx {
            Some(idx) => {
                if reverse {
                    if idx == 0 {
                        visible[visible.len() - 1]
                    } else {
                        visible[idx - 1]
                    }
                } else {
                    visible[(idx + 1) % visible.len()]
                }
            }
            None => visible[0],
        };

        self.focus_window(next_id);
    }

    /// Switch to a different virtual workspace
    pub fn switch_workspace(&mut self, ws: u8) {
        if ws >= NUM_WORKSPACES {
            return;
        }
        self.current_workspace = ws;
        CURRENT_WORKSPACE.store(ws, Ordering::Relaxed);
        self.focused_window = self
            .windows
            .iter()
            .rev()
            .find(|w| w.is_visible())
            .map(|w| w.id);
    }

    /// Move a window to a specific workspace
    pub fn move_window_to_workspace(&mut self, id: WindowId, ws: u8) {
        if ws >= NUM_WORKSPACES {
            return;
        }
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.workspace = ws;
        }
        if self.focused_window == Some(id) {
            self.focused_window = self
                .windows
                .iter()
                .rev()
                .find(|w| w.is_visible())
                .map(|w| w.id);
        }
    }

    /// Move focused window to a workspace and optionally follow it
    pub fn move_focused_to_workspace(&mut self, ws: u8, follow: bool) {
        if let Some(fid) = self.focused_window {
            self.move_window_to_workspace(fid, ws);
            if follow {
                self.switch_workspace(ws);
            }
        }
    }

    /// Show all windows (restore from "show desktop")
    pub fn show_all_windows(&mut self) {
        for w in self.windows.iter_mut() {
            w.visible = true;
            if w.state == WindowState::Minimized {
                w.state = WindowState::Normal;
            }
        }
        if let Some(top) = self.windows.last() {
            let id = top.id;
            self.focus_window(id);
        }
    }

    /// Hide all windows ("show desktop" feature)
    pub fn hide_all_windows(&mut self) {
        for w in self.windows.iter_mut() {
            if w.state != WindowState::Minimized {
                w.state = WindowState::Minimized;
            }
            w.focused = false;
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
        self.focused_window = None;
    }

    /// Toggle "show desktop"
    pub fn toggle_show_desktop(&mut self) {
        let any_visible = self.windows.iter().any(|w| w.is_visible());
        if any_visible {
            self.hide_all_windows();
        } else {
            self.show_all_windows();
        }
    }

    /// Cancel all drag/resize operations
    pub fn cancel_all_interactions(&mut self) {
        for w in self.windows.iter_mut() {
            w.dragging = false;
            w.resizing = ResizeEdge::None;
        }
    }
}
