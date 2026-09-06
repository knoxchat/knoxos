/// Immediate-Mode UI Context — The heart of the egui-inspired UI layer.
///
/// `Ui` is what you use to place widgets. It manages layout, input interaction,
/// and drawing. Inspired by egui's `Ui`, adapted for `no_std` bare-metal with
/// integer coordinates and direct framebuffer rendering.
///
/// # Usage pattern (inside a window content draw):
/// ```ignore
/// let mut ui = Ui::new(fb, content_rect, &input_state);
/// ui.heading("Settings");
/// ui.horizontal(|ui| {
///     ui.label("Name:");
///     if ui.button("Click").clicked() { /* ... */ }
/// });
/// ui.add_space(8);
/// ui.slider_u8("Volume", &mut volume, 0, 100);
/// ui.checkbox("Enable", &mut flag);
/// ```
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::id::Id;
use super::layout::{Align, Direction, Layout, Region, Spacing};
use super::response::{InnerResponse, Response};

// ═══════════════════════════════════════════════════════════════════════
// Input snapshot — captured once per frame, read by all widgets
// ═══════════════════════════════════════════════════════════════════════

/// A snapshot of input state for the current frame.
/// Captured from the kernel's input module at the start of a frame.
#[derive(Clone, Debug)]
pub struct InputState {
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub pointer_primary_down: bool,
    pub pointer_secondary_down: bool,
    /// Was the primary button just pressed this frame (was up last frame)?
    pub pointer_primary_pressed: bool,
    /// Was the primary button just released this frame?
    pub pointer_primary_released: bool,
    /// Was the secondary button just pressed?
    pub pointer_secondary_pressed: bool,
    /// Was the secondary button just released?
    pub pointer_secondary_released: bool,
    /// Scroll wheel delta (positive = up/away from user)
    pub scroll_delta: i32,
    /// The last character typed (if any).
    pub char_typed: Option<char>,
    /// Whether backspace was pressed.
    pub backspace_pressed: bool,
    /// Whether Enter was pressed.
    pub enter_pressed: bool,
    /// Whether Tab was pressed.
    pub tab_pressed: bool,
    /// Whether Escape was pressed.
    pub escape_pressed: bool,
    /// Whether the left arrow key was pressed.
    pub left_pressed: bool,
    /// Whether the right arrow key was pressed.
    pub right_pressed: bool,
    /// Whether the up arrow key was pressed.
    pub up_pressed: bool,
    /// Whether the down arrow key was pressed.
    pub down_pressed: bool,
    /// Whether Home was pressed.
    pub home_pressed: bool,
    /// Whether End was pressed.
    pub end_pressed: bool,
    /// Whether Delete was pressed.
    pub delete_pressed: bool,
    /// Frame tick counter (for animations).
    pub frame_tick: u64,
    /// Double-click detected.
    pub double_click: bool,
}

impl InputState {
    /// Create a blank input state (no interaction).
    pub fn none() -> Self {
        Self {
            pointer_x: 0,
            pointer_y: 0,
            pointer_primary_down: false,
            pointer_secondary_down: false,
            pointer_primary_pressed: false,
            pointer_primary_released: false,
            pointer_secondary_pressed: false,
            pointer_secondary_released: false,
            scroll_delta: 0,
            char_typed: None,
            backspace_pressed: false,
            enter_pressed: false,
            tab_pressed: false,
            escape_pressed: false,
            left_pressed: false,
            right_pressed: false,
            up_pressed: false,
            down_pressed: false,
            home_pressed: false,
            end_pressed: false,
            delete_pressed: false,
            frame_tick: 0,
            double_click: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Persistent UI state — survives across frames
// ═══════════════════════════════════════════════════════════════════════

/// Per-widget persistent state entry.
#[derive(Clone, Debug)]
pub enum WidgetState {
    Bool(bool),
    I32(i32),
    U32(u32),
    String(String),
    ScrollOffset(i32, i32),
}

/// Persistent state store for all immediate-mode widgets.
/// Keyed by `Id`. Stored globally and persists across frames.
pub struct UiMemory {
    /// Widget states keyed by Id hash.
    entries: Vec<(u64, WidgetState)>,
    /// Which widget currently has keyboard focus.
    pub focused_id: Option<Id>,
    /// Focus last frame (for `gained_focus` / `lost_focus`).
    pub prev_focused_id: Option<Id>,
    /// Which widget was pressed on (for drag tracking).
    pub active_id: Option<Id>,
    /// Pointer position when active_id was pressed.
    pub active_start_x: i32,
    pub active_start_y: i32,
    /// Whether active_id has moved enough to count as a drag.
    pub active_is_dragging: bool,
    /// Pointer position last frame (for drag delta).
    pub prev_pointer_x: i32,
    pub prev_pointer_y: i32,
    /// Previous frame's primary button state (for press/release detection).
    pub prev_primary_down: bool,
    pub prev_secondary_down: bool,
}

impl UiMemory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            focused_id: None,
            prev_focused_id: None,
            active_id: None,
            active_start_x: 0,
            active_start_y: 0,
            active_is_dragging: false,
            prev_pointer_x: 0,
            prev_pointer_y: 0,
            prev_primary_down: false,
            prev_secondary_down: false,
        }
    }

    /// Get a state entry.
    pub fn get(&self, id: Id) -> Option<&WidgetState> {
        let key = id.value();
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// Set a state entry.
    pub fn set(&mut self, id: Id, state: WidgetState) {
        let key = id.value();
        if let Some(entry) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = state;
        } else {
            self.entries.push((key, state));
        }
    }

    /// Get a bool state, defaulting to `default` if not present.
    pub fn get_bool(&self, id: Id, default: bool) -> bool {
        match self.get(id) {
            Some(WidgetState::Bool(v)) => *v,
            _ => default,
        }
    }

    /// Set a bool state.
    pub fn set_bool(&mut self, id: Id, value: bool) {
        self.set(id, WidgetState::Bool(value));
    }

    /// Get an i32 state, defaulting to `default` if not present.
    pub fn get_i32(&self, id: Id, default: i32) -> i32 {
        match self.get(id) {
            Some(WidgetState::I32(v)) => *v,
            _ => default,
        }
    }

    /// Set an i32 state.
    pub fn set_i32(&mut self, id: Id, value: i32) {
        self.set(id, WidgetState::I32(value));
    }

    /// Get scroll offset for a scrollable region.
    pub fn get_scroll(&self, id: Id) -> (i32, i32) {
        match self.get(id) {
            Some(WidgetState::ScrollOffset(x, y)) => (*x, *y),
            _ => (0, 0),
        }
    }

    /// Set scroll offset.
    pub fn set_scroll(&mut self, id: Id, x: i32, y: i32) {
        self.set(id, WidgetState::ScrollOffset(x, y));
    }

    /// Get a String state.
    pub fn get_string(&self, id: Id) -> Option<&str> {
        match self.get(id) {
            Some(WidgetState::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Set a String state.
    pub fn set_string(&mut self, id: Id, value: String) {
        self.set(id, WidgetState::String(value));
    }

    /// Begin a new frame — update prev states.
    pub fn begin_frame(&mut self, input: &InputState) {
        self.prev_focused_id = self.focused_id;
        self.prev_pointer_x = input.pointer_x;
        self.prev_pointer_y = input.pointer_y;

        // Detect press/release transitions
        // (input already has these, so just track for drag logic)
        if !input.pointer_primary_down && self.prev_primary_down {
            // Released — clear active
            self.active_id = None;
            self.active_is_dragging = false;
        }
        self.prev_primary_down = input.pointer_primary_down;
        self.prev_secondary_down = input.pointer_secondary_down;
    }
}

lazy_static::lazy_static! {
    /// Global persistent UI memory — shared across all frames.
    pub static ref UI_MEMORY: spin::Mutex<UiMemory> = spin::Mutex::new(UiMemory::new());
}

// ═══════════════════════════════════════════════════════════════════════
// Style — visual appearance of widgets
// ═══════════════════════════════════════════════════════════════════════

/// Visual style for the immediate-mode UI.
#[derive(Clone, Debug)]
pub struct Style {
    pub spacing: Spacing,
    /// Background color for interactive widgets (buttons, etc.)
    pub widget_bg: Pixel,
    /// Hovered widget background.
    pub widget_bg_hovered: Pixel,
    /// Active/pressed widget background.
    pub widget_bg_active: Pixel,
    /// Primary accent color.
    pub accent: Pixel,
    /// Accent color, hovered.
    pub accent_hovered: Pixel,
    /// Text color.
    pub text_color: Pixel,
    /// Dimmed text.
    pub text_dimmed: Pixel,
    /// Disabled text.
    pub text_disabled: Pixel,
    /// Widget border color.
    pub border_color: Pixel,
    /// Focused widget border.
    pub border_focused: Pixel,
    /// Separator color.
    pub separator_color: Pixel,
    /// Panel / window background.
    pub panel_bg: Pixel,
    /// Selection / highlight color.
    pub selection_bg: Pixel,
    /// Corner radius for buttons and widgets.
    pub corner_radius: u32,
    /// Corner radius for windows/panels.
    pub window_corner_radius: u32,
    /// Font scale: 1 = compact (8×12), 2 = large
    pub font_scale: u32,
    /// Use bold text for headings.
    pub heading_bold: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            spacing: Spacing::default(),
            widget_bg: Pixel::rgb(45, 48, 55),
            widget_bg_hovered: Pixel::rgb(55, 60, 70),
            widget_bg_active: Pixel::rgb(35, 38, 45),
            accent: Pixel::rgb(82, 139, 255),
            accent_hovered: Pixel::rgb(100, 155, 255),
            text_color: colors::WHITE,
            text_dimmed: Pixel::rgb(160, 165, 175),
            text_disabled: Pixel::rgb(90, 95, 105),
            border_color: Pixel::rgb(65, 70, 80),
            border_focused: Pixel::rgb(82, 139, 255),
            separator_color: Pixel::rgb(50, 55, 65),
            panel_bg: Pixel::rgb(24, 26, 32),
            selection_bg: Pixel::rgb(40, 100, 220),
            corner_radius: 6,
            window_corner_radius: 10,
            font_scale: 1,
            heading_bold: true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Ui — the main immediate-mode context
// ═══════════════════════════════════════════════════════════════════════

/// The primary immediate-mode UI context.
///
/// You place widgets by calling methods like `label()`, `button()`, `slider()`.
/// Each method draws the widget, checks input, and returns a `Response`.
pub struct Ui<'a> {
    /// Reference to the framebuffer we draw into.
    pub fb: &'a mut FrameBuffer,
    /// Input state for this frame.
    pub input: &'a InputState,
    /// Layout configuration.
    pub layout: Layout,
    /// Current region (cursor, bounds).
    pub region: Region,
    /// Visual style.
    pub style: Style,
    /// Unique ID for this Ui scope.
    pub id: Id,
    /// Next auto-generated ID counter.
    next_auto_id: u64,
    /// Whether this Ui is enabled for interaction.
    pub enabled: bool,
    /// Clip rectangle — widgets outside this are not drawn or interacted with.
    pub clip_rect: Rect,
}

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

    // ── Allocation & interaction ─────────────────────────────────

    /// Allocate space for a widget of given size. Returns the assigned rect.
    pub fn allocate_space(&mut self, width: u32, height: u32) -> Rect {
        self.layout
            .allocate(&mut self.region, width, height, &self.style.spacing)
    }

    /// Check interaction (hover, click, drag) for a widget at `rect`.
    pub fn interact(&self, rect: Rect, id: Id, sense_click: bool, sense_drag: bool) -> Response {
        let mut resp = Response::none(id, rect);
        resp.enabled = self.enabled;

        if !self.enabled {
            return resp;
        }

        // Check if pointer is inside the widget rect AND the clip rect
        let px = self.input.pointer_x;
        let py = self.input.pointer_y;
        let in_rect = rect.contains(px, py);
        let in_clip = self.clip_rect.contains(px, py);
        let pointer_over = in_rect && in_clip;

        resp.hovered = pointer_over;

        let mem = UI_MEMORY.lock();
        let is_active = mem.active_id == Some(id);

        if sense_click {
            // Click detection: pointer released while hovering
            if pointer_over && self.input.pointer_primary_released && is_active {
                resp.clicked = true;
            }
            if pointer_over && self.input.double_click && self.input.pointer_primary_pressed {
                resp.double_clicked = true;
            }
            // Right-click
            if pointer_over && self.input.pointer_secondary_released {
                resp.secondary_clicked = true;
            }
        }

        if sense_drag {
            if is_active && self.input.pointer_primary_down {
                resp.is_pointer_button_down_on = true;
                if mem.active_is_dragging {
                    resp.dragged = true;
                    resp.drag_delta_x = self.input.pointer_x - mem.prev_pointer_x;
                    resp.drag_delta_y = self.input.pointer_y - mem.prev_pointer_y;
                }
            }
            if is_active && self.input.pointer_primary_released && mem.active_is_dragging {
                resp.drag_stopped = true;
            }
        }

        // Focus tracking
        if let Some(fid) = mem.focused_id {
            if fid == id {
                resp.has_focus = true;
                if mem.prev_focused_id != Some(id) {
                    resp.gained_focus = true;
                }
            }
        }
        if mem.prev_focused_id == Some(id) && mem.focused_id != Some(id) {
            resp.lost_focus = true;
        }

        drop(mem);

        // Update active_id on press
        if pointer_over && self.input.pointer_primary_pressed && (sense_click || sense_drag) {
            let mut mem = UI_MEMORY.lock();
            mem.active_id = Some(id);
            mem.active_start_x = px;
            mem.active_start_y = py;
            mem.active_is_dragging = false;
        }

        // Detect drag start (moved > 3px threshold)
        if self.input.pointer_primary_down {
            let mut mem = UI_MEMORY.lock();
            if mem.active_id == Some(id) && !mem.active_is_dragging && sense_drag {
                let dx = self.input.pointer_x - mem.active_start_x;
                let dy = self.input.pointer_y - mem.active_start_y;
                if dx * dx + dy * dy > 9 {
                    mem.active_is_dragging = true;
                    resp.drag_started = true;
                }
            }
        }

        resp
    }

    // ── Spacing & separators ─────────────────────────────────────

    /// Add empty vertical or horizontal space.
    pub fn add_space(&mut self, amount: i32) {
        self.layout.add_space(&mut self.region, amount);
    }

    /// Draw a horizontal separator line.
    pub fn separator(&mut self) -> Response {
        let id = self.auto_id();
        let w = self.available_width().max(0) as u32;
        let rect = self.allocate_space(w, 1);
        self.fb
            .draw_hline(rect.x, rect.y, rect.width, self.style.separator_color);
        self.add_space(self.style.spacing.item_spacing_y);
        Response::none(id, rect)
    }

    // ── Text widgets ─────────────────────────────────────────────

    /// Draw a label (non-interactive text).
    pub fn label(&mut self, text: &str) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8; // compact font: 8px wide
        let text_h = 12u32;
        let rect = self.allocate_space(text_w, text_h);
        fonts::draw_string_compact(self.fb, rect.x, rect.y, text, self.style.text_color, 1);
        self.interact(rect, id, false, false)
    }

    /// Draw colored label.
    pub fn colored_label(&mut self, text: &str, color: Pixel) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8;
        let text_h = 12u32;
        let rect = self.allocate_space(text_w, text_h);
        fonts::draw_string_compact(self.fb, rect.x, rect.y, text, color, 1);
        self.interact(rect, id, false, false)
    }

    /// Draw dimmed text.
    pub fn dimmed_label(&mut self, text: &str) -> Response {
        self.colored_label(text, self.style.text_dimmed)
    }

    /// Draw a heading (larger, bold text).
    pub fn heading(&mut self, text: &str) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8; // same width, just bold + spacing
        let text_h = 16u32;
        let rect = self.allocate_space(text_w, text_h + 4);
        if self.style.heading_bold {
            fonts::draw_string_bold(self.fb, rect.x, rect.y + 2, text, self.style.text_color, 1);
        } else {
            fonts::draw_string_compact(self.fb, rect.x, rect.y + 2, text, self.style.text_color, 1);
        }
        self.interact(rect, id, false, false)
    }

    /// Draw a small label (dimmed).
    pub fn small(&mut self, text: &str) -> Response {
        self.colored_label(text, self.style.text_dimmed)
    }

    /// Draw a monospace code label.
    pub fn code(&mut self, text: &str) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8 + 8;
        let text_h = 16u32;
        let rect = self.allocate_space(text_w, text_h);
        self.fb
            .fill_rounded_rect_aa(rect, Pixel::rgb(35, 38, 45), 3);
        fonts::draw_string_compact(
            self.fb,
            rect.x + 4,
            rect.y + 2,
            text,
            Pixel::rgb(220, 180, 120),
            1,
        );
        self.interact(rect, id, false, false)
    }

    // ── Button ───────────────────────────────────────────────────

    /// Draw a clickable button.
    pub fn button(&mut self, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let pad_x = self.style.spacing.button_padding_x as u32;
        let pad_y = self.style.spacing.button_padding_y as u32;
        let w = text_w + pad_x * 2;
        let h = 12 + pad_y * 2;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            self.style.widget_bg_active
        } else if resp.hovered {
            self.style.widget_bg_hovered
        } else {
            self.style.widget_bg
        };

        self.fb
            .fill_rounded_rect_aa(rect, bg, self.style.corner_radius);
        self.fb.draw_rounded_rect(
            rect,
            if resp.hovered {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            self.style.corner_radius,
            1,
        );
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            text,
            self.style.text_color,
            1,
        );

        resp
    }

    /// Draw a primary accent-colored button.
    pub fn primary_button(&mut self, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let pad_x = self.style.spacing.button_padding_x as u32;
        let pad_y = self.style.spacing.button_padding_y as u32;
        let w = text_w + pad_x * 2;
        let h = 12 + pad_y * 2;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            Pixel::rgb(50, 120, 220)
        } else if resp.hovered {
            self.style.accent_hovered
        } else {
            self.style.accent
        };

        self.fb
            .fill_rounded_rect_aa(rect, bg, self.style.corner_radius);
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            text,
            colors::WHITE,
            1,
        );

        resp
    }

    /// Draw a small button (less padding).
    pub fn small_button(&mut self, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let w = text_w + 8;
        let h = 16u32;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            self.style.widget_bg_active
        } else if resp.hovered {
            self.style.widget_bg_hovered
        } else {
            Pixel::new(0, 0, 0, 0) // transparent when idle
        };
        if bg.a > 0 {
            self.fb
                .fill_rounded_rect_aa(rect, bg, self.style.corner_radius);
        }
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            text,
            self.style.text_color,
            1,
        );

        resp
    }

    /// Draw a selectable label (like a button but styled as text, highlighted when selected).
    pub fn selectable_label(&mut self, selected: bool, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let w = text_w + 12;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        if selected || resp.hovered {
            let bg = if selected {
                self.style.selection_bg
            } else {
                self.style.widget_bg_hovered
            };
            self.fb.fill_rounded_rect_aa(rect, bg, 4);
        }

        let text_color = if selected {
            colors::WHITE
        } else if resp.hovered {
            self.style.text_color
        } else {
            self.style.text_dimmed
        };
        fonts::draw_string_compact(
            self.fb,
            rect.x + 6,
            rect.y + (rect.height as i32 - 12) / 2,
            text,
            text_color,
            1,
        );

        resp
    }

    /// Draw a selectable value: if clicked, set `current` to `value`.
    pub fn selectable_value<V: PartialEq + Copy>(
        &mut self,
        current: &mut V,
        value: V,
        text: &str,
    ) -> Response {
        let selected = *current == value;
        let resp = self.selectable_label(selected, text);
        if resp.clicked() {
            *current = value;
        }
        resp
    }

    // ── Checkbox ─────────────────────────────────────────────────

    /// Draw a checkbox with a label. Toggles `checked` on click.
    pub fn checkbox(&mut self, checked: &mut bool, text: &str) -> Response {
        let id = self.id_from(text);
        let box_size = 16u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = box_size + 8 + text_w;
        let h = box_size.max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            *checked = !*checked;
        }

        // Checkbox box
        let box_y = rect.y + (h as i32 - box_size as i32) / 2;
        let box_rect = Rect::new(rect.x, box_y, box_size, box_size);
        let box_bg = if *checked {
            self.style.accent
        } else if resp.hovered {
            self.style.widget_bg_hovered
        } else {
            self.style.widget_bg
        };
        self.fb.fill_rounded_rect_aa(box_rect, box_bg, 3);
        self.fb.draw_rounded_rect(
            box_rect,
            if *checked {
                self.style.accent
            } else {
                self.style.border_color
            },
            3,
            1,
        );
        if *checked {
            // Checkmark
            self.fb
                .draw_line_aa(rect.x + 3, box_y + 8, rect.x + 6, box_y + 12, colors::WHITE);
            self.fb.draw_line_aa(
                rect.x + 6,
                box_y + 12,
                rect.x + 12,
                box_y + 4,
                colors::WHITE,
            );
        }

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x + box_size as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.changed = r.clicked;
        r
    }

    // ── Radio button ─────────────────────────────────────────────

    /// Draw a radio button. Sets `current` to `value` on click.
    pub fn radio_value<V: PartialEq + Copy>(
        &mut self,
        current: &mut V,
        value: V,
        text: &str,
    ) -> Response {
        let selected = *current == value;
        let id = self.id_from(text);
        let circle_r = 8u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = circle_r * 2 + 8 + text_w;
        let h = (circle_r * 2).max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            *current = value;
        }

        // Outer circle
        let cx = rect.x + circle_r as i32;
        let cy = rect.y + h as i32 / 2;
        let border = if resp.hovered {
            self.style.accent_hovered
        } else if selected {
            self.style.accent
        } else {
            self.style.border_color
        };
        self.fb
            .fill_circle_aa(cx, cy, circle_r, self.style.widget_bg);
        // Draw border as a filled ring (outer - inner)
        self.fb.fill_circle_aa(cx, cy, circle_r, border);
        self.fb
            .fill_circle_aa(cx, cy, circle_r - 2, self.style.widget_bg);

        // Inner dot when selected
        if selected {
            self.fb.fill_circle_aa(cx, cy, 4, self.style.accent);
        }

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x + (circle_r * 2) as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.changed = r.clicked;
        r
    }

    /// Draw a radio button with bool state.
    pub fn radio(&mut self, selected: bool, text: &str) -> Response {
        let id = self.id_from(text);
        let circle_r = 8u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = circle_r * 2 + 8 + text_w;
        let h = (circle_r * 2).max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        let cx = rect.x + circle_r as i32;
        let cy = rect.y + h as i32 / 2;
        let border = if resp.hovered {
            self.style.accent_hovered
        } else if selected {
            self.style.accent
        } else {
            self.style.border_color
        };
        self.fb.fill_circle_aa(cx, cy, circle_r, border);
        self.fb
            .fill_circle_aa(cx, cy, circle_r - 2, self.style.widget_bg);
        if selected {
            self.fb.fill_circle_aa(cx, cy, 4, self.style.accent);
        }
        fonts::draw_string_compact(
            self.fb,
            rect.x + (circle_r * 2) as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        resp
    }

    // ── Slider ───────────────────────────────────────────────────

    /// Draw a horizontal slider for a u8 value in [min, max].
    pub fn slider_u8(&mut self, label: &str, value: &mut u8, min: u8, max: u8) -> Response {
        let id = self.id_from(label);
        let slider_w = self.style.spacing.slider_width as u32;
        let label_w = (label.len() as u32) * 8 + 8;
        let value_w = 32u32; // "255"
        let total_w = label_w + slider_w + 8 + value_w;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            label,
            self.style.text_color,
            1,
        );

        // Slider track
        let track_x = rect.x + label_w as i32;
        let track_y = rect.y + h as i32 / 2 - 3;
        let track_rect = Rect::new(track_x, track_y, slider_w, 6);
        self.fb
            .fill_rounded_rect_aa(track_rect, self.style.widget_bg, 3);

        // Fill portion
        let range = (max as i32 - min as i32).max(1);
        let fill_frac = (*value as i32 - min as i32) * slider_w as i32 / range;
        if fill_frac > 0 {
            self.fb.fill_rounded_rect_aa(
                Rect::new(track_x, track_y, fill_frac as u32, 6),
                self.style.accent,
                3,
            );
        }

        // Thumb
        let thumb_cx = track_x + fill_frac;
        let thumb_cy = rect.y + h as i32 / 2;

        // Interaction: drag the slider track area
        let interact_rect = Rect::new(track_x - 8, rect.y, slider_w + 16, h);
        let resp = self.interact(interact_rect, id, true, true);

        let thumb_color = if resp.dragged || resp.is_pointer_button_down_on {
            self.style.accent_hovered
        } else if resp.hovered {
            colors::WHITE
        } else {
            self.style.accent
        };

        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 8, thumb_color);
        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 5, colors::WHITE);

        // Update value if dragged or clicked
        let mut changed = false;
        if resp.is_pointer_button_down_on || resp.dragged {
            let relative = (self.input.pointer_x - track_x).clamp(0, slider_w as i32);
            let new_val = min as i32 + relative * range / slider_w as i32;
            let new_val = new_val.clamp(min as i32, max as i32) as u8;
            if new_val != *value {
                *value = new_val;
                changed = true;
            }
        }

        // Value text
        let val_str = alloc::format!("{}", *value);
        fonts::draw_string_compact(
            self.fb,
            track_x + slider_w as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            &val_str,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.rect = rect;
        r.changed = changed;
        r
    }

    /// Draw a slider for an i32 value in [min, max].
    pub fn slider_i32(&mut self, label: &str, value: &mut i32, min: i32, max: i32) -> Response {
        let id = self.id_from(label);
        let slider_w = self.style.spacing.slider_width as u32;
        let label_w = (label.len() as u32) * 8 + 8;
        let value_w = 48u32;
        let total_w = label_w + slider_w + 8 + value_w;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            label,
            self.style.text_color,
            1,
        );

        let track_x = rect.x + label_w as i32;
        let track_y = rect.y + h as i32 / 2 - 3;
        self.fb.fill_rounded_rect_aa(
            Rect::new(track_x, track_y, slider_w, 6),
            self.style.widget_bg,
            3,
        );

        let range = (max - min).max(1);
        let fill_frac = (*value - min) * slider_w as i32 / range;
        if fill_frac > 0 {
            self.fb.fill_rounded_rect_aa(
                Rect::new(track_x, track_y, fill_frac.max(0) as u32, 6),
                self.style.accent,
                3,
            );
        }

        let thumb_cx = track_x + fill_frac.clamp(0, slider_w as i32);
        let thumb_cy = rect.y + h as i32 / 2;

        let interact_rect = Rect::new(track_x - 8, rect.y, slider_w + 16, h);
        let resp = self.interact(interact_rect, id, true, true);

        let thumb_color = if resp.dragged || resp.is_pointer_button_down_on {
            self.style.accent_hovered
        } else if resp.hovered {
            colors::WHITE
        } else {
            self.style.accent
        };
        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 8, thumb_color);
        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 5, colors::WHITE);

        let mut changed = false;
        if resp.is_pointer_button_down_on || resp.dragged {
            let relative = (self.input.pointer_x - track_x).clamp(0, slider_w as i32);
            let new_val = min + relative * range / slider_w as i32;
            let new_val = new_val.clamp(min, max);
            if new_val != *value {
                *value = new_val;
                changed = true;
            }
        }

        let val_str = alloc::format!("{}", *value);
        fonts::draw_string_compact(
            self.fb,
            track_x + slider_w as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            &val_str,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.rect = rect;
        r.changed = changed;
        r
    }

    // ── Progress bar ─────────────────────────────────────────────

    /// Draw a progress bar (0-100).
    pub fn progress_bar(&mut self, progress: u8, text: Option<&str>) -> Response {
        let id = self.auto_id();
        let w = self.available_width().max(100) as u32;
        let h = 20u32;
        let rect = self.allocate_space(w, h);

        self.fb.fill_rounded_rect_aa(rect, self.style.widget_bg, 3);
        let fill_w = (rect.width * progress.min(100) as u32) / 100;
        if fill_w > 0 {
            self.fb.fill_rounded_rect_aa(
                Rect::new(rect.x, rect.y, fill_w, h),
                self.style.accent,
                3,
            );
        }

        let label = if let Some(t) = text {
            alloc::format!("{} {}%", t, progress)
        } else {
            alloc::format!("{}%", progress)
        };
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            &label,
            colors::WHITE,
            1,
        );

        Response::none(id, rect)
    }

    // ── Text input (single-line) ─────────────────────────────────

    /// Draw a single-line text edit field.
    pub fn text_edit_singleline(&mut self, text: &mut String) -> Response {
        let id = self.auto_id();
        let w = self.available_width().clamp(80, 300) as u32;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        // Gain focus on click
        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.focused_id = Some(id);
        }

        let focused = {
            let mem = UI_MEMORY.lock();
            mem.focused_id == Some(id)
        };

        // Handle keyboard input when focused
        let mut changed = false;
        if focused {
            if let Some(ch) = self.input.char_typed {
                if (' '..='~').contains(&ch) {
                    text.push(ch);
                    changed = true;
                }
            }
            if self.input.backspace_pressed && !text.is_empty() {
                text.pop();
                changed = true;
            }
            if self.input.escape_pressed || self.input.enter_pressed {
                let mut mem = UI_MEMORY.lock();
                mem.focused_id = None;
            }
        }

        // Draw
        let bg = if focused {
            Pixel::rgb(35, 38, 45)
        } else {
            self.style.widget_bg
        };
        self.fb.fill_rounded_rect_aa(rect, bg, 4);
        self.fb.draw_rounded_rect(
            rect,
            if focused {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            4,
            1,
        );

        // Text
        let display = if text.is_empty() && !focused {
            // Could show placeholder here
            ""
        } else {
            text.as_str()
        };
        // Truncate to visible width
        let max_chars = ((w as i32 - 12) / 8).max(0) as usize;
        let visible = if display.len() > max_chars {
            &display[display.len() - max_chars..]
        } else {
            display
        };
        fonts::draw_string_compact(
            self.fb,
            rect.x + 6,
            rect.y + (h as i32 - 12) / 2,
            visible,
            self.style.text_color,
            1,
        );

        // Cursor blink
        if focused {
            let cursor_x = rect.x + 6 + (visible.len() as i32 * 8);
            let phase = (self.input.frame_tick / 8) % 2;
            if phase == 0 {
                self.fb.fill_rect(
                    Rect::new(cursor_x, rect.y + 4, 2, h - 8),
                    self.style.text_color,
                );
            }
        }

        let mut r = resp;
        r.changed = changed;
        r.has_focus = focused;
        r
    }

    // ── Toggle / switch ──────────────────────────────────────────

    /// Draw a toggle switch with label.
    pub fn toggle(&mut self, enabled: &mut bool, text: &str) -> Response {
        let id = self.id_from(text);
        let switch_w = 36u32;
        let switch_h = 18u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = text_w + 8 + switch_w;
        let h = switch_h.max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            *enabled = !*enabled;
        }

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        // Switch track
        let sx = rect.x + text_w as i32 + 8;
        let sy = rect.y + (h as i32 - switch_h as i32) / 2;
        let track_color = if *enabled {
            self.style.accent
        } else {
            self.style.widget_bg
        };
        self.fb
            .fill_rounded_rect_aa(Rect::new(sx, sy, switch_w, switch_h), track_color, 9);

        // Knob
        let knob_x = if *enabled {
            sx + switch_w as i32 - switch_h as i32 + 2
        } else {
            sx + 2
        };
        self.fb.fill_circle_aa(knob_x + 7, sy + 9, 7, colors::WHITE);

        let mut r = resp;
        r.changed = r.clicked;
        r
    }

    // ── Layout helpers ───────────────────────────────────────────

    /// Run a closure with a horizontal (left-to-right) layout.
    /// Returns the `InnerResponse` containing the closure's result and the container's response.
    pub fn horizontal<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.with_layout(Layout::left_to_right(Align::Center), add_contents)
    }

    /// Run a closure with a vertical (top-down) layout.
    pub fn vertical<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.with_layout(Layout::top_down(Align::LEFT), add_contents)
    }

    /// Run a closure with a specific layout, in a child region.
    pub fn with_layout<R>(
        &mut self,
        layout: Layout,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let child_id = self.auto_id();
        let avail_w = self.available_width().max(0) as u32;
        let avail_h = self.available_height().max(0) as u32;
        let child_rect = Rect::new(self.region.cursor_x, self.region.cursor_y, avail_w, avail_h);

        // Create the child Ui by temporarily borrowing our framebuffer
        // We need to use unsafe to split the borrow, since the child needs &mut fb
        // but we also need to update our region afterwards.
        let saved_region = self.region;
        let saved_layout = self.layout;

        self.layout = layout;
        let old_cursor_x = self.region.cursor_x;
        let old_cursor_y = self.region.cursor_y;

        // Reset cursor within the child region
        let child_region = Region::from_max_rect(&layout, child_rect);
        self.region = child_region;
        let old_id = self.id;
        self.id = child_id;

        let inner = add_contents(self);

        let child_min_rect = self.region.min_rect;

        // Restore parent layout
        self.layout = saved_layout;
        self.region = saved_region;
        self.id = old_id;

        // Allocate the space used by the child in the parent layout
        let used_w = if child_min_rect.width > 0 {
            child_min_rect.width
        } else {
            0
        };
        let used_h = if child_min_rect.height > 0 {
            child_min_rect.height
        } else {
            0
        };
        let alloc_rect = self.allocate_space(used_w, used_h);

        let resp = Response::none(child_id, alloc_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    /// Run a closure with additional left indent.
    pub fn indent<R>(
        &mut self,
        id_salt: &str,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let indent = self.style.spacing.indent;
        self.region.cursor_x += indent;
        self.region.max_rect.x += indent;
        self.region.max_rect.width = (self.region.max_rect.width as i32 - indent).max(0) as u32;

        let child_id = self.id_from(id_salt);
        let old_id = self.id;
        self.id = child_id;

        let inner = add_contents(self);

        self.id = old_id;
        self.region.cursor_x -= indent;
        self.region.max_rect.x -= indent;
        self.region.max_rect.width = (self.region.max_rect.width as i32 + indent) as u32;

        let resp = Response::none(child_id, self.region.min_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    /// Draw a visual group (bordered box) around child widgets.
    pub fn group<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let group_id = self.auto_id();
        let avail_w = self.available_width().max(0) as u32;
        let group_start_y = self.region.cursor_y;
        let group_x = self.region.cursor_x;

        // Inset for padding
        let pad = 8i32;
        self.region.cursor_x += pad;
        self.region.cursor_y += pad;
        let orig_max_w = self.region.max_rect.width;
        self.region.max_rect.x += pad;
        self.region.max_rect.width = (self.region.max_rect.width as i32 - pad * 2).max(0) as u32;

        let inner = add_contents(self);

        self.region.cursor_x -= pad;
        self.region.max_rect.x -= pad;
        self.region.max_rect.width = orig_max_w;

        let group_end_y = self.region.cursor_y + pad;
        let group_h = (group_end_y - group_start_y).max(0) as u32;
        let group_rect = Rect::new(group_x, group_start_y, avail_w, group_h);

        self.fb.draw_rounded_rect(
            group_rect,
            self.style.border_color,
            self.style.corner_radius,
            1,
        );

        self.region.cursor_y = group_end_y + self.style.spacing.item_spacing_y;

        let resp = Response::none(group_id, group_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    // ── Collapsing header ────────────────────────────────────────

    /// Draw a collapsing/expanding section header.
    pub fn collapsing<R>(
        &mut self,
        title: &str,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let id = self.id_from(title);
        let is_open = {
            let mem = UI_MEMORY.lock();
            mem.get_bool(id, false)
        };

        // Header
        let header_w = self.available_width().max(0) as u32;
        let header_h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(header_w, header_h);
        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.set_bool(id, !is_open);
        }

        // Draw header bg on hover
        if resp.hovered {
            self.fb
                .fill_rounded_rect_aa(rect, self.style.widget_bg_hovered, 4);
        }

        // Triangle indicator
        let tri_x = rect.x + 4;
        let tri_y = rect.y + header_h as i32 / 2;
        if is_open {
            // Down triangle ▼
            for dy in 0..6i32 {
                let half = dy;
                self.fb.draw_hline(
                    tri_x + 3 - half,
                    tri_y - 3 + dy,
                    (half * 2 + 1) as u32,
                    self.style.text_color,
                );
            }
        } else {
            // Right triangle ►
            for dx in 0..6i32 {
                let half = dx;
                self.fb.draw_vline(
                    tri_x + dx,
                    tri_y - half,
                    (half * 2 + 1) as u32,
                    self.style.text_color,
                );
            }
        }

        // Title text
        if is_open {
            fonts::draw_string_bold_compact(
                self.fb,
                rect.x + 16,
                rect.y + (header_h as i32 - 12) / 2,
                title,
                self.style.text_color,
                1,
            );
        } else {
            fonts::draw_string_compact(
                self.fb,
                rect.x + 16,
                rect.y + (header_h as i32 - 12) / 2,
                title,
                self.style.text_color,
                1,
            );
        }

        // Content
        let inner = if is_open {
            let indent = self.style.spacing.indent;
            self.region.cursor_x += indent;
            self.region.max_rect.x += indent;
            self.region.max_rect.width = (self.region.max_rect.width as i32 - indent).max(0) as u32;

            let r = add_contents(self);

            self.region.cursor_x -= indent;
            self.region.max_rect.x -= indent;
            self.region.max_rect.width = (self.region.max_rect.width as i32 + indent) as u32;

            Some(r)
        } else {
            None
        };

        InnerResponse {
            inner,
            response: resp,
        }
    }

    // ── Combo box / dropdown ─────────────────────────────────────

    /// Draw a combo box (dropdown selector). Shows the `selected_text` and
    /// opens a popup listing items when clicked.
    pub fn combo_box(
        &mut self,
        label: &str,
        selected_text: &str,
        items: &[&str],
        current_index: &mut usize,
    ) -> Response {
        let id = self.id_from(label);
        let combo_w = self.style.spacing.combo_width as u32;
        let label_w = if label.is_empty() {
            0u32
        } else {
            (label.len() as u32) * 8 + 8
        };
        let total_w = label_w + combo_w;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        // Label
        if !label.is_empty() {
            fonts::draw_string_compact(
                self.fb,
                rect.x,
                rect.y + (h as i32 - 12) / 2,
                label,
                self.style.text_color,
                1,
            );
        }

        let combo_rect = Rect::new(rect.x + label_w as i32, rect.y, combo_w, h);
        let resp = self.interact(combo_rect, id, true, false);

        // Toggle open/closed
        let is_open = {
            let mem = UI_MEMORY.lock();
            mem.get_bool(id, false)
        };
        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.set_bool(id, !is_open);
        }

        // Draw combo box
        let bg = if is_open || resp.hovered {
            self.style.widget_bg_hovered
        } else {
            self.style.widget_bg
        };
        self.fb
            .fill_rounded_rect_aa(combo_rect, bg, self.style.corner_radius);
        self.fb.draw_rounded_rect(
            combo_rect,
            if is_open {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            self.style.corner_radius,
            1,
        );
        fonts::draw_string_compact(
            self.fb,
            combo_rect.x + 6,
            combo_rect.y + (h as i32 - 12) / 2,
            selected_text,
            self.style.text_color,
            1,
        );

        // Down arrow
        let arrow_x = combo_rect.x + combo_rect.width as i32 - 16;
        let arrow_y = combo_rect.y + h as i32 / 2 - 2;
        for dy in 0..4i32 {
            self.fb.draw_hline(
                arrow_x + 2 - dy,
                arrow_y + dy,
                (dy * 2 + 1) as u32,
                self.style.text_dimmed,
            );
        }

        // Dropdown popup
        let mut changed = false;
        if is_open && !items.is_empty() {
            let popup_x = combo_rect.x;
            let popup_y = combo_rect.y + h as i32 + 2;
            let popup_w = combo_rect.width;
            let item_h = 24u32;
            let popup_h = items.len() as u32 * item_h + 4;
            let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

            // Shadow
            self.fb.fill_rounded_rect_aa(
                Rect::new(popup_x + 2, popup_y + 2, popup_w, popup_h),
                Pixel::new(0, 0, 0, 100),
                self.style.corner_radius,
            );
            // Background
            self.fb.fill_rounded_rect_aa(
                popup_rect,
                Pixel::rgb(35, 38, 45),
                self.style.corner_radius,
            );
            self.fb.draw_rounded_rect(
                popup_rect,
                self.style.border_color,
                self.style.corner_radius,
                1,
            );

            for (i, item) in items.iter().enumerate() {
                let iy = popup_y + 2 + i as i32 * item_h as i32;
                let item_rect = Rect::new(popup_x + 2, iy, popup_w - 4, item_h);
                let is_selected = i == *current_index;
                let item_id = id.with_index(i);

                let pointer_in = item_rect.contains(self.input.pointer_x, self.input.pointer_y);

                if is_selected || pointer_in {
                    let bg = if is_selected {
                        self.style.accent
                    } else {
                        self.style.widget_bg_hovered
                    };
                    self.fb.fill_rounded_rect_aa(item_rect, bg, 3);
                }

                fonts::draw_string_compact(
                    self.fb,
                    item_rect.x + 6,
                    item_rect.y + (item_h as i32 - 12) / 2,
                    item,
                    colors::WHITE,
                    1,
                );

                if pointer_in && self.input.pointer_primary_released {
                    *current_index = i;
                    changed = true;
                    let mut mem = UI_MEMORY.lock();
                    mem.set_bool(id, false); // close
                }
            }

            // Close on click outside
            if self.input.pointer_primary_pressed
                && !popup_rect.contains(self.input.pointer_x, self.input.pointer_y)
                && !combo_rect.contains(self.input.pointer_x, self.input.pointer_y)
            {
                let mut mem = UI_MEMORY.lock();
                mem.set_bool(id, false);
            }
        }

        let mut r = resp;
        r.changed = changed;
        r
    }

    // ── Scroll area ──────────────────────────────────────────────

    /// Create a vertically scrollable area. The closure receives a child `Ui`
    /// whose content can exceed the visible height.
    pub fn scroll_area<R>(
        &mut self,
        id_salt: &str,
        height: u32,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let id = self.id_from(id_salt);
        let w = self.available_width().max(0) as u32;
        let area_rect = self.allocate_space(w, height);

        // Get current scroll offset
        let (_, scroll_y) = {
            let mem = UI_MEMORY.lock();
            mem.get_scroll(id)
        };

        // Push clip
        self.fb.push_clip(area_rect);

        // Create a tall child region offset by scroll
        let content_rect = Rect::new(
            area_rect.x,
            area_rect.y - scroll_y,
            w - self.style.spacing.scroll_bar_width as u32 - 2,
            100_000, // virtually infinite height
        );

        let saved_region = self.region;
        let saved_layout = self.layout;
        let saved_clip = self.clip_rect;
        let saved_id = self.id;

        self.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
        self.layout = Layout::top_down(Align::LEFT);
        self.clip_rect = area_rect;
        self.id = id;

        let inner = add_contents(self);

        let content_height = (self.region.cursor_y - (area_rect.y - scroll_y)).max(0);
        let child_min = self.region.min_rect;

        // Restore
        self.region = saved_region;
        self.layout = saved_layout;
        self.clip_rect = saved_clip;
        self.id = saved_id;

        self.fb.pop_clip();

        // Handle scroll input
        let pointer_in_area = area_rect.contains(self.input.pointer_x, self.input.pointer_y);
        let mut new_scroll_y = scroll_y;
        if pointer_in_area && self.input.scroll_delta != 0 {
            new_scroll_y -= self.input.scroll_delta * 20;
        }
        // Clamp scroll
        let max_scroll = (content_height - height as i32).max(0);
        new_scroll_y = new_scroll_y.clamp(0, max_scroll);

        {
            let mut mem = UI_MEMORY.lock();
            mem.set_scroll(id, 0, new_scroll_y);
        }

        // Draw scrollbar if content overflows
        if content_height > height as i32 {
            let sb_w = self.style.spacing.scroll_bar_width as u32;
            let sb_x = area_rect.x + area_rect.width as i32 - sb_w as i32;
            let sb_rect = Rect::new(sb_x, area_rect.y, sb_w, height);

            // Track
            self.fb
                .fill_rounded_rect_aa(sb_rect, Pixel::rgb(30, 32, 38), sb_w / 2);

            // Thumb
            let thumb_h = ((height as i64 * height as i64) / content_height as i64)
                .max(20)
                .min(height as i64) as u32;
            let thumb_y = area_rect.y
                + if max_scroll > 0 {
                    (new_scroll_y as i64 * (height as i64 - thumb_h as i64) / max_scroll as i64)
                        as i32
                } else {
                    0
                };
            let thumb_rect = Rect::new(sb_x, thumb_y, sb_w, thumb_h);
            let thumb_color = if pointer_in_area {
                Pixel::rgb(100, 105, 115)
            } else {
                Pixel::rgb(70, 75, 85)
            };
            self.fb
                .fill_rounded_rect_aa(thumb_rect, thumb_color, sb_w / 2);
        }

        let resp = Response::none(id, area_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    // ── Utility: formatted text ──────────────────────────────────

    /// Draw a label with a formatted string (convenience for `alloc::format!`).
    pub fn label_fmt(&mut self, args: core::fmt::Arguments) -> Response {
        let s = alloc::format!("{}", args);
        self.label(&s)
    }

    // ── Grid layout ──────────────────────────────────────────────

    /// Draw widgets in a grid with `columns` columns.
    /// The closure is called once; widgets are auto-wrapped into columns.
    pub fn grid<R>(
        &mut self,
        id_salt: &str,
        columns: usize,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let child_id = self.id_from(id_salt);
        let avail_w = self.available_width().max(0) as u32;
        let col_w = if columns > 0 {
            avail_w / columns as u32
        } else {
            avail_w
        };

        // Use a wrapping horizontal layout with known column width
        let layout = Layout::left_to_right(Align::TOP).with_main_wrap(true);
        let child_rect = Rect::new(
            self.region.cursor_x,
            self.region.cursor_y,
            avail_w,
            self.available_height().max(0) as u32,
        );

        let saved_region = self.region;
        let saved_layout = self.layout;
        let saved_id = self.id;

        self.region = Region::from_max_rect(&layout, child_rect);
        self.layout = layout;
        self.id = child_id;

        let inner = add_contents(self);

        let child_min = self.region.min_rect;
        self.layout = saved_layout;
        self.region = saved_region;
        self.id = saved_id;

        let used_h = if child_min.height > 0 {
            child_min.height
        } else {
            0
        };
        let alloc_rect = self.allocate_space(avail_w, used_h);

        let resp = Response::none(child_id, alloc_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    // ── Color picker (simple) ────────────────────────────────────

    /// Draw a simple color preview button. Returns `clicked`.
    pub fn color_button(&mut self, label: &str, color: Pixel) -> Response {
        let id = self.id_from(label);
        let swatch_size = 20u32;
        let label_w = (label.len() as u32) * 8 + 8;
        let total_w = label_w + swatch_size;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            label,
            self.style.text_color,
            1,
        );

        let sx = rect.x + label_w as i32;
        let sy = rect.y + (h as i32 - swatch_size as i32) / 2;
        let swatch_rect = Rect::new(sx, sy, swatch_size, swatch_size);
        self.fb.fill_rounded_rect_aa(swatch_rect, color, 3);
        self.fb.draw_rounded_rect(
            swatch_rect,
            if resp.hovered {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            3,
            1,
        );

        resp
    }

    // ── Image placeholder ────────────────────────────────────────

    /// Draw a colored rectangle as an image placeholder.
    pub fn image_placeholder(&mut self, width: u32, height: u32, color: Pixel) -> Response {
        let id = self.auto_id();
        let rect = self.allocate_space(width, height);
        self.fb.fill_rounded_rect_aa(rect, color, 4);
        Response::none(id, rect)
    }

    // ── Tooltip ──────────────────────────────────────────────────

    /// Show a tooltip near the pointer if the given response is hovered.
    pub fn show_tooltip(&mut self, resp: &Response, text: &str) {
        if !resp.hovered {
            return;
        }
        let text_w = (text.len() as u32) * 8 + 12;
        let text_h = 22u32;
        let tx = self.input.pointer_x + 12;
        let ty = self.input.pointer_y + 16;

        // Shadow
        self.fb.fill_rounded_rect_aa(
            Rect::new(tx + 2, ty + 2, text_w, text_h),
            Pixel::new(0, 0, 0, 120),
            6,
        );
        self.fb
            .fill_rounded_rect_aa(Rect::new(tx, ty, text_w, text_h), Pixel::rgb(50, 52, 58), 6);
        self.fb.draw_rounded_rect(
            Rect::new(tx, ty, text_w, text_h),
            self.style.border_color,
            6,
            1,
        );
        fonts::draw_string_compact(self.fb, tx + 6, ty + 5, text, colors::WHITE, 1);
    }

    // ── Spinner / loading ────────────────────────────────────────

    /// Draw a loading spinner animation.
    pub fn spinner(&mut self) -> Response {
        let id = self.auto_id();
        let size = 20u32;
        let rect = self.allocate_space(size, size);
        let cx = rect.x + size as i32 / 2;
        let cy = rect.y + size as i32 / 2;

        // Draw rotating dots
        let tick = self.input.frame_tick;
        let num_dots = 8u32;
        for i in 0..num_dots {
            let angle_step = 628 / num_dots as i32; // ~2π * 100
            let angle = (tick as i32 * 10 + i as i32 * angle_step) % 628;
            // Simple integer sin/cos approximation
            let (sin_a, cos_a) = int_sincos(angle);
            let dx = cos_a * 8 / 100;
            let dy = sin_a * 8 / 100;
            let alpha = 60 + (i as u8) * 25;
            self.fb.fill_circle_aa(
                cx + dx,
                cy + dy,
                2,
                Pixel::new(
                    self.style.accent.r,
                    self.style.accent.g,
                    self.style.accent.b,
                    alpha,
                ),
            );
        }

        Response::none(id, rect)
    }
}

/// Integer sin/cos approximation. Input: angle in centidegrees (0-628 ≈ 0-2π).
/// Returns (sin*100, cos*100).
fn int_sincos(angle: i32) -> (i32, i32) {
    // Very rough lookup using symmetry
    let a = angle.rem_euclid(628); // normalize to 0-627
    // Quarter tables for sin (0-π/2 in 0-157 steps)
    let quarter = a % 157;
    let sin_q = quarter * 100 / 157; // linear approximation of sin in [0, π/2]
    let sin_q = sin_q * (157 - quarter) * 4 / 157; // parabolic correction

    let (sin_val, cos_val) = match a / 157 {
        0 => (sin_q, 100 - sin_q),          // 0..π/2
        1 => (sin_q, -(100 - sin_q.abs())), // π/2..π  (adjusted below)
        2 => (-sin_q, -(100 - sin_q)),      // π..3π/2
        _ => (-sin_q, 100 - sin_q.abs()),   // 3π/2..2π
    };

    (sin_val.clamp(-100, 100), cos_val.clamp(-100, 100))
}
