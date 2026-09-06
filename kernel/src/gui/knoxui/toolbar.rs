/// Toolbar — Horizontal toolbar with icon buttons, separators, and toggles
///
/// ```ignore
/// Toolbar::new("editor_toolbar").show(ui, |tb| {
///     if tb.icon_button("N", "New File").clicked { /* ... */ }
///     if tb.icon_button("O", "Open").clicked { /* ... */ }
///     tb.separator();
///     if tb.icon_button("S", "Save").clicked { /* ... */ }
///     tb.toggle("B", "Bold", &mut bold);
/// });
/// ```
use alloc::string::String;
use alloc::vec::Vec;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;

/// Toolbar item types.
pub enum ToolbarItem {
    Button {
        icon: String,
        tooltip: String,
    },
    Toggle {
        icon: String,
        tooltip: String,
        active: bool,
    },
    Separator,
    Spacer,
}

/// Toolbar widget.
pub struct Toolbar {
    id: Id,
    height: u32,
    button_size: u32,
    bg_color: Pixel,
}

impl Toolbar {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            height: 32,
            button_size: 28,
            bg_color: Pixel::rgb(22, 24, 30),
        }
    }

    pub fn height(mut self, h: u32) -> Self {
        self.height = h;
        self
    }
    pub fn button_size(mut self, s: u32) -> Self {
        self.button_size = s;
        self
    }

    pub fn show<'a>(self, ui: &mut Ui<'a>, add_items: impl FnOnce(&mut ToolbarContext)) {
        let avail_w = ui.available_width().max(0) as u32;
        let bar_rect = ui.allocate_space(avail_w, self.height);

        ui.fb.fill_rect(bar_rect, self.bg_color);
        ui.fb.draw_hline(
            bar_rect.x,
            bar_rect.y + self.height as i32,
            avail_w,
            Pixel::rgb(40, 44, 52),
        );

        let mut ctx = ToolbarContext {
            ui,
            bar_id: self.id,
            x: bar_rect.x + 4,
            y: bar_rect.y,
            height: self.height,
            button_size: self.button_size,
            avail_w,
        };

        add_items(&mut ctx);
    }
}

/// Context passed to the toolbar builder closure.
pub struct ToolbarContext<'a, 'b> {
    ui: &'b mut Ui<'a>,
    bar_id: Id,
    x: i32,
    y: i32,
    height: u32,
    button_size: u32,
    avail_w: u32,
}

/// Result of a toolbar button click.
pub struct ToolbarButtonResult {
    pub clicked: bool,
    pub hovered: bool,
}

impl<'a, 'b> ToolbarContext<'a, 'b> {
    /// Add an icon button (single character as icon).
    pub fn icon_button(&mut self, icon: &str, _tooltip: &str) -> ToolbarButtonResult {
        let btn_id = self.bar_id.with(icon);
        let btn_y = self.y + (self.height as i32 - self.button_size as i32) / 2;
        let btn_rect = Rect::new(self.x, btn_y, self.button_size, self.button_size);

        let resp = self.ui.interact(btn_rect, btn_id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            Pixel::rgb(50, 55, 65)
        } else if resp.hovered {
            Pixel::rgb(40, 44, 55)
        } else {
            Pixel::new(0, 0, 0, 0)
        };
        if bg.a > 0 {
            self.ui.fb.fill_rounded_rect_aa(btn_rect, bg, 4);
        }

        fonts::draw_string_centered_in_rect(
            self.ui.fb,
            self.x,
            btn_y,
            self.button_size,
            self.button_size,
            icon,
            colors::WHITE,
        );

        self.x += self.button_size as i32 + 2;

        ToolbarButtonResult {
            clicked: resp.clicked(),
            hovered: resp.hovered(),
        }
    }

    /// Add a toggle button (highlighted when active).
    pub fn toggle(&mut self, icon: &str, _tooltip: &str, active: &mut bool) -> ToolbarButtonResult {
        let btn_id = self.bar_id.with(icon).with("toggle");
        let btn_y = self.y + (self.height as i32 - self.button_size as i32) / 2;
        let btn_rect = Rect::new(self.x, btn_y, self.button_size, self.button_size);

        let resp = self.ui.interact(btn_rect, btn_id, true, false);

        if resp.clicked() {
            *active = !*active;
        }

        let bg = if *active {
            self.ui.style().accent
        } else if resp.hovered {
            Pixel::rgb(40, 44, 55)
        } else {
            Pixel::new(0, 0, 0, 0)
        };
        if bg.a > 0 {
            self.ui.fb.fill_rounded_rect_aa(btn_rect, bg, 4);
        }

        fonts::draw_string_centered_in_rect(
            self.ui.fb,
            self.x,
            btn_y,
            self.button_size,
            self.button_size,
            icon,
            colors::WHITE,
        );

        self.x += self.button_size as i32 + 2;

        ToolbarButtonResult {
            clicked: resp.clicked(),
            hovered: resp.hovered(),
        }
    }

    /// Add a vertical separator.
    pub fn separator(&mut self) {
        let sep_y = self.y + 4;
        let sep_h = self.height - 8;
        self.ui
            .fb
            .draw_vline(self.x + 4, sep_y, sep_h, Pixel::rgb(55, 60, 70));
        self.x += 10;
    }

    /// Add flexible space (pushes remaining items to the right).
    pub fn spacer(&mut self) {
        // Calculate remaining space and move x to the right
        let remaining = self.avail_w as i32 - (self.x - self.ui.region.max_rect.x);
        self.x += remaining / 2;
    }
}
