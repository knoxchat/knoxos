/// Panel — Docked side/top/bottom/central panels (inspired by egui::SidePanel, CentralPanel)
///
/// Panels carve out a portion of the available UI space. They're the primary
/// way to structure a desktop application layout:
///
/// ```ignore
/// SidePanel::left("nav", 220).show(ui, |ui| {
///     ui.heading("Navigation");
///     // ...
/// });
/// TopBottomPanel::top("toolbar", 40).show(ui, |ui| {
///     ui.horizontal(|ui| { ui.button("File"); });
/// });
/// CentralPanel::new("main").show(ui, |ui| {
///     ui.label("Main content area");
/// });
/// ```
use alloc::string::String;

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::{InputState, Style, UI_MEMORY, Ui, UiMemory};

/// Which side a side panel is docked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

/// Which edge a top/bottom panel is docked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopBottomSide {
    Top,
    Bottom,
}

// ═══════════════════════════════════════════════════════════════════════
// SidePanel — left or right docked panel
// ═══════════════════════════════════════════════════════════════════════

/// A panel docked to the left or right edge of the UI.
///
/// Takes a fixed width and paints a background + optional resize handle.
pub struct SidePanel {
    id: Id,
    side: PanelSide,
    default_width: u32,
    min_width: u32,
    max_width: u32,
    resizable: bool,
    show_separator: bool,
    bg_color: Option<Pixel>,
}

impl SidePanel {
    /// Create a left-docked side panel.
    pub fn left(id_salt: &str, default_width: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            side: PanelSide::Left,
            default_width,
            min_width: 60,
            max_width: 600,
            resizable: true,
            show_separator: true,
            bg_color: None,
        }
    }

    /// Create a right-docked side panel.
    pub fn right(id_salt: &str, default_width: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            side: PanelSide::Right,
            default_width,
            min_width: 60,
            max_width: 600,
            resizable: true,
            show_separator: true,
            bg_color: None,
        }
    }

    pub fn min_width(mut self, min: u32) -> Self {
        self.min_width = min;
        self
    }

    pub fn max_width(mut self, max: u32) -> Self {
        self.max_width = max;
        self
    }

    pub fn resizable(mut self, v: bool) -> Self {
        self.resizable = v;
        self
    }

    pub fn show_separator(mut self, v: bool) -> Self {
        self.show_separator = v;
        self
    }

    pub fn bg_color(mut self, c: Pixel) -> Self {
        self.bg_color = Some(c);
        self
    }

    /// Show the panel. The closure receives a `Ui` constrained to the panel area.
    /// Returns the remaining rect that can be used by subsequent panels / central.
    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> SidePanelResponse<R> {
        let outer = ui.max_rect();
        let style = ui.style().clone();

        // Retrieve persisted width or use default
        let mut width = {
            let mem = UI_MEMORY.lock();
            mem.get_i32(self.id, self.default_width as i32)
        } as u32;
        width = width.clamp(self.min_width, self.max_width);

        // Compute panel rect
        let panel_rect = match self.side {
            PanelSide::Left => Rect::new(outer.x, outer.y, width, outer.height),
            PanelSide::Right => Rect::new(
                outer.x + outer.width as i32 - width as i32,
                outer.y,
                width,
                outer.height,
            ),
        };

        // Draw panel background
        let bg = self.bg_color.unwrap_or(style.panel_bg);
        ui.fb.fill_rect(panel_rect, bg);

        // Separator line
        if self.show_separator {
            let sep_color = style.separator_color;
            match self.side {
                PanelSide::Left => {
                    ui.fb.draw_vline(
                        panel_rect.x + panel_rect.width as i32,
                        panel_rect.y,
                        panel_rect.height,
                        sep_color,
                    );
                }
                PanelSide::Right => {
                    ui.fb
                        .draw_vline(panel_rect.x, panel_rect.y, panel_rect.height, sep_color);
                }
            }
        }

        // Resize handle interaction
        if self.resizable {
            let handle_x = match self.side {
                PanelSide::Left => panel_rect.x + panel_rect.width as i32 - 3,
                PanelSide::Right => panel_rect.x,
            };
            let handle_rect = Rect::new(handle_x, panel_rect.y, 6, panel_rect.height);
            let resize_id = self.id.with("resize");
            let resp = ui.interact(handle_rect, resize_id, false, true);

            if resp.dragged {
                let new_w = match self.side {
                    PanelSide::Left => (ui.input.pointer_x - outer.x)
                        .clamp(self.min_width as i32, self.max_width as i32)
                        as u32,
                    PanelSide::Right => (outer.x + outer.width as i32 - ui.input.pointer_x)
                        .clamp(self.min_width as i32, self.max_width as i32)
                        as u32,
                };
                let mut mem = UI_MEMORY.lock();
                mem.set_i32(self.id, new_w as i32);
            }

            // Draw resize handle highlight on hover/drag
            if resp.hovered || resp.dragged {
                ui.fb.fill_rect(
                    Rect::new(handle_x + 2, panel_rect.y, 2, panel_rect.height),
                    Pixel::new(style.accent.r, style.accent.g, style.accent.b, 120),
                );
            }
        }

        // Create child Ui inside the panel
        let content_rect = Rect::new(
            panel_rect.x + style.spacing.window_padding_x,
            panel_rect.y + style.spacing.window_padding_y,
            (panel_rect.width as i32 - style.spacing.window_padding_x * 2).max(0) as u32,
            (panel_rect.height as i32 - style.spacing.window_padding_y * 2).max(0) as u32,
        );

        let saved_region = ui.region;
        let saved_layout = ui.layout;
        let saved_clip = ui.clip_rect;
        let saved_id = ui.id;

        ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
        ui.layout = Layout::top_down(Align::LEFT);
        ui.clip_rect = panel_rect;
        ui.id = self.id;
        ui.fb.push_clip(panel_rect);

        let inner = add_contents(ui);

        ui.fb.pop_clip();
        ui.region = saved_region;
        ui.layout = saved_layout;
        ui.clip_rect = saved_clip;
        ui.id = saved_id;

        // Compute the remaining rect
        let remaining = match self.side {
            PanelSide::Left => Rect::new(
                outer.x + width as i32 + 1,
                outer.y,
                (outer.width as i32 - width as i32 - 1).max(0) as u32,
                outer.height,
            ),
            PanelSide::Right => Rect::new(
                outer.x,
                outer.y,
                (outer.width as i32 - width as i32 - 1).max(0) as u32,
                outer.height,
            ),
        };

        // Shrink the parent's max_rect so the next panel/central uses the remaining space
        ui.region.max_rect = remaining;
        ui.region.cursor_x = remaining.x + style.spacing.window_padding_x;
        ui.region.cursor_y = remaining.y + style.spacing.window_padding_y;

        SidePanelResponse {
            inner,
            panel_rect,
            remaining_rect: remaining,
        }
    }
}

/// Result of showing a side panel.
pub struct SidePanelResponse<R> {
    pub inner: R,
    pub panel_rect: Rect,
    pub remaining_rect: Rect,
}

// ═══════════════════════════════════════════════════════════════════════
// TopBottomPanel — top or bottom docked panel
// ═══════════════════════════════════════════════════════════════════════

/// A panel docked to the top or bottom edge.
pub struct TopBottomPanel {
    id: Id,
    side: TopBottomSide,
    height: u32,
    resizable: bool,
    min_height: u32,
    max_height: u32,
    show_separator: bool,
    bg_color: Option<Pixel>,
}

impl TopBottomPanel {
    pub fn top(id_salt: &str, height: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            side: TopBottomSide::Top,
            height,
            resizable: false,
            min_height: 20,
            max_height: 400,
            show_separator: true,
            bg_color: None,
        }
    }

    pub fn bottom(id_salt: &str, height: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            side: TopBottomSide::Bottom,
            height,
            resizable: false,
            min_height: 20,
            max_height: 400,
            show_separator: true,
            bg_color: None,
        }
    }

    pub fn resizable(mut self, v: bool) -> Self {
        self.resizable = v;
        self
    }

    pub fn bg_color(mut self, c: Pixel) -> Self {
        self.bg_color = Some(c);
        self
    }

    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> TopBottomPanelResponse<R> {
        let outer = ui.max_rect();
        let style = ui.style().clone();

        let mut h = {
            let mem = UI_MEMORY.lock();
            mem.get_i32(self.id, self.height as i32)
        } as u32;
        h = h.clamp(self.min_height, self.max_height);

        let panel_rect = match self.side {
            TopBottomSide::Top => Rect::new(outer.x, outer.y, outer.width, h),
            TopBottomSide::Bottom => Rect::new(
                outer.x,
                outer.y + outer.height as i32 - h as i32,
                outer.width,
                h,
            ),
        };

        let bg = self.bg_color.unwrap_or(style.panel_bg);
        ui.fb.fill_rect(panel_rect, bg);

        if self.show_separator {
            match self.side {
                TopBottomSide::Top => {
                    ui.fb.draw_hline(
                        panel_rect.x,
                        panel_rect.y + panel_rect.height as i32,
                        panel_rect.width,
                        style.separator_color,
                    );
                }
                TopBottomSide::Bottom => {
                    ui.fb.draw_hline(
                        panel_rect.x,
                        panel_rect.y,
                        panel_rect.width,
                        style.separator_color,
                    );
                }
            }
        }

        // Child Ui
        let content_rect = Rect::new(
            panel_rect.x + style.spacing.window_padding_x,
            panel_rect.y + style.spacing.window_padding_y / 2,
            (panel_rect.width as i32 - style.spacing.window_padding_x * 2).max(0) as u32,
            (panel_rect.height as i32 - style.spacing.window_padding_y).max(0) as u32,
        );

        let saved_region = ui.region;
        let saved_layout = ui.layout;
        let saved_clip = ui.clip_rect;
        let saved_id = ui.id;

        ui.region = Region::from_max_rect(&Layout::left_to_right(Align::Center), content_rect);
        ui.layout = Layout::left_to_right(Align::Center);
        ui.clip_rect = panel_rect;
        ui.id = self.id;
        ui.fb.push_clip(panel_rect);

        let inner = add_contents(ui);

        ui.fb.pop_clip();
        ui.region = saved_region;
        ui.layout = saved_layout;
        ui.clip_rect = saved_clip;
        ui.id = saved_id;

        // Remaining rect
        let remaining = match self.side {
            TopBottomSide::Top => Rect::new(
                outer.x,
                outer.y + h as i32 + 1,
                outer.width,
                (outer.height as i32 - h as i32 - 1).max(0) as u32,
            ),
            TopBottomSide::Bottom => Rect::new(
                outer.x,
                outer.y,
                outer.width,
                (outer.height as i32 - h as i32 - 1).max(0) as u32,
            ),
        };

        ui.region.max_rect = remaining;
        ui.region.cursor_x = remaining.x + style.spacing.window_padding_x;
        ui.region.cursor_y = remaining.y + style.spacing.window_padding_y;

        TopBottomPanelResponse {
            inner,
            panel_rect,
            remaining_rect: remaining,
        }
    }
}

pub struct TopBottomPanelResponse<R> {
    pub inner: R,
    pub panel_rect: Rect,
    pub remaining_rect: Rect,
}

// ═══════════════════════════════════════════════════════════════════════
// CentralPanel — fills the remaining space
// ═══════════════════════════════════════════════════════════════════════

/// A panel that fills all remaining space after side/top/bottom panels.
/// Should always be added last.
pub struct CentralPanel {
    id: Id,
    bg_color: Option<Pixel>,
}

impl CentralPanel {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            bg_color: None,
        }
    }

    pub fn bg_color(mut self, c: Pixel) -> Self {
        self.bg_color = Some(c);
        self
    }

    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let panel_rect = ui.max_rect();
        let style = ui.style().clone();

        if let Some(bg) = self.bg_color {
            ui.fb.fill_rect(panel_rect, bg);
        }

        let content_rect = Rect::new(
            panel_rect.x + style.spacing.window_padding_x,
            panel_rect.y + style.spacing.window_padding_y,
            (panel_rect.width as i32 - style.spacing.window_padding_x * 2).max(0) as u32,
            (panel_rect.height as i32 - style.spacing.window_padding_y * 2).max(0) as u32,
        );

        let saved_region = ui.region;
        let saved_layout = ui.layout;
        let saved_clip = ui.clip_rect;
        let saved_id = ui.id;

        ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
        ui.layout = Layout::top_down(Align::LEFT);
        ui.clip_rect = panel_rect;
        ui.id = self.id;
        ui.fb.push_clip(panel_rect);

        let inner = add_contents(ui);

        ui.fb.pop_clip();
        ui.region = saved_region;
        ui.layout = saved_layout;
        ui.clip_rect = saved_clip;
        ui.id = saved_id;

        let resp = Response::none(self.id, panel_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }
}
