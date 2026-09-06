// ─── Strip — Dynamic strip layout builder ────────────────────────────
//
// Inspired by egui_extras::StripBuilder. Creates a row/column strip
// where each cell has a pre-determined size (exact, remainder, etc.).
// Unlike Grid, Strip cells have explicit sizing rather than auto-layout.

use alloc::vec::Vec;

use crate::gui::framebuffer::Rect;
use crate::gui::ui::Ui;

/// Sizing mode for a strip cell.
#[derive(Clone, Copy, Debug)]
pub enum StripSize {
    /// Exact pixel size.
    Exact(u32),
    /// Relative fraction of remaining space (0.0 - 1.0).
    Relative(u32, u32), // numerator, denominator (to avoid f32)
    /// Take all remaining space.
    Remainder,
}

impl StripSize {
    pub fn exact(px: u32) -> Self {
        Self::Exact(px)
    }

    pub fn remainder() -> Self {
        Self::Remainder
    }

    /// Relative size, e.g. `relative(1, 3)` = 1/3 of remaining.
    pub fn relative(num: u32, den: u32) -> Self {
        Self::Relative(num, den)
    }
}

/// Builder for creating a strip layout.
///
/// ```ignore
/// StripBuilder::new(ui)
///     .size(StripSize::exact(200))
///     .size(StripSize::remainder())
///     .horizontal(|mut strip| {
///         strip.cell(|ui| { ui.label("Sidebar"); });
///         strip.cell(|ui| { ui.label("Main content"); });
///     });
/// ```
pub struct StripBuilder<'a, 'b> {
    ui: &'a mut Ui<'b>,
    sizes: Vec<StripSize>,
}

impl<'a, 'b> StripBuilder<'a, 'b> {
    pub fn new(ui: &'a mut Ui<'b>) -> Self {
        Self {
            ui,
            sizes: Vec::new(),
        }
    }

    pub fn size(mut self, size: StripSize) -> Self {
        self.sizes.push(size);
        self
    }

    pub fn sizes(mut self, size: StripSize, count: usize) -> Self {
        for _ in 0..count {
            self.sizes.push(size);
        }
        self
    }

    /// Lay out cells horizontally (left to right).
    pub fn horizontal(self, add_contents: impl FnOnce(&mut Strip)) {
        let avail_w = self.ui.available_width().max(0) as u32;
        let avail_h = self.ui.available_height().max(0) as u32;
        let widths = resolve_sizes(&self.sizes, avail_w);
        let start_x = self.ui.region.cursor_x;
        let start_y = self.ui.region.cursor_y;

        let rects: Vec<Rect> = widths
            .iter()
            .scan(start_x, |x, &w| {
                let rect = Rect::new(*x, start_y, w, avail_h);
                *x += w as i32;
                Some(rect)
            })
            .collect();

        let mut strip = Strip {
            ui: self.ui,
            rects,
            index: 0,
        };
        add_contents(&mut strip);

        // Advance past the strip
        strip.ui.region.cursor_y = start_y + avail_h as i32;
    }

    /// Lay out cells vertically (top to bottom).
    pub fn vertical(self, add_contents: impl FnOnce(&mut Strip)) {
        let avail_w = self.ui.available_width().max(0) as u32;
        let avail_h = self.ui.available_height().max(0) as u32;
        let heights = resolve_sizes(&self.sizes, avail_h);
        let start_x = self.ui.region.cursor_x;
        let start_y = self.ui.region.cursor_y;

        let rects: Vec<Rect> = heights
            .iter()
            .scan(start_y, |y, &h| {
                let rect = Rect::new(start_x, *y, avail_w, h);
                *y += h as i32;
                Some(rect)
            })
            .collect();

        let mut strip = Strip {
            ui: self.ui,
            rects,
            index: 0,
        };
        add_contents(&mut strip);

        let total_h: u32 = heights.iter().sum();
        strip.ui.region.cursor_y = start_y + total_h as i32;
    }
}

/// A laid-out strip. Add cells sequentially with `.cell()`.
pub struct Strip<'a, 'b> {
    pub ui: &'a mut Ui<'b>,
    rects: Vec<Rect>,
    index: usize,
}

impl<'a, 'b> Strip<'a, 'b> {
    /// Add a cell to the strip. The closure receives the Ui positioned within the cell rect.
    pub fn cell(&mut self, add_contents: impl FnOnce(&mut Ui)) {
        if self.index >= self.rects.len() {
            return;
        }
        let rect = self.rects[self.index];
        self.index += 1;

        // Set up the UI region for this cell
        let old_cursor_x = self.ui.region.cursor_x;
        let old_cursor_y = self.ui.region.cursor_y;
        let old_max_rect = self.ui.region.max_rect;

        self.ui.region.cursor_x = rect.x;
        self.ui.region.cursor_y = rect.y;
        self.ui.region.max_rect = rect;

        self.ui.fb.push_clip(rect);
        add_contents(self.ui);
        self.ui.fb.pop_clip();

        self.ui.region.cursor_x = old_cursor_x;
        self.ui.region.cursor_y = old_cursor_y;
        self.ui.region.max_rect = old_max_rect;
    }

    /// Returns how many cells remain.
    pub fn remaining(&self) -> usize {
        self.rects.len().saturating_sub(self.index)
    }
}

/// Resolve abstract sizes into concrete pixel widths/heights.
fn resolve_sizes(sizes: &[StripSize], available: u32) -> Vec<u32> {
    let mut results = Vec::with_capacity(sizes.len());
    let mut used = 0u32;
    let mut remainder_count = 0u32;

    // First pass: allocate exact and relative sizes
    for size in sizes {
        match size {
            StripSize::Exact(px) => {
                results.push(*px);
                used += px;
            }
            StripSize::Relative(num, den) => {
                let px = if *den > 0 { available * num / den } else { 0 };
                results.push(px);
                used += px;
            }
            StripSize::Remainder => {
                results.push(0); // placeholder
                remainder_count += 1;
            }
        }
    }

    // Second pass: distribute remaining space
    let remaining = available.saturating_sub(used);
    let per_remainder = remaining.checked_div(remainder_count).unwrap_or(0);
    for (i, size) in sizes.iter().enumerate() {
        if matches!(size, StripSize::Remainder) {
            results[i] = per_remainder;
        }
    }

    results
}
