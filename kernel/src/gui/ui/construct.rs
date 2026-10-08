use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Layout, Region};

use super::{InputState, Style, Ui};

impl<'a> Ui<'a> {
    // ── Construction ─────────────────────────────────────────────

    /// Create a new top-level Ui filling the given rectangle.
    pub fn new(fb: &'a mut FrameBuffer, max_rect: Rect, input: &'a InputState) -> Self {
        let layout = Layout::default();
        let style = Style::default();
        let padded = Rect::new(
            max_rect.x + style.spacing.window_padding_x,
            max_rect.y + style.spacing.window_padding_y,
            (max_rect.width as i32 - style.spacing.window_padding_x * 2).max(0) as u32,
            (max_rect.height as i32 - style.spacing.window_padding_y * 2).max(0) as u32,
        );
        let region = Region::from_max_rect(&layout, padded);
        Self {
            fb,
            input,
            layout,
            region,
            style: Style::default(),
            id: Id::from_str("root"),
            next_auto_id: 1,
            enabled: true,
            clip_rect: max_rect,
        }
    }

    /// Create a Ui with no window padding — for painting into an existing
    /// window content rect (calculator grid, settings pane, etc.).
    pub fn new_tight(fb: &'a mut FrameBuffer, max_rect: Rect, input: &'a InputState) -> Self {
        let layout = Layout::default();
        let style = Style::default();
        let region = Region::from_max_rect(&layout, max_rect);
        Self {
            fb,
            input,
            layout,
            region,
            style,
            id: Id::from_str("root"),
            next_auto_id: 1,
            enabled: true,
            clip_rect: max_rect,
        }
    }

    /// Create a Ui with a specific layout and ID.
    pub fn new_with(
        fb: &'a mut FrameBuffer,
        max_rect: Rect,
        input: &'a InputState,
        layout: Layout,
        id: Id,
        style: Style,
    ) -> Self {
        let padded = Rect::new(
            max_rect.x + style.spacing.window_padding_x,
            max_rect.y + style.spacing.window_padding_y,
            (max_rect.width as i32 - style.spacing.window_padding_x * 2).max(0) as u32,
            (max_rect.height as i32 - style.spacing.window_padding_y * 2).max(0) as u32,
        );
        let region = Region::from_max_rect(&layout, padded);
        Self {
            fb,
            input,
            layout,
            region,
            style,
            id,
            next_auto_id: 1,
            enabled: true,
            clip_rect: max_rect,
        }
    }

    /// Create a Ui with no padding (for internal child regions).
    pub fn new_child_raw(
        fb: &'a mut FrameBuffer,
        max_rect: Rect,
        input: &'a InputState,
        layout: Layout,
        id: Id,
        style: Style,
        clip_rect: Rect,
    ) -> Self {
        let region = Region::from_max_rect(&layout, max_rect);
        Self {
            fb,
            input,
            layout,
            region,
            style,
            id,
            next_auto_id: 1,
            enabled: true,
            clip_rect,
        }
    }

    // ── ID generation ────────────────────────────────────────────

    /// Get a unique auto-generated ID for an anonymous widget.
    pub fn auto_id(&mut self) -> Id {
        let id = self.id.with_index(self.next_auto_id as usize);
        self.next_auto_id += 1;
        id
    }

    /// Get a stable ID for a named widget.
    pub fn id_from(&self, salt: &str) -> Id {
        self.id.with(salt)
    }

    // ── Layout accessors ─────────────────────────────────────────

    /// The remaining available width for widgets.
    pub fn available_width(&self) -> i32 {
        self.region.available_width(&self.layout)
    }

    /// The remaining available height for widgets.
    pub fn available_height(&self) -> i32 {
        self.region.available_height(&self.layout)
    }

    /// The full max_rect (available area before any widgets are placed).
    pub fn max_rect(&self) -> Rect {
        self.region.max_rect
    }

    /// The bounding rect of all widgets placed so far.
    pub fn min_rect(&self) -> Rect {
        self.region.min_rect
    }

    /// Current cursor position.
    pub fn cursor(&self) -> (i32, i32) {
        (self.region.cursor_x, self.region.cursor_y)
    }

    /// Current style reference.
    pub fn style(&self) -> &Style {
        &self.style
    }

    /// Mutable style reference.
    pub fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    /// Set the layout for subsequent widgets.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }
}
