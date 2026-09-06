/// Layout Engine — Automatic widget placement for immediate-mode UI.
///
/// Inspired by egui's `Layout` and `Region`: supports vertical (top-down),
/// horizontal (left-to-right), and wrapping layouts. All coordinates are
/// integer-based (i32/u32) — no floating point required.
///
/// The layout engine tracks a *cursor* that advances as widgets are placed,
/// and a *region* that records the bounding box of all placed widgets.
use super::framebuffer::Rect;

// ═══════════════════════════════════════════════════════════════════════
// Alignment
// ═══════════════════════════════════════════════════════════════════════

/// Alignment on a single axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// Left / Top
    Min,
    /// Center
    Center,
    /// Right / Bottom
    Max,
}

impl Align {
    pub const LEFT: Align = Align::Min;
    pub const RIGHT: Align = Align::Max;
    pub const TOP: Align = Align::Min;
    pub const BOTTOM: Align = Align::Max;
    pub const CENTER: Align = Align::Center;
}

// ═══════════════════════════════════════════════════════════════════════
// Direction
// ═══════════════════════════════════════════════════════════════════════

/// Layout direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
    TopDown,
    BottomUp,
}

impl Direction {
    #[inline]
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::LeftToRight | Self::RightToLeft)
    }

    #[inline]
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::TopDown | Self::BottomUp)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Layout
// ═══════════════════════════════════════════════════════════════════════

/// Describes how to place widgets: direction, alignment, justification, wrapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Main axis direction.
    pub main_dir: Direction,
    /// Wrap to next row/column when the main axis overflows.
    pub main_wrap: bool,
    /// Alignment along the main axis.
    pub main_align: Align,
    /// Justify: stretch widgets to fill the main axis.
    pub main_justify: bool,
    /// Alignment along the cross axis.
    pub cross_align: Align,
    /// Justify along the cross axis (fill full width/height).
    pub cross_justify: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self::top_down(Align::LEFT)
    }
}

/// Constructors
impl Layout {
    /// Vertical layout, top to bottom. `halign` controls horizontal alignment.
    #[inline]
    pub fn top_down(halign: Align) -> Self {
        Self {
            main_dir: Direction::TopDown,
            main_wrap: false,
            main_align: Align::Center,
            main_justify: false,
            cross_align: halign,
            cross_justify: false,
        }
    }

    /// Vertical layout, top to bottom, widgets justified (fill full width).
    #[inline]
    pub fn top_down_justified(halign: Align) -> Self {
        Self::top_down(halign).with_cross_justify(true)
    }

    /// Horizontal layout, left to right. `valign` controls vertical alignment.
    #[inline]
    pub fn left_to_right(valign: Align) -> Self {
        Self {
            main_dir: Direction::LeftToRight,
            main_wrap: false,
            main_align: Align::Center,
            main_justify: false,
            cross_align: valign,
            cross_justify: false,
        }
    }

    /// Horizontal layout, right to left.
    #[inline]
    pub fn right_to_left(valign: Align) -> Self {
        Self {
            main_dir: Direction::RightToLeft,
            main_wrap: false,
            main_align: Align::Center,
            main_justify: false,
            cross_align: valign,
            cross_justify: false,
        }
    }

    /// Vertical layout, bottom to top.
    #[inline]
    pub fn bottom_up(halign: Align) -> Self {
        Self {
            main_dir: Direction::BottomUp,
            main_wrap: false,
            main_align: Align::Center,
            main_justify: false,
            cross_align: halign,
            cross_justify: false,
        }
    }

    /// Centered and justified: single widget fills the whole region.
    #[inline]
    pub fn centered_and_justified(main_dir: Direction) -> Self {
        Self {
            main_dir,
            main_wrap: false,
            main_align: Align::Center,
            main_justify: true,
            cross_align: Align::Center,
            cross_justify: true,
        }
    }

    // ── Builder modifiers ──

    #[inline]
    pub fn with_main_wrap(mut self, wrap: bool) -> Self {
        self.main_wrap = wrap;
        self
    }

    #[inline]
    pub fn with_main_align(mut self, align: Align) -> Self {
        self.main_align = align;
        self
    }

    #[inline]
    pub fn with_cross_align(mut self, align: Align) -> Self {
        self.cross_align = align;
        self
    }

    #[inline]
    pub fn with_main_justify(mut self, justify: bool) -> Self {
        self.main_justify = justify;
        self
    }

    #[inline]
    pub fn with_cross_justify(mut self, justify: bool) -> Self {
        self.cross_justify = justify;
        self
    }
}

/// Inspectors
impl Layout {
    #[inline]
    pub fn main_dir(&self) -> Direction {
        self.main_dir
    }

    #[inline]
    pub fn is_horizontal(&self) -> bool {
        self.main_dir.is_horizontal()
    }

    #[inline]
    pub fn is_vertical(&self) -> bool {
        self.main_dir.is_vertical()
    }

    #[inline]
    pub fn horizontal_align(&self) -> Align {
        if self.is_horizontal() {
            self.main_align
        } else {
            self.cross_align
        }
    }

    #[inline]
    pub fn vertical_align(&self) -> Align {
        if self.is_vertical() {
            self.main_align
        } else {
            self.cross_align
        }
    }

    pub fn horizontal_justify(&self) -> bool {
        if self.is_horizontal() {
            self.main_justify
        } else {
            self.cross_justify
        }
    }

    pub fn vertical_justify(&self) -> bool {
        if self.is_vertical() {
            self.main_justify
        } else {
            self.cross_justify
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Spacing — configurable item and window padding
// ═══════════════════════════════════════════════════════════════════════

/// Controls spacing between widgets, padding inside containers, etc.
#[derive(Clone, Copy, Debug)]
pub struct Spacing {
    /// Horizontal, vertical gap between widgets.
    pub item_spacing_x: i32,
    pub item_spacing_y: i32,
    /// Padding inside windows / panels.
    pub window_padding_x: i32,
    pub window_padding_y: i32,
    /// Padding inside buttons.
    pub button_padding_x: i32,
    pub button_padding_y: i32,
    /// Indent amount for nested groups / collapsing headers.
    pub indent: i32,
    /// Default widget height (for buttons, single-line text inputs, etc.)
    pub interact_height: i32,
    /// Default slider width.
    pub slider_width: i32,
    /// Default combo box width.
    pub combo_width: i32,
    /// Scroll bar width.
    pub scroll_bar_width: i32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            item_spacing_x: 8,
            item_spacing_y: 4,
            window_padding_x: 8,
            window_padding_y: 8,
            button_padding_x: 12,
            button_padding_y: 4,
            indent: 20,
            interact_height: 24,
            slider_width: 200,
            combo_width: 160,
            scroll_bar_width: 8,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Region — tracks cursor, min_rect, max_rect during layout
// ═══════════════════════════════════════════════════════════════════════

/// The state of a layout region: where widgets have been placed,
/// what space is available, and where the next widget goes.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    /// Bounding box of all widgets placed so far.
    pub min_rect: Rect,
    /// Maximum available area (soft limit).
    pub max_rect: Rect,
    /// Where the next widget will be placed.
    /// Advances as widgets are added.
    pub cursor_x: i32,
    pub cursor_y: i32,
    /// For wrapping layouts: track the cross-axis extent of the current row/column.
    pub row_max_cross: i32,
}

impl Region {
    /// Create a new region from a max rect, with cursor at the appropriate starting position.
    pub fn from_max_rect(layout: &Layout, max_rect: Rect) -> Self {
        let (cx, cy) = match layout.main_dir {
            Direction::LeftToRight => (max_rect.x, max_rect.y),
            Direction::RightToLeft => (max_rect.x + max_rect.width as i32, max_rect.y),
            Direction::TopDown => (max_rect.x, max_rect.y),
            Direction::BottomUp => (max_rect.x, max_rect.y + max_rect.height as i32),
        };
        Region {
            min_rect: Rect::new(cx, cy, 0, 0),
            max_rect,
            cursor_x: cx,
            cursor_y: cy,
            row_max_cross: 0,
        }
    }

    /// Expand `min_rect` to include the given rect.
    pub fn expand_to_include(&mut self, rect: Rect) {
        if self.min_rect.width == 0 && self.min_rect.height == 0 {
            self.min_rect = rect;
        } else {
            let x0 = self.min_rect.x.min(rect.x);
            let y0 = self.min_rect.y.min(rect.y);
            let x1 = (self.min_rect.x + self.min_rect.width as i32).max(rect.x + rect.width as i32);
            let y1 =
                (self.min_rect.y + self.min_rect.height as i32).max(rect.y + rect.height as i32);
            self.min_rect = Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
        }
    }

    /// Available width remaining.
    pub fn available_width(&self, layout: &Layout) -> i32 {
        match layout.main_dir {
            Direction::LeftToRight => {
                (self.max_rect.x + self.max_rect.width as i32) - self.cursor_x
            }
            Direction::RightToLeft => self.cursor_x - self.max_rect.x,
            Direction::TopDown | Direction::BottomUp => self.max_rect.width as i32,
        }
    }

    /// Available height remaining.
    pub fn available_height(&self, layout: &Layout) -> i32 {
        match layout.main_dir {
            Direction::TopDown => (self.max_rect.y + self.max_rect.height as i32) - self.cursor_y,
            Direction::BottomUp => self.cursor_y - self.max_rect.y,
            Direction::LeftToRight | Direction::RightToLeft => self.max_rect.height as i32,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Layout engine functions
// ═══════════════════════════════════════════════════════════════════════

impl Layout {
    /// Allocate a rectangle for a widget of the given size.
    /// Returns the widget rect and advances the cursor.
    pub fn allocate(
        &self,
        region: &mut Region,
        desired_width: u32,
        desired_height: u32,
        spacing: &Spacing,
    ) -> Rect {
        let avail_w = region.available_width(self).max(0) as u32;
        let avail_h = region.available_height(self).max(0) as u32;

        // Apply cross-axis justification: stretch widget to fill available cross-axis
        let (w, h) = match self.main_dir {
            Direction::LeftToRight | Direction::RightToLeft => {
                let w = desired_width;
                let h = if self.cross_justify {
                    desired_height.max(avail_h)
                } else {
                    desired_height
                };
                (w, h)
            }
            Direction::TopDown | Direction::BottomUp => {
                let w = if self.cross_justify {
                    desired_width.max(avail_w)
                } else {
                    desired_width
                };
                let h = desired_height;
                (w, h)
            }
        };

        // Handle wrapping
        if self.main_wrap {
            match self.main_dir {
                Direction::LeftToRight
                    if region.cursor_x + w as i32
                        > region.max_rect.x + region.max_rect.width as i32
                        && region.cursor_x > region.max_rect.x =>
                {
                    // Wrap to next row
                    region.cursor_x = region.max_rect.x;
                    region.cursor_y += region.row_max_cross + spacing.item_spacing_y;
                    region.row_max_cross = 0;
                }
                Direction::TopDown
                    if region.cursor_y + h as i32
                        > region.max_rect.y + region.max_rect.height as i32
                        && region.cursor_y > region.max_rect.y =>
                {
                    // Wrap to next column
                    region.cursor_y = region.max_rect.y;
                    region.cursor_x += region.row_max_cross + spacing.item_spacing_x;
                    region.row_max_cross = 0;
                }
                _ => {} // RightToLeft / BottomUp wrapping is rare
            }
        }

        // Compute the cross-axis alignment offset
        let rect = match self.main_dir {
            Direction::LeftToRight => {
                let x = region.cursor_x;
                let y = match self.cross_align {
                    Align::Min => region.max_rect.y,
                    Align::Center => {
                        region.max_rect.y + (region.max_rect.height as i32 - h as i32) / 2
                    }
                    Align::Max => region.max_rect.y + region.max_rect.height as i32 - h as i32,
                };
                // In horizontal layout, use cursor_y for stacked rows
                let y = if self.main_wrap || !self.cross_justify {
                    region.cursor_y
                } else {
                    y
                };
                Rect::new(x, y, w, h)
            }
            Direction::RightToLeft => {
                let x = region.cursor_x - w as i32;
                let y = region.cursor_y;
                Rect::new(x, y, w, h)
            }
            Direction::TopDown => {
                let x = match self.cross_align {
                    Align::Min => region.max_rect.x,
                    Align::Center => {
                        region.max_rect.x + (region.max_rect.width as i32 - w as i32) / 2
                    }
                    Align::Max => region.max_rect.x + region.max_rect.width as i32 - w as i32,
                };
                let x = if self.cross_justify {
                    region.max_rect.x
                } else {
                    x
                };
                let y = region.cursor_y;
                Rect::new(x, y, w, h)
            }
            Direction::BottomUp => {
                let x = match self.cross_align {
                    Align::Min => region.max_rect.x,
                    Align::Center => {
                        region.max_rect.x + (region.max_rect.width as i32 - w as i32) / 2
                    }
                    Align::Max => region.max_rect.x + region.max_rect.width as i32 - w as i32,
                };
                let y = region.cursor_y - h as i32;
                Rect::new(x, y, w, h)
            }
        };

        // Advance cursor
        match self.main_dir {
            Direction::LeftToRight => {
                region.cursor_x = rect.x + rect.width as i32 + spacing.item_spacing_x;
                region.row_max_cross = region.row_max_cross.max(h as i32);
            }
            Direction::RightToLeft => {
                region.cursor_x = rect.x - spacing.item_spacing_x;
                region.row_max_cross = region.row_max_cross.max(h as i32);
            }
            Direction::TopDown => {
                region.cursor_y = rect.y + rect.height as i32 + spacing.item_spacing_y;
                region.row_max_cross = region.row_max_cross.max(w as i32);
            }
            Direction::BottomUp => {
                region.cursor_y = rect.y - spacing.item_spacing_y;
                region.row_max_cross = region.row_max_cross.max(w as i32);
            }
        }

        // Expand min_rect to include this widget
        region.expand_to_include(rect);

        rect
    }

    /// Add empty space (advance cursor without placing a widget).
    pub fn add_space(&self, region: &mut Region, amount: i32) {
        match self.main_dir {
            Direction::LeftToRight => region.cursor_x += amount,
            Direction::RightToLeft => region.cursor_x -= amount,
            Direction::TopDown => region.cursor_y += amount,
            Direction::BottomUp => region.cursor_y -= amount,
        }
    }
}

/// Align `inner_size` within `outer` rect according to h/v alignment.
pub fn align_rect_in(
    outer: Rect,
    inner_w: u32,
    inner_h: u32,
    halign: Align,
    valign: Align,
) -> Rect {
    let x = match halign {
        Align::Min => outer.x,
        Align::Center => outer.x + (outer.width as i32 - inner_w as i32) / 2,
        Align::Max => outer.x + outer.width as i32 - inner_w as i32,
    };
    let y = match valign {
        Align::Min => outer.y,
        Align::Center => outer.y + (outer.height as i32 - inner_h as i32) / 2,
        Align::Max => outer.y + outer.height as i32 - inner_h as i32,
    };
    Rect::new(x, y, inner_w, inner_h)
}
