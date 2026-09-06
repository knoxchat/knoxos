/// UI Widgets - Reusable GUI components
use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::theme::ThemeColors;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// Button widget
pub struct Button {
    pub rect: Rect,
    pub label: String,
    pub hovered: bool,
    pub pressed: bool,
    pub bg_color: Pixel,
    pub text_color: Pixel,
}

impl Button {
    pub fn new(x: i32, y: i32, width: u32, height: u32, label: &str) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            label: String::from(label),
            hovered: false,
            pressed: false,
            bg_color: Pixel::rgb(60, 60, 60),
            text_color: colors::WHITE,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let bg = if self.pressed {
            colors::darken(self.bg_color, 51)
        } else if self.hovered {
            colors::lighten(self.bg_color, 38)
        } else {
            self.bg_color
        };

        // AA rounded rect background with subtle radius for modern look
        fb.fill_rounded_rect_aa(self.rect, bg, 6);
        // 1px AA border highlight
        fb.draw_rounded_rect(self.rect, colors::lighten(bg, 51), 6, 1);
        fonts::draw_string_centered_compact(
            fb,
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height,
            &self.label,
            self.text_color,
            1,
        );
    }
}

/// Text input widget
pub struct TextInput {
    pub rect: Rect,
    pub text: String,
    pub placeholder: String,
    pub focused: bool,
    pub cursor_pos: usize,
    pub blink_tick: u32,
}

impl TextInput {
    pub fn new(x: i32, y: i32, width: u32, height: u32, placeholder: &str) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            text: String::new(),
            placeholder: String::from(placeholder),
            focused: false,
            cursor_pos: 0,
            blink_tick: 0,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        // AA rounded background
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(40, 40, 40), 4);

        // Border with AA
        let border_color = if self.focused {
            colors::HIGHLIGHT
        } else {
            Pixel::rgb(80, 80, 80)
        };
        fb.draw_rounded_rect(self.rect, border_color, 4, 1);

        // Text or placeholder
        let (text, color) = if self.text.is_empty() {
            (&self.placeholder, Pixel::rgb(120, 120, 120))
        } else {
            (&self.text, colors::WHITE)
        };

        fonts::draw_string_compact(
            fb,
            self.rect.x + 6,
            self.rect.y + (self.rect.height as i32 - 12) / 2,
            text,
            color,
            1,
        );

        // Cursor — smooth blinking with rounded shape
        if self.focused {
            let cursor_x = self.rect.x + 6 + (self.cursor_pos as i32 * 8);
            // Smooth blink: sinusoidal fade using tick counter
            // Period = 36 ticks (~2 sec at 18Hz). Use triangle wave for smooth on/off.
            let phase = self.blink_tick % 36;
            let alpha = if phase < 18 {
                // Fade in: 0→255 over 18 ticks
                (phase * 255 / 18) as u8
            } else {
                // Fade out: 255→0 over 18 ticks
                ((36 - phase) * 255 / 18) as u8
            };
            // Always visible for first second after focus/keystroke
            let alpha = if self.blink_tick < 18 { 255 } else { alpha };
            if alpha > 10 {
                let cursor_color = Pixel::new(255, 255, 255, alpha);
                fb.fill_rounded_rect_aa(
                    Rect::new(cursor_x, self.rect.y + 4, 2, self.rect.height - 8),
                    cursor_color,
                    1,
                );
            }
        }
    }

    /// Advance blink animation (call once per frame tick)
    pub fn tick(&mut self) {
        if self.focused {
            self.blink_tick += 1;
        }
    }

    /// Reset blink to fully visible (call on keystroke or focus)
    pub fn reset_blink(&mut self) {
        self.blink_tick = 0;
    }

    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor_pos, ch);
        self.cursor_pos += 1;
        self.blink_tick = 0; // Reset blink on input
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.text.remove(self.cursor_pos);
            self.blink_tick = 0; // Reset blink on input
        }
    }
}

/// Scrollbar widget
pub struct ScrollBar {
    pub rect: Rect,
    pub scroll_position: f32, // 0.0 - 1.0
    pub content_ratio: f32,   // visible / total
    pub vertical: bool,
}

impl ScrollBar {
    pub fn draw(&self, fb: &mut FrameBuffer) {
        // Track (AA rounded)
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(30, 30, 30), 4);

        // Thumb
        let thumb_size = (self.content_ratio
            * if self.vertical {
                self.rect.height as f32
            } else {
                self.rect.width as f32
            })
        .max(20.0) as u32;

        let max_travel = if self.vertical {
            self.rect.height - thumb_size
        } else {
            self.rect.width - thumb_size
        };

        let thumb_pos = (self.scroll_position * max_travel as f32) as i32;

        let thumb_rect = if self.vertical {
            Rect::new(
                self.rect.x,
                self.rect.y + thumb_pos,
                self.rect.width,
                thumb_size,
            )
        } else {
            Rect::new(
                self.rect.x + thumb_pos,
                self.rect.y,
                thumb_size,
                self.rect.height,
            )
        };

        fb.fill_rounded_rect_aa(thumb_rect, Pixel::rgb(80, 80, 80), 4);
    }
}

/// Context menu (right-click menu)
pub struct ContextMenu {
    pub rect: Rect,
    pub items: Vec<MenuItem>,
    pub visible: bool,
    pub hovered_index: Option<usize>,
}

pub struct MenuItem {
    pub label: String,
    pub separator: bool,
    pub enabled: bool,
}

impl ContextMenu {
    pub fn new(x: i32, y: i32, items: Vec<MenuItem>) -> Self {
        let width = 200u32;
        let height = items.len() as u32 * 24 + 8;
        Self {
            rect: Rect::new(x, y, width, height),
            items,
            visible: false,
            hovered_index: None,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        if !self.visible {
            return;
        }

        // Background with soft AA shadow
        fb.fill_rounded_rect_aa(
            Rect::new(
                self.rect.x + 2,
                self.rect.y + 2,
                self.rect.width,
                self.rect.height,
            ),
            Pixel::new(0, 0, 0, 100),
            8,
        );
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(40, 40, 40), 8);
        fb.draw_rounded_rect(self.rect, Pixel::rgb(70, 70, 70), 8, 1);

        // Items
        for (i, item) in self.items.iter().enumerate() {
            let item_y = self.rect.y + 4 + (i as i32 * 24);

            if item.separator {
                fb.draw_hline(
                    self.rect.x + 8,
                    item_y + 12,
                    self.rect.width - 16,
                    Pixel::rgb(70, 70, 70),
                );
                continue;
            }

            // Hover highlight (AA rounded)
            if self.hovered_index == Some(i) {
                fb.fill_rounded_rect_aa(
                    Rect::new(self.rect.x + 2, item_y, self.rect.width - 4, 24),
                    colors::SELECTION,
                    4,
                );
            }

            let text_color = if item.enabled {
                colors::WHITE
            } else {
                Pixel::rgb(100, 100, 100)
            };

            fonts::draw_string_compact(
                fb,
                self.rect.x + 12,
                item_y + 6,
                &item.label,
                text_color,
                1,
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHECKBOX WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Checkbox {
    pub x: i32,
    pub y: i32,
    pub checked: bool,
    pub label: String,
    pub hovered: bool,
}

impl Checkbox {
    pub fn new(x: i32, y: i32, label: &str, checked: bool) -> Self {
        Self {
            x,
            y,
            checked,
            label: String::from(label),
            hovered: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let box_size: u32 = 16;
        let bg = if self.checked {
            Pixel::rgb(82, 139, 255)
        } else if self.hovered {
            Pixel::rgb(55, 55, 60)
        } else {
            Pixel::rgb(40, 40, 44)
        };

        fb.fill_rounded_rect_aa(Rect::new(self.x, self.y, box_size, box_size), bg, 3);
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, box_size, box_size),
            if self.checked {
                Pixel::rgb(82, 139, 255)
            } else {
                Pixel::rgb(80, 80, 84)
            },
            3,
            1,
        );

        if self.checked {
            // Checkmark — anti-aliased
            fb.draw_line_aa(
                self.x + 3,
                self.y + 8,
                self.x + 6,
                self.y + 12,
                colors::WHITE,
            );
            fb.draw_line_aa(
                self.x + 6,
                self.y + 12,
                self.x + 12,
                self.y + 4,
                colors::WHITE,
            );
        }

        // Label
        fonts::draw_string_compact(
            fb,
            self.x + box_size as i32 + 8,
            self.y + 2,
            &self.label,
            colors::WHITE,
            1,
        );
    }

    pub fn hit_test(&self, mx: i32, my: i32) -> bool {
        mx >= self.x
            && mx < self.x + 16 + 8 + self.label.len() as i32 * 8
            && my >= self.y
            && my < self.y + 16
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SLIDER WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Slider {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub value: u8, // 0-100
    pub track_color: Pixel,
    pub fill_color: Pixel,
    pub dragging: bool,
}

impl Slider {
    pub fn new(x: i32, y: i32, width: u32, value: u8) -> Self {
        Self {
            x,
            y,
            width,
            value,
            track_color: Pixel::rgb(50, 50, 55),
            fill_color: Pixel::rgb(82, 139, 255),
            dragging: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let track_h: u32 = 6;
        let track_y = self.y + 4;

        // Track (AA)
        fb.fill_rounded_rect_aa(
            Rect::new(self.x, track_y, self.width, track_h),
            self.track_color,
            3,
        );

        // Fill (AA)
        let fill_w = (self.width * self.value as u32) / 100;
        if fill_w > 0 {
            fb.fill_rounded_rect_aa(
                Rect::new(self.x, track_y, fill_w, track_h),
                self.fill_color,
                3,
            );
        }

        // Thumb (AA for smooth edges)
        let thumb_x = self.x + fill_w as i32;
        let thumb_y = track_y + 3;
        fb.fill_circle_aa(thumb_x, thumb_y, 8, self.fill_color);
        fb.fill_circle_aa(thumb_x, thumb_y, 5, colors::WHITE);
    }

    pub fn hit_test(&self, mx: i32, my: i32) -> bool {
        mx >= self.x - 8
            && mx <= self.x + self.width as i32 + 8
            && my >= self.y - 4
            && my <= self.y + 16
    }

    pub fn update_from_mouse(&mut self, mx: i32) {
        let relative = (mx - self.x).clamp(0, self.width as i32);
        self.value = (relative * 100 / self.width as i32) as u8;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROGRESS BAR WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct ProgressBar {
    pub rect: Rect,
    pub progress: u8, // 0-100
    pub color: Pixel,
    pub show_text: bool,
    pub indeterminate: bool,
    pub animation_tick: u32,
}

impl ProgressBar {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            progress: 0,
            color: Pixel::rgb(82, 139, 255),
            show_text: true,
            indeterminate: false,
            animation_tick: 0,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        // Track (AA)
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(40, 40, 44), 3);

        if self.indeterminate {
            // Bouncing bar animation
            let bar_w = self.rect.width / 3;
            let travel = self.rect.width - bar_w;
            let pos = (self.animation_tick % (travel * 2)) as i32;
            let actual_pos = if pos < travel as i32 {
                pos
            } else {
                travel as i32 * 2 - pos
            };
            fb.fill_rounded_rect_aa(
                Rect::new(
                    self.rect.x + actual_pos,
                    self.rect.y,
                    bar_w,
                    self.rect.height,
                ),
                self.color,
                3,
            );
        } else {
            // Determinate fill
            let fill_w = (self.rect.width * self.progress as u32) / 100;
            if fill_w > 0 {
                fb.fill_rounded_rect_aa(
                    Rect::new(self.rect.x, self.rect.y, fill_w, self.rect.height),
                    self.color,
                    3,
                );
            }

            // Percentage text
            if self.show_text && self.rect.height >= 12 {
                let pct_str = alloc::format!("{}%", self.progress);
                fonts::draw_string_compact(
                    fb,
                    self.rect.x + (self.rect.width as i32 - pct_str.len() as i32 * 8) / 2,
                    self.rect.y + (self.rect.height as i32 - 12) / 2,
                    &pct_str,
                    colors::WHITE,
                    1,
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TOOLTIP WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Tooltip {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub show_delay_ticks: u32,
    pub current_ticks: u32,
}

impl Tooltip {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            x: 0,
            y: 0,
            visible: false,
            show_delay_ticks: 18, // ~1 second at 18 Hz
            current_ticks: 0,
        }
    }

    pub fn show(&mut self, x: i32, y: i32, text: &str) {
        self.x = x;
        self.y = y;
        self.text = String::from(text);
        self.current_ticks = 0;
        self.visible = false;
    }

    pub fn tick(&mut self) {
        if !self.text.is_empty() {
            self.current_ticks += 1;
            if self.current_ticks >= self.show_delay_ticks {
                self.visible = true;
            }
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.text.clear();
        self.current_ticks = 0;
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        if !self.visible || self.text.is_empty() {
            return;
        }

        let text_w = self.text.len() as u32 * 8 + 12;
        let text_h: u32 = 22;

        // Shadow (AA rounded)
        fb.fill_rounded_rect_aa(
            Rect::new(self.x + 2, self.y + 2, text_w, text_h),
            Pixel::new(0, 0, 0, 120),
            6,
        );

        // Background (AA rounded)
        fb.fill_rounded_rect_aa(
            Rect::new(self.x, self.y, text_w, text_h),
            Pixel::rgb(50, 50, 55),
            6,
        );
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, text_w, text_h),
            Pixel::rgb(80, 80, 84),
            6,
            1,
        );

        fonts::draw_string_compact(fb, self.x + 6, self.y + 5, &self.text, colors::WHITE, 1);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TABS WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct TabBar {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub tabs: Vec<String>,
    pub active_index: usize,
}

impl TabBar {
    pub fn new(x: i32, y: i32, width: u32, tabs: Vec<String>) -> Self {
        Self {
            x,
            y,
            width,
            tabs,
            active_index: 0,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let tab_h: u32 = 32;
        let tab_count = self.tabs.len().max(1);
        let tab_w = self.width / tab_count as u32;

        // Background
        fb.fill_rect(
            Rect::new(self.x, self.y, self.width, tab_h),
            Pixel::rgb(30, 30, 34),
        );

        for (i, tab_label) in self.tabs.iter().enumerate() {
            let tx = self.x + (i as u32 * tab_w) as i32;
            let is_active = i == self.active_index;

            if is_active {
                // Subtle active tab background with AA rounded corners
                fb.fill_rounded_rect_aa(
                    Rect::new(tx + 2, self.y + 2, tab_w - 4, tab_h - 4),
                    Pixel::rgb(40, 40, 44),
                    4,
                );
                // Rounded capsule indicator at bottom (pill shape)
                let indicator_w = tab_w.clamp(16, 48);
                let indicator_x = tx + (tab_w as i32 - indicator_w as i32) / 2;
                fb.fill_rounded_rect_aa(
                    Rect::new(indicator_x, self.y + tab_h as i32 - 3, indicator_w, 3),
                    Pixel::rgb(82, 139, 255),
                    2,
                );
            }

            let color = if is_active {
                colors::WHITE
            } else {
                Pixel::rgb(140, 140, 140)
            };
            if is_active {
                fonts::draw_string_centered_bold_compact(
                    fb, tx, self.y, tab_w, tab_h, tab_label, color, 1,
                );
            } else {
                fonts::draw_string_centered_compact(
                    fb, tx, self.y, tab_w, tab_h, tab_label, color, 1,
                );
            }
        }

        // Bottom border
        fb.draw_hline(
            self.x,
            self.y + tab_h as i32,
            self.width,
            Pixel::rgb(50, 50, 55),
        );
    }

    pub fn handle_click(&mut self, mx: i32, my: i32) -> bool {
        let tab_h = 32;
        if my < self.y || my > self.y + tab_h || mx < self.x || mx > self.x + self.width as i32 {
            return false;
        }
        let tab_count = self.tabs.len().max(1) as u32;
        let tab_w = self.width / tab_count;
        let idx = ((mx - self.x) as u32 / tab_w) as usize;
        if idx < self.tabs.len() {
            self.active_index = idx;
            return true;
        }
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BADGE / TAG WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Badge;

impl Badge {
    /// Draw a small count badge (like notification count)
    pub fn draw_count(fb: &mut FrameBuffer, x: i32, y: i32, count: u32) {
        if count == 0 {
            return;
        }

        let text = if count > 99 {
            String::from("99+")
        } else {
            alloc::format!("{}", count)
        };

        let w = (text.len() as u32 * 8 + 8).max(16);
        let h: u32 = 16;

        fb.fill_rounded_rect_aa(Rect::new(x, y, w, h), Pixel::rgb(247, 118, 142), 8);

        fonts::draw_string_compact(
            fb,
            x + (w as i32 - text.len() as i32 * 8) / 2,
            y + 2,
            &text,
            colors::WHITE,
            1,
        );
    }

    /// Draw a status dot (AA smooth)
    pub fn draw_dot(fb: &mut FrameBuffer, x: i32, y: i32, color: Pixel) {
        fb.fill_circle_aa(x, y, 4, color);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SEPARATOR WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Separator;

impl Separator {
    pub fn draw_horizontal(fb: &mut FrameBuffer, x: i32, y: i32, width: u32) {
        fb.draw_hline(x, y, width, Pixel::rgb(50, 50, 55));
    }

    pub fn draw_vertical(fb: &mut FrameBuffer, x: i32, y: i32, height: u32) {
        fb.draw_vline(x, y, height, Pixel::rgb(50, 50, 55));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RADIO BUTTON WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct RadioButton {
    pub x: i32,
    pub y: i32,
    pub selected: bool,
    pub label: String,
    pub hovered: bool,
}

impl RadioButton {
    pub fn new(x: i32, y: i32, label: &str, selected: bool) -> Self {
        Self {
            x,
            y,
            selected,
            label: String::from(label),
            hovered: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let radius: u32 = 8;
        let cx = self.x + radius as i32;
        let cy = self.y + radius as i32;

        // Outer circle
        let outer_color = if self.selected {
            Pixel::rgb(82, 139, 255)
        } else if self.hovered {
            Pixel::rgb(80, 80, 84)
        } else {
            Pixel::rgb(60, 60, 64)
        };
        fb.fill_circle_aa(cx, cy, radius, outer_color);

        // Inner background (2px ring)
        let inner_bg = if self.selected {
            Pixel::rgb(82, 139, 255)
        } else {
            Pixel::rgb(30, 30, 34)
        };
        fb.fill_circle_aa(cx, cy, radius - 2, inner_bg);

        // Selected dot (inner filled circle)
        if self.selected {
            fb.fill_circle_aa(cx, cy, 4, colors::WHITE);
        }

        // Label
        fonts::draw_string_compact(
            fb,
            self.x + radius as i32 * 2 + 8,
            self.y + 4,
            &self.label,
            colors::WHITE,
            1,
        );
    }

    pub fn hit_test(&self, mx: i32, my: i32) -> bool {
        mx >= self.x
            && mx < self.x + 16 + 8 + self.label.len() as i32 * 8
            && my >= self.y
            && my < self.y + 16
    }
}

/// Radio button group — renders a vertical list of radio options.
/// Only one can be selected at a time.
pub struct RadioGroup {
    pub x: i32,
    pub y: i32,
    pub options: Vec<String>,
    pub selected: usize,
    pub spacing: i32,
}

impl RadioGroup {
    pub fn new(x: i32, y: i32, options: &[&str], selected: usize) -> Self {
        Self {
            x,
            y,
            options: options.iter().map(|s| String::from(*s)).collect(),
            selected,
            spacing: 24,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        for (i, opt) in self.options.iter().enumerate() {
            let ry = self.y + i as i32 * self.spacing;
            let rb = RadioButton::new(self.x, ry, opt, i == self.selected);
            rb.draw(fb);
        }
    }

    /// Returns Some(index) if clicked on a radio option, None otherwise
    pub fn hit_test(&self, mx: i32, my: i32) -> Option<usize> {
        for (i, opt) in self.options.iter().enumerate() {
            let ry = self.y + i as i32 * self.spacing;
            let rb = RadioButton::new(self.x, ry, opt, i == self.selected);
            if rb.hit_test(mx, my) {
                return Some(i);
            }
        }
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TOGGLE SWITCH WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct ToggleSwitch {
    pub x: i32,
    pub y: i32,
    pub on: bool,
    pub label: String,
    pub hovered: bool,
}

impl ToggleSwitch {
    pub fn new(x: i32, y: i32, label: &str, on: bool) -> Self {
        Self {
            x,
            y,
            on,
            label: String::from(label),
            hovered: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let track_w: u32 = 36;
        let track_h: u32 = 20;
        let thumb_r: u32 = 8;

        // Track background
        let track_color = if self.on {
            Pixel::rgb(82, 139, 255)
        } else if self.hovered {
            Pixel::rgb(70, 70, 76)
        } else {
            Pixel::rgb(50, 50, 55)
        };
        fb.fill_rounded_rect_aa(
            Rect::new(self.x, self.y, track_w, track_h),
            track_color,
            track_h / 2,
        );

        // Thumb (sliding circle)
        let thumb_x = if self.on {
            self.x + track_w as i32 - thumb_r as i32 - 2
        } else {
            self.x + thumb_r as i32 + 2
        };
        let thumb_y = self.y + track_h as i32 / 2;
        fb.fill_circle_aa(thumb_x, thumb_y, thumb_r, colors::WHITE);

        // Label
        fonts::draw_string_compact(
            fb,
            self.x + track_w as i32 + 10,
            self.y + 4,
            &self.label,
            colors::WHITE,
            1,
        );
    }

    pub fn hit_test(&self, mx: i32, my: i32) -> bool {
        mx >= self.x
            && mx < self.x + 36 + 10 + self.label.len() as i32 * 8
            && my >= self.y
            && my < self.y + 20
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DATE/TIME PICKER WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct DateTimePicker {
    pub x: i32,
    pub y: i32,
    pub year: u16,
    pub month: u8,  // 1-12
    pub day: u8,    // 1-31
    pub hour: u8,   // 0-23
    pub minute: u8, // 0-59
    pub show_time: bool,
}

impl DateTimePicker {
    pub fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            year: 2026,
            month: 3,
            day: 7,
            hour: 12,
            minute: 0,
            show_time: true,
        }
    }

    fn days_in_month(&self) -> u8 {
        match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (self.year % 4 == 0 && self.year % 100 != 0) || self.year % 400 == 0 {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        use alloc::format;

        let bg = Pixel::rgb(36, 36, 42);
        let w: u32 = if self.show_time { 280 } else { 220 };
        let h: u32 = 36;

        // Background
        fb.fill_rounded_rect_aa(Rect::new(self.x, self.y, w, h), bg, 6);
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, w, h),
            Pixel::rgb(60, 60, 66),
            6,
            1,
        );

        // Date display
        let month_names = [
            "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let month_name = month_names.get(self.month as usize).unwrap_or(&"???");
        let date_str = format!("{} {:02}, {:04}", month_name, self.day, self.year);
        fonts::draw_string_compact(fb, self.x + 10, self.y + 11, &date_str, colors::WHITE, 1);

        if self.show_time {
            let time_str = format!("{:02}:{:02}", self.hour, self.minute);
            fonts::draw_string_compact(
                fb,
                self.x + 170,
                self.y + 11,
                &time_str,
                Pixel::rgb(140, 200, 255),
                1,
            );
        }

        // Navigation arrows (< >)
        fonts::draw_string_compact(
            fb,
            self.x - 16,
            self.y + 11,
            "<",
            Pixel::rgb(120, 120, 130),
            1,
        );
        fonts::draw_string_compact(
            fb,
            self.x + w as i32 + 6,
            self.y + 11,
            ">",
            Pixel::rgb(120, 120, 130),
            1,
        );
    }

    /// Handle click. Returns true if the arrow buttons were clicked (to change date).
    pub fn handle_click(&mut self, mx: i32, my: i32) -> bool {
        let w: i32 = if self.show_time { 280 } else { 220 };
        let h: i32 = 36;

        // Left arrow (previous day)
        if mx >= self.x - 20 && mx < self.x && my >= self.y && my < self.y + h {
            if self.day > 1 {
                self.day -= 1;
            } else {
                if self.month > 1 {
                    self.month -= 1;
                } else {
                    self.month = 12;
                    self.year -= 1;
                }
                self.day = self.days_in_month();
            }
            return true;
        }

        // Right arrow (next day)
        if mx >= self.x + w && mx < self.x + w + 20 && my >= self.y && my < self.y + h {
            if self.day < self.days_in_month() {
                self.day += 1;
            } else {
                self.day = 1;
                if self.month < 12 {
                    self.month += 1;
                } else {
                    self.month = 1;
                    self.year += 1;
                }
            }
            return true;
        }

        false
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Rating Widget — Star-based rating selector
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct RatingWidget {
    pub x: i32,
    pub y: i32,
    pub max_stars: u8,
    pub current: u8,
    pub star_size: i32,
    pub gap: i32,
}

impl RatingWidget {
    pub fn new(x: i32, y: i32, max_stars: u8, current: u8) -> Self {
        Self {
            x,
            y,
            max_stars: max_stars.min(10),
            current: current.min(max_stars),
            star_size: 16,
            gap: 4,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        for i in 0..self.max_stars {
            let sx = self.x + i as i32 * (self.star_size + self.gap);
            let filled = i < self.current;
            let color = if filled {
                Pixel::rgb(255, 200, 50)
            } else {
                Pixel::new(
                    theme.text_primary.r,
                    theme.text_primary.g,
                    theme.text_primary.b,
                    60,
                )
            };
            let cx = sx + self.star_size / 2;
            let cy = self.y + self.star_size / 2;
            let r = self.star_size / 2;
            if filled {
                fb.fill_circle_aa(cx, cy, (r - 1) as u32, color);
                // Star points
                fb.fill_rect(Rect::new(cx - 1, cy - r, 2, (r / 2) as u32), color);
                fb.fill_rect(Rect::new(cx - r, cy - 2, (r / 2) as u32, 3), color);
                fb.fill_rect(Rect::new(cx + r / 2, cy - 2, (r / 2) as u32, 3), color);
                fb.fill_rect(Rect::new(cx - r / 2, cy + r / 2, 2, (r / 2) as u32), color);
                fb.fill_rect(
                    Rect::new(cx + r / 2 - 1, cy + r / 2, 2, (r / 2) as u32),
                    color,
                );
            } else {
                fb.draw_rounded_rect(
                    Rect::new(
                        sx + 2,
                        self.y + 2,
                        (self.star_size - 4) as u32,
                        (self.star_size - 4) as u32,
                    ),
                    color,
                    (r - 2) as u32,
                    1,
                );
            }
        }
    }

    /// Hit test — returns Some(new_rating) if clicked
    pub fn hit_test(&self, mx: i32, my: i32) -> Option<u8> {
        if my < self.y || my > self.y + self.star_size {
            return None;
        }
        let total_w = self.max_stars as i32 * (self.star_size + self.gap) - self.gap;
        if mx < self.x || mx > self.x + total_w {
            return None;
        }
        let rel_x = mx - self.x;
        let star_idx = (rel_x / (self.star_size + self.gap)) as u8;
        Some((star_idx + 1).min(self.max_stars))
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Avatar Widget — Circular user icon with initials
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct AvatarWidget {
    pub x: i32,
    pub y: i32,
    pub radius: u32,
    pub initials: [u8; 2],
    pub bg_color: Pixel,
}

impl AvatarWidget {
    pub fn new(x: i32, y: i32, radius: u32, name: &str, bg_color: Pixel) -> Self {
        let bytes = name.as_bytes();
        let first = if !bytes.is_empty() {
            bytes[0].to_ascii_uppercase()
        } else {
            b'?'
        };
        let second = name
            .split_whitespace()
            .nth(1)
            .and_then(|w| w.as_bytes().first().copied())
            .map(|b| b.to_ascii_uppercase())
            .unwrap_or(0);
        Self {
            x,
            y,
            radius,
            initials: [first, second],
            bg_color,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let cx = self.x + self.radius as i32;
        let cy = self.y + self.radius as i32;

        fb.fill_circle_aa(cx, cy, self.radius, self.bg_color);
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, self.radius * 2, self.radius * 2),
            Pixel::new(255, 255, 255, 40),
            self.radius,
            1,
        );

        let mut text = [0u8; 2];
        let mut len = 0;
        text[0] = self.initials[0];
        len += 1;
        if self.initials[1] != 0 {
            text[1] = self.initials[1];
            len += 1;
        }
        let s = core::str::from_utf8(&text[..len]).unwrap_or("?");
        let text_w = fonts::measure_string_width_compact(s, 1) as i32;
        let text_x = cx - text_w / 2;
        let text_y = cy - 5;
        fonts::draw_string_compact(fb, text_x, text_y, s, Pixel::rgb(255, 255, 255), 1);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Pagination Widget — Page number navigation
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PaginationWidget {
    pub x: i32,
    pub y: i32,
    pub total_pages: usize,
    pub current_page: usize,
    pub button_size: i32,
    pub gap: i32,
}

impl PaginationWidget {
    pub fn new(x: i32, y: i32, total_pages: usize, current_page: usize) -> Self {
        Self {
            x,
            y,
            total_pages: total_pages.max(1),
            current_page: current_page.min(total_pages.saturating_sub(1)),
            button_size: 28,
            gap: 4,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        // Prev arrow
        let prev_rect = Rect::new(
            self.x,
            self.y,
            self.button_size as u32,
            self.button_size as u32,
        );
        fb.fill_rounded_rect_aa(
            prev_rect,
            Pixel::new(
                theme.bg_surface.r,
                theme.bg_surface.g,
                theme.bg_surface.b,
                200,
            ),
            6,
        );
        fonts::draw_string_compact(fb, self.x + 8, self.y + 8, "<", theme.text_primary, 1);

        // Page buttons (max 7 visible)
        let max_visible = 7usize.min(self.total_pages);
        let start_page = if self.total_pages <= max_visible || self.current_page < max_visible / 2 {
            0
        } else if self.current_page >= self.total_pages - max_visible / 2 {
            self.total_pages - max_visible
        } else {
            self.current_page - max_visible / 2
        };

        let mut cx = self.x + self.button_size + self.gap;
        for i in 0..max_visible {
            let page = start_page + i;
            let rect = Rect::new(cx, self.y, self.button_size as u32, self.button_size as u32);
            let is_current = page == self.current_page;
            if is_current {
                fb.fill_rounded_rect_aa(rect, Pixel::new(0, 160, 255, 200), 6);
            } else {
                fb.fill_rounded_rect_aa(
                    rect,
                    Pixel::new(
                        theme.bg_surface.r,
                        theme.bg_surface.g,
                        theme.bg_surface.b,
                        160,
                    ),
                    6,
                );
            }
            let num_str = page + 1;
            let mut buf = [0u8; 4];
            let s = format_usize(num_str, &mut buf);
            let tw = fonts::measure_string_width_compact(s, 1) as i32;
            let text_color = if is_current {
                Pixel::rgb(255, 255, 255)
            } else {
                theme.text_primary
            };
            fonts::draw_string_compact(
                fb,
                cx + (self.button_size - tw) / 2,
                self.y + 8,
                s,
                text_color,
                1,
            );
            cx += self.button_size + self.gap;
        }

        // Next arrow
        let next_rect = Rect::new(cx, self.y, self.button_size as u32, self.button_size as u32);
        fb.fill_rounded_rect_aa(
            next_rect,
            Pixel::new(
                theme.bg_surface.r,
                theme.bg_surface.g,
                theme.bg_surface.b,
                200,
            ),
            6,
        );
        fonts::draw_string_compact(fb, cx + 8, self.y + 8, ">", theme.text_primary, 1);
    }

    /// Hit test — returns Some(new_page) if a page button was clicked
    pub fn hit_test(&self, mx: i32, my: i32) -> Option<usize> {
        if my < self.y || my > self.y + self.button_size {
            return None;
        }
        if mx >= self.x && mx < self.x + self.button_size {
            return Some(self.current_page.saturating_sub(1));
        }
        let max_visible = 7usize.min(self.total_pages);
        let start_page = if self.total_pages <= max_visible || self.current_page < max_visible / 2 {
            0
        } else if self.current_page >= self.total_pages - max_visible / 2 {
            self.total_pages - max_visible
        } else {
            self.current_page - max_visible / 2
        };
        let mut cx = self.x + self.button_size + self.gap;
        for i in 0..max_visible {
            if mx >= cx && mx < cx + self.button_size {
                return Some(start_page + i);
            }
            cx += self.button_size + self.gap;
        }
        if mx >= cx && mx < cx + self.button_size {
            return Some((self.current_page + 1).min(self.total_pages - 1));
        }
        None
    }
}

/// Format a usize as a decimal string into a fixed buffer
fn format_usize(mut n: usize, buf: &mut [u8; 4]) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap_or("0");
    }
    let mut i = buf.len();
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("0")
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tag Input Widget — Chips with removable tags
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct TagInput {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub tags: Vec<String>,
}

impl TagInput {
    pub fn new(x: i32, y: i32, width: u32) -> Self {
        Self {
            x,
            y,
            width,
            tags: Vec::new(),
        }
    }

    pub fn add_tag(&mut self, tag: &str) {
        if !tag.is_empty() && !self.tags.iter().any(|t| t == tag) {
            self.tags.push(String::from(tag));
        }
    }

    pub fn remove_tag(&mut self, index: usize) {
        if index < self.tags.len() {
            self.tags.remove(index);
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        let chip_h = 24i32;
        let chip_gap = 6i32;
        let chip_pad = 8i32;
        let mut cx = self.x;
        let cy = self.y;

        let container_h = chip_h + 8;
        fb.fill_rounded_rect_aa(
            Rect::new(
                self.x - 4,
                self.y - 4,
                self.width + 8,
                container_h as u32 + 8,
            ),
            Pixel::new(
                theme.bg_surface.r,
                theme.bg_surface.g,
                theme.bg_surface.b,
                180,
            ),
            8,
        );

        for tag in &self.tags {
            let text_w = fonts::measure_string_width_compact(tag, 1) as i32;
            let chip_w = text_w + chip_pad * 2 + 16;
            if cx + chip_w > self.x + self.width as i32 {
                break;
            }
            let chip_rect = Rect::new(cx, cy, chip_w as u32, chip_h as u32);
            fb.fill_rounded_rect_aa(chip_rect, Pixel::new(0, 140, 220, 180), (chip_h / 2) as u32);
            fonts::draw_string_compact(
                fb,
                cx + chip_pad,
                cy + 6,
                tag,
                Pixel::rgb(255, 255, 255),
                1,
            );
            let x_btn = cx + chip_w - 14;
            fonts::draw_string_compact(fb, x_btn, cy + 6, "x", Pixel::new(255, 255, 255, 180), 1);
            cx += chip_w + chip_gap;
        }
    }

    /// Hit test — returns Some(tag_index) if the × of a tag was clicked
    pub fn hit_test_remove(&self, mx: i32, my: i32) -> Option<usize> {
        let chip_h = 24i32;
        let chip_gap = 6i32;
        let chip_pad = 8i32;
        if my < self.y || my > self.y + chip_h {
            return None;
        }
        let mut cx = self.x;
        for (i, tag) in self.tags.iter().enumerate() {
            let text_w = fonts::measure_string_width_compact(tag, 1) as i32;
            let chip_w = text_w + chip_pad * 2 + 16;
            if cx + chip_w > self.x + self.width as i32 {
                break;
            }
            let x_area_start = cx + chip_w - 16;
            if mx >= x_area_start && mx <= cx + chip_w {
                return Some(i);
            }
            cx += chip_w + chip_gap;
        }
        None
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Stepper / Wizard Widget — Multi-step progress indicator
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct StepperWidget {
    pub x: i32,
    pub y: i32,
    pub steps: Vec<String>,
    pub current_step: usize,
    pub step_width: i32,
}

impl StepperWidget {
    pub fn new(x: i32, y: i32, steps: Vec<String>) -> Self {
        Self {
            x,
            y,
            step_width: 120,
            current_step: 0,
            steps,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        let circle_r = 14u32;
        let line_y = self.y + circle_r as i32;

        for (i, label) in self.steps.iter().enumerate() {
            let cx = self.x + i as i32 * self.step_width + self.step_width / 2;
            let cy = line_y;

            let is_done = i < self.current_step;
            let is_current = i == self.current_step;

            // Connecting line to next step
            if i + 1 < self.steps.len() {
                let next_cx = self.x + (i + 1) as i32 * self.step_width + self.step_width / 2;
                let line_color = if is_done {
                    Pixel::new(0, 180, 120, 200)
                } else {
                    Pixel::new(
                        theme.text_primary.r,
                        theme.text_primary.g,
                        theme.text_primary.b,
                        40,
                    )
                };
                fb.fill_rect(
                    Rect::new(
                        cx + circle_r as i32,
                        cy - 1,
                        (next_cx - cx - circle_r as i32 * 2) as u32,
                        2,
                    ),
                    line_color,
                );
            }

            // Step circle
            if is_done {
                fb.fill_circle_aa(cx, cy, circle_r, Pixel::new(0, 180, 120, 220));
                fonts::draw_string_compact(fb, cx - 4, cy - 5, "v", Pixel::rgb(255, 255, 255), 1);
            } else if is_current {
                fb.fill_circle_aa(cx, cy, circle_r, Pixel::new(0, 140, 255, 220));
                let mut buf = [0u8; 4];
                let s = format_usize(i + 1, &mut buf);
                let tw = fonts::measure_string_width_compact(s, 1) as i32;
                fonts::draw_string_compact(
                    fb,
                    cx - tw / 2,
                    cy - 5,
                    s,
                    Pixel::rgb(255, 255, 255),
                    1,
                );
            } else {
                fb.fill_circle_aa(
                    cx,
                    cy,
                    circle_r,
                    Pixel::new(
                        theme.bg_surface.r,
                        theme.bg_surface.g,
                        theme.bg_surface.b,
                        200,
                    ),
                );
                fb.draw_rounded_rect(
                    Rect::new(
                        cx - circle_r as i32,
                        cy - circle_r as i32,
                        circle_r * 2,
                        circle_r * 2,
                    ),
                    Pixel::new(
                        theme.text_primary.r,
                        theme.text_primary.g,
                        theme.text_primary.b,
                        60,
                    ),
                    circle_r,
                    1,
                );
                let mut buf = [0u8; 4];
                let s = format_usize(i + 1, &mut buf);
                let tw = fonts::measure_string_width_compact(s, 1) as i32;
                fonts::draw_string_compact(
                    fb,
                    cx - tw / 2,
                    cy - 5,
                    s,
                    Pixel::new(
                        theme.text_primary.r,
                        theme.text_primary.g,
                        theme.text_primary.b,
                        120,
                    ),
                    1,
                );
            }

            // Label below circle
            let label_w = fonts::measure_string_width_compact(label, 1) as i32;
            let label_x = cx - label_w / 2;
            let label_y = cy + circle_r as i32 + 6;
            let label_color = if is_done || is_current {
                theme.text_primary
            } else {
                Pixel::new(
                    theme.text_primary.r,
                    theme.text_primary.g,
                    theme.text_primary.b,
                    100,
                )
            };
            fonts::draw_string_compact(fb, label_x, label_y, label, label_color, 1);
        }
    }

    pub fn next(&mut self) -> bool {
        if self.current_step + 1 < self.steps.len() {
            self.current_step += 1;
            true
        } else {
            false
        }
    }

    pub fn prev(&mut self) -> bool {
        if self.current_step > 0 {
            self.current_step -= 1;
            true
        } else {
            false
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Notification Banner Widget — Inline alert/message bar
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub enum BannerType {
    Info,
    Success,
    Warning,
    Error,
}

pub struct NotificationBanner {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub message: String,
    pub banner_type: BannerType,
}

impl NotificationBanner {
    pub fn new(x: i32, y: i32, width: u32, message: &str, banner_type: BannerType) -> Self {
        Self {
            x,
            y,
            width,
            message: String::from(message),
            banner_type,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let h = 36u32;
        let rect = Rect::new(self.x, self.y, self.width, h);

        let (bg, icon_color, icon_char) = match self.banner_type {
            BannerType::Info => (Pixel::new(20, 60, 120, 220), Pixel::rgb(80, 180, 255), "i"),
            BannerType::Success => (Pixel::new(20, 80, 40, 220), Pixel::rgb(60, 220, 100), "v"),
            BannerType::Warning => (Pixel::new(100, 80, 10, 220), Pixel::rgb(255, 200, 50), "!"),
            BannerType::Error => (Pixel::new(100, 20, 20, 220), Pixel::rgb(255, 80, 80), "x"),
        };

        fb.fill_rounded_rect_aa(rect, bg, 8);

        fb.fill_circle_aa(self.x + 20, self.y + h as i32 / 2, 10, icon_color);
        fonts::draw_string_compact(
            fb,
            self.x + 16,
            self.y + h as i32 / 2 - 5,
            icon_char,
            Pixel::rgb(255, 255, 255),
            1,
        );

        let text_x = self.x + 38;
        let text_y = self.y + (h as i32 - 10) / 2;
        fonts::draw_string_compact(
            fb,
            text_x,
            text_y,
            &self.message,
            Pixel::new(230, 235, 245, 240),
            1,
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// File Picker Dialog Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// File picker mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePickerMode {
    Open,
    Save,
    SelectDirectory,
}

/// File picker dialog
pub struct FilePickerDialog {
    pub rect: Rect,
    pub mode: FilePickerMode,
    pub current_path: String,
    pub selected_file: Option<String>,
    pub entries: Vec<FilePickerEntry>,
    pub filter: String,
    pub filename_input: String,
    pub scroll_offset: usize,
    pub visible: bool,
    pub confirmed: bool,
    pub cancelled: bool,
}

/// An entry in the file picker
#[derive(Debug, Clone)]
pub struct FilePickerEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub selected: bool,
}

impl FilePickerDialog {
    pub fn new(x: i32, y: i32, width: u32, height: u32, mode: FilePickerMode) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            mode,
            current_path: String::from("/"),
            selected_file: None,
            entries: Vec::new(),
            filter: String::from("*"),
            filename_input: String::new(),
            scroll_offset: 0,
            visible: true,
            confirmed: false,
            cancelled: false,
        }
    }

    /// Refresh entries from VFS
    pub fn refresh(&mut self) {
        self.entries.clear();
        if let Ok(dir_entries) = crate::file_manager::list_dir(&self.current_path) {
            for entry in dir_entries {
                let is_dir = entry.file_type == crate::vfs::FileType::Directory;
                self.entries.push(FilePickerEntry {
                    name: entry.name.clone(),
                    is_dir,
                    size: entry.size,
                    selected: false,
                });
            }
        }
        // Sort: dirs first, then alphabetical
        self.entries
            .sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    }

    /// Navigate to directory
    pub fn navigate(&mut self, dir: &str) {
        if dir == ".." {
            if let Some(pos) = self.current_path.rfind('/') {
                if pos == 0 {
                    self.current_path = String::from("/");
                } else {
                    self.current_path.truncate(pos);
                }
            }
        } else {
            if self.current_path.ends_with('/') {
                self.current_path.push_str(dir);
            } else {
                self.current_path.push('/');
                self.current_path.push_str(dir);
            }
        }
        self.scroll_offset = 0;
        self.refresh();
    }

    /// Get the full path of the selected file
    pub fn selected_path(&self) -> Option<String> {
        self.selected_file.as_ref().map(|f| {
            if self.current_path.ends_with('/') {
                alloc::format!("{}{}", self.current_path, f)
            } else {
                alloc::format!("{}/{}", self.current_path, f)
            }
        })
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        if !self.visible {
            return;
        }
        let r = self.rect;
        // Background
        fb.fill_rounded_rect_aa(r, Pixel::new(30, 30, 45, 240), 12);
        fb.draw_rounded_rect(r, Pixel::rgb(80, 80, 120), 12, 1);

        // Title
        let title = match self.mode {
            FilePickerMode::Open => "Open File",
            FilePickerMode::Save => "Save File",
            FilePickerMode::SelectDirectory => "Select Directory",
        };
        fonts::draw_string_compact(fb, r.x + 12, r.y + 10, title, colors::WHITE, 1);

        // Path bar
        fonts::draw_string_compact(
            fb,
            r.x + 12,
            r.y + 30,
            &self.current_path,
            Pixel::rgb(150, 150, 200),
            1,
        );

        // File list
        let list_y = r.y + 50;
        let row_h = 20i32;
        let max_rows = ((r.height as i32 - 90) / row_h).max(1) as usize;
        for (i, entry) in self
            .entries
            .iter()
            .skip(self.scroll_offset)
            .take(max_rows)
            .enumerate()
        {
            let ey = list_y + i as i32 * row_h;
            let icon = if entry.is_dir { "[D] " } else { "    " };
            let label = alloc::format!("{}{}", icon, entry.name);
            let color = if entry.selected {
                Pixel::rgb(100, 200, 255)
            } else if entry.is_dir {
                Pixel::rgb(200, 200, 255)
            } else {
                Pixel::rgb(200, 200, 200)
            };
            fonts::draw_string_compact(fb, r.x + 12, ey, &label, color, 1);
        }

        // OK / Cancel buttons
        let btn_y = r.y + r.height as i32 - 32;
        fb.fill_rounded_rect_aa(
            Rect::new(r.x + r.width as i32 - 150, btn_y, 60, 24),
            Pixel::rgb(40, 120, 200),
            6,
        );
        fonts::draw_string_compact(
            fb,
            r.x + r.width as i32 - 142,
            btn_y + 6,
            "OK",
            colors::WHITE,
            1,
        );
        fb.fill_rounded_rect_aa(
            Rect::new(r.x + r.width as i32 - 80, btn_y, 68, 24),
            Pixel::rgb(80, 80, 80),
            6,
        );
        fonts::draw_string_compact(
            fb,
            r.x + r.width as i32 - 72,
            btn_y + 6,
            "Cancel",
            colors::WHITE,
            1,
        );
    }

    pub fn handle_click(&mut self, x: i32, y: i32) -> bool {
        if !self.rect.contains(x, y) {
            return false;
        }
        let list_y = self.rect.y + 50;
        let row_h = 20i32;
        let max_rows = ((self.rect.height as i32 - 90) / row_h).max(1) as usize;

        // Check file list clicks
        let idx = ((y - list_y) / row_h) as usize + self.scroll_offset;
        if idx < self.entries.len() && y >= list_y {
            let entry = &self.entries[idx];
            if entry.is_dir {
                let name = entry.name.clone();
                self.navigate(&name);
            } else {
                for e in &mut self.entries {
                    e.selected = false;
                }
                self.entries[idx].selected = true;
                self.selected_file = Some(self.entries[idx].name.clone());
            }
            return true;
        }

        // OK button
        let btn_y = self.rect.y + self.rect.height as i32 - 32;
        if x >= self.rect.x + self.rect.width as i32 - 150
            && x < self.rect.x + self.rect.width as i32 - 90
            && y >= btn_y
            && y < btn_y + 24
        {
            self.confirmed = true;
            self.visible = false;
            return true;
        }
        // Cancel
        if x >= self.rect.x + self.rect.width as i32 - 80 && y >= btn_y && y < btn_y + 24 {
            self.cancelled = true;
            self.visible = false;
            return true;
        }
        true
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Chart / Graph Widget (line, bar, pie)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Line,
    Bar,
    Pie,
}

/// A data series for the chart
#[derive(Debug, Clone)]
pub struct ChartSeries {
    pub label: String,
    pub color: Pixel,
    pub values: Vec<f32>,
}

/// Chart widget
pub struct Chart {
    pub rect: Rect,
    pub chart_type: ChartType,
    pub title: String,
    pub series: Vec<ChartSeries>,
    pub x_labels: Vec<String>,
    pub y_min: f32,
    pub y_max: f32,
    pub bg_color: Pixel,
    pub grid_color: Pixel,
    pub text_color: Pixel,
}

impl Chart {
    pub fn new(x: i32, y: i32, w: u32, h: u32, chart_type: ChartType, title: &str) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            chart_type,
            title: String::from(title),
            series: Vec::new(),
            x_labels: Vec::new(),
            y_min: 0.0,
            y_max: 100.0,
            bg_color: Pixel::new(20, 20, 30, 240),
            grid_color: Pixel::new(60, 60, 80, 128),
            text_color: Pixel::rgb(180, 180, 200),
        }
    }

    pub fn add_series(&mut self, label: &str, color: Pixel, values: Vec<f32>) {
        self.series.push(ChartSeries {
            label: String::from(label),
            color,
            values,
        });
        // Auto-scale
        for s in &self.series {
            for &v in &s.values {
                if v > self.y_max {
                    self.y_max = v;
                }
                if v < self.y_min {
                    self.y_min = v;
                }
            }
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let r = self.rect;
        fb.fill_rounded_rect_aa(r, self.bg_color, 8);

        // Title
        fonts::draw_string_compact(fb, r.x + 8, r.y + 4, &self.title, self.text_color, 1);

        let margin = 40i32;
        let plot_x = r.x + margin;
        let plot_y = r.y + 24;
        let plot_w = (r.width as i32 - margin - 8).max(1);
        let plot_h = (r.height as i32 - 32 - 8).max(1);

        // Grid lines
        for i in 0..=4 {
            let gy = plot_y + plot_h - (plot_h * i / 4);
            for gx in (plot_x..plot_x + plot_w).step_by(3) {
                fb.set_pixel(gx as usize, gy as usize, self.grid_color);
            }
        }

        let range = (self.y_max - self.y_min).max(1.0);

        match self.chart_type {
            ChartType::Line => {
                for series in &self.series {
                    let n = series.values.len().max(1);
                    for i in 1..n {
                        let x0 = plot_x + (plot_w * (i - 1) as i32 / n.max(1) as i32);
                        let x1 = plot_x + (plot_w * i as i32 / n.max(1) as i32);
                        let y0 = plot_y + plot_h
                            - ((series.values[i - 1] - self.y_min) / range * plot_h as f32) as i32;
                        let y1 = plot_y + plot_h
                            - ((series.values[i] - self.y_min) / range * plot_h as f32) as i32;
                        fb.draw_line_aa(x0, y0, x1, y1, series.color);
                    }
                }
            }
            ChartType::Bar => {
                if let Some(series) = self.series.first() {
                    let n = series.values.len().max(1);
                    let bar_w = (plot_w / n as i32 - 2).max(1);
                    for (i, &v) in series.values.iter().enumerate() {
                        let bx = plot_x + (plot_w * i as i32 / n as i32) + 1;
                        let bh = ((v - self.y_min) / range * plot_h as f32) as i32;
                        let by = plot_y + plot_h - bh;
                        fb.fill_rect(Rect::new(bx, by, bar_w as u32, bh as u32), series.color);
                    }
                }
            }
            ChartType::Pie => {
                if let Some(series) = self.series.first() {
                    let cx = r.x + r.width as i32 / 2;
                    let cy = plot_y + plot_h / 2;
                    let radius = plot_h.min(plot_w) / 2 - 4;
                    let total: f32 = series.values.iter().sum::<f32>().max(0.001);
                    let pie_colors = [
                        Pixel::rgb(66, 133, 244),
                        Pixel::rgb(234, 67, 53),
                        Pixel::rgb(251, 188, 4),
                        Pixel::rgb(52, 168, 83),
                        Pixel::rgb(171, 71, 188),
                        Pixel::rgb(255, 112, 67),
                    ];
                    // Simple pie by filling circle segments
                    let mut angle_start = 0.0f32;
                    for (i, &v) in series.values.iter().enumerate() {
                        let sweep = v / total * 360.0;
                        let color = pie_colors[i % pie_colors.len()];
                        // Fill arc by iterating pixels in bounding box
                        for py in (cy - radius)..=(cy + radius) {
                            for px in (cx - radius)..=(cx + radius) {
                                let dx = (px - cx) as f32;
                                let dy = (py - cy) as f32;
                                if dx * dx + dy * dy <= (radius * radius) as f32 {
                                    let mut angle =
                                        libm::atan2f(dy, dx) * 180.0 / core::f32::consts::PI;
                                    if angle < 0.0 {
                                        angle += 360.0;
                                    }
                                    if angle >= angle_start && angle < angle_start + sweep {
                                        fb.blend_pixel(px as usize, py as usize, color);
                                    }
                                }
                            }
                        }
                        angle_start += sweep;
                    }
                }
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Rich Text Editor Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Text formatting span
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichTextStyle {
    Normal,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Heading1,
    Heading2,
}

/// A span of styled text
#[derive(Debug, Clone)]
pub struct RichSpan {
    pub text: String,
    pub style: RichTextStyle,
    pub color: Option<Pixel>,
}

/// Rich text editor widget
pub struct RichTextEditor {
    pub rect: Rect,
    pub spans: Vec<RichSpan>,
    pub cursor_pos: usize,
    pub scroll_y: i32,
    pub current_style: RichTextStyle,
    pub editable: bool,
    pub bg_color: Pixel,
}

impl RichTextEditor {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            spans: Vec::new(),
            cursor_pos: 0,
            scroll_y: 0,
            current_style: RichTextStyle::Normal,
            editable: true,
            bg_color: Pixel::new(25, 25, 35, 240),
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        self.spans.push(RichSpan {
            text: String::from(text),
            style: self.current_style,
            color: None,
        });
    }

    pub fn set_style(&mut self, style: RichTextStyle) {
        self.current_style = style;
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        fb.fill_rounded_rect_aa(self.rect, self.bg_color, 6);

        let mut x = self.rect.x + 8;
        let mut y = self.rect.y + 8 - self.scroll_y;
        let max_x = self.rect.x + self.rect.width as i32 - 8;

        for span in &self.spans {
            let color = span.color.unwrap_or(Pixel::rgb(220, 220, 240));
            let scale = match span.style {
                RichTextStyle::Heading1 => 2,
                RichTextStyle::Heading2 => 1,
                _ => 1,
            };

            for ch in span.text.chars() {
                if ch == '\n' || x >= max_x {
                    x = self.rect.x + 8;
                    y += 16 * scale;
                    if ch == '\n' {
                        continue;
                    }
                }
                if y >= self.rect.y && y < self.rect.y + self.rect.height as i32 - 8 {
                    fonts::draw_char(fb, x, y, ch, color, scale as u32);
                    if span.style == RichTextStyle::Underline {
                        for ux in x..x + 8 * scale {
                            fb.set_pixel(ux as usize, (y + 12 * scale) as usize, color);
                        }
                    }
                    if span.style == RichTextStyle::Strikethrough {
                        for sx in x..x + 8 * scale {
                            fb.set_pixel(sx as usize, (y + 6 * scale) as usize, color);
                        }
                    }
                }
                x += 8 * scale;
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Video Player Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Video player widget with transport controls
pub struct VideoPlayer {
    pub rect: Rect,
    pub state: PlaybackState,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8,
    pub file_path: String,
    pub frame_data: Vec<u8>,
    pub frame_width: u32,
    pub frame_height: u32,
}

impl VideoPlayer {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            state: PlaybackState::Stopped,
            position_ms: 0,
            duration_ms: 0,
            volume: 80,
            file_path: String::new(),
            frame_data: Vec::new(),
            frame_width: 0,
            frame_height: 0,
        }
    }

    pub fn play(&mut self) {
        self.state = PlaybackState::Playing;
    }
    pub fn pause(&mut self) {
        self.state = PlaybackState::Paused;
    }
    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.position_ms = 0;
    }

    pub fn seek(&mut self, ms: u64) {
        self.position_ms = ms.min(self.duration_ms);
    }

    fn format_time(ms: u64) -> String {
        let secs = ms / 1000;
        let m = secs / 60;
        let s = secs % 60;
        alloc::format!("{:02}:{:02}", m, s)
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let r = self.rect;
        // Video area (black background)
        fb.fill_rect(r, Pixel::rgb(0, 0, 0));

        // If we have frame data, blit it
        if !self.frame_data.is_empty() && self.frame_width > 0 && self.frame_height > 0 {
            let scale_x = r.width as f32 / self.frame_width as f32;
            let scale_y = (r.height - 40) as f32 / self.frame_height as f32;
            let scale = if scale_x < scale_y { scale_x } else { scale_y };
            let draw_w = (self.frame_width as f32 * scale) as u32;
            let draw_h = (self.frame_height as f32 * scale) as u32;
            let ox = r.x + (r.width as i32 - draw_w as i32) / 2;
            let oy = r.y + ((r.height - 40) as i32 - draw_h as i32) / 2;
            // Nearest-neighbor blit from RGBA data
            for py in 0..draw_h.min(r.height - 40) {
                let src_y = (py as f32 / scale) as usize;
                for px in 0..draw_w {
                    let src_x = (px as f32 / scale) as usize;
                    let idx = (src_y * self.frame_width as usize + src_x) * 4;
                    if idx + 3 < self.frame_data.len() {
                        let pixel = Pixel::new(
                            self.frame_data[idx],
                            self.frame_data[idx + 1],
                            self.frame_data[idx + 2],
                            self.frame_data[idx + 3],
                        );
                        fb.set_pixel((ox + px as i32) as usize, (oy + py as i32) as usize, pixel);
                    }
                }
            }
        } else {
            // No video — show placeholder
            let cx = r.x + r.width as i32 / 2;
            let cy = r.y + (r.height as i32 - 40) / 2;
            fb.fill_circle_aa(cx, cy, 30, Pixel::new(60, 60, 80, 200));
            // Play triangle
            if self.state != PlaybackState::Playing {
                for dy in -12..=12i32 {
                    let hw = (12 - dy.abs()) / 2;
                    for dx in -2..hw {
                        fb.set_pixel((cx + dx) as usize, (cy + dy) as usize, colors::WHITE);
                    }
                }
            }
        }

        // Transport bar
        let bar_y = r.y + r.height as i32 - 36;
        fb.fill_rect(
            Rect::new(r.x, bar_y, r.width, 36),
            Pixel::new(20, 20, 30, 220),
        );

        // Progress bar
        let prog_x = r.x + 80;
        let prog_w = r.width as i32 - 160;
        fb.fill_rect(
            Rect::new(prog_x, bar_y + 14, prog_w as u32, 4),
            Pixel::rgb(60, 60, 80),
        );
        if self.duration_ms > 0 {
            let fill = (self.position_ms as f32 / self.duration_ms as f32 * prog_w as f32) as i32;
            fb.fill_rect(
                Rect::new(prog_x, bar_y + 14, fill.max(0) as u32, 4),
                Pixel::rgb(80, 160, 255),
            );
        }

        // Time display
        let pos_str = Self::format_time(self.position_ms);
        let dur_str = Self::format_time(self.duration_ms);
        fonts::draw_string_compact(
            fb,
            r.x + 8,
            bar_y + 10,
            &pos_str,
            Pixel::rgb(200, 200, 220),
            1,
        );
        let time_right = alloc::format!("{}", dur_str);
        fonts::draw_string_compact(
            fb,
            r.x + r.width as i32 - 50,
            bar_y + 10,
            &time_right,
            Pixel::rgb(200, 200, 220),
            1,
        );

        // Play/Pause button
        let btn_cx = r.x + 50;
        let btn_cy = bar_y + 18;
        match self.state {
            PlaybackState::Playing => {
                fb.fill_rect(Rect::new(btn_cx - 6, btn_cy - 8, 4, 16), colors::WHITE);
                fb.fill_rect(Rect::new(btn_cx + 2, btn_cy - 8, 4, 16), colors::WHITE);
            }
            _ => {
                for dy in -8..=8i32 {
                    let hw = (8 - dy.abs()) / 2;
                    for dx in -2..hw {
                        fb.set_pixel(
                            (btn_cx + dx) as usize,
                            (btn_cy + dy) as usize,
                            colors::WHITE,
                        );
                    }
                }
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Syntax Highlighting Code Editor Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxLanguage {
    PlainText,
    Rust,
    C,
    Python,
    JavaScript,
    Shell,
    Toml,
    Json,
    Markdown,
}

/// Syntax-highlighted code editor widget
pub struct CodeEditor {
    pub rect: Rect,
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_y: usize,
    pub language: SyntaxLanguage,
    pub show_line_numbers: bool,
    pub tab_size: u8,
    pub bg_color: Pixel,
    pub gutter_color: Pixel,
}

impl CodeEditor {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            lines: alloc::vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            scroll_y: 0,
            language: SyntaxLanguage::PlainText,
            show_line_numbers: true,
            tab_size: 4,
            bg_color: Pixel::new(18, 18, 28, 255),
            gutter_color: Pixel::new(30, 30, 42, 255),
        }
    }

    pub fn set_content(&mut self, text: &str) {
        self.lines = text.lines().map(String::from).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.scroll_y = 0;
    }

    pub fn content(&self) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line);
        }
        out
    }

    pub fn detect_language(&mut self, filename: &str) {
        self.language = if filename.ends_with(".rs") {
            SyntaxLanguage::Rust
        } else if filename.ends_with(".c") || filename.ends_with(".h") {
            SyntaxLanguage::C
        } else if filename.ends_with(".py") {
            SyntaxLanguage::Python
        } else if filename.ends_with(".js") || filename.ends_with(".ts") {
            SyntaxLanguage::JavaScript
        } else if filename.ends_with(".sh") {
            SyntaxLanguage::Shell
        } else if filename.ends_with(".toml") {
            SyntaxLanguage::Toml
        } else if filename.ends_with(".json") {
            SyntaxLanguage::Json
        } else if filename.ends_with(".md") {
            SyntaxLanguage::Markdown
        } else {
            SyntaxLanguage::PlainText
        };
    }

    /// Get syntax color for a token
    fn token_color(&self, token: &str, in_string: bool, in_comment: bool) -> Pixel {
        if in_comment {
            return Pixel::rgb(106, 135, 89);
        }
        if in_string {
            return Pixel::rgb(206, 145, 120);
        }

        let keywords = match self.language {
            SyntaxLanguage::Rust => &[
                "fn", "let", "mut", "pub", "struct", "enum", "impl", "use", "mod", "if", "else",
                "for", "while", "loop", "match", "return", "self", "Self", "crate", "super",
                "true", "false", "const", "static", "unsafe", "async", "await", "trait", "where",
                "type",
            ] as &[&str],
            SyntaxLanguage::C => &[
                "int", "void", "char", "float", "double", "if", "else", "for", "while", "return",
                "struct", "typedef", "enum", "const", "static", "sizeof", "switch", "case",
                "break", "continue",
            ],
            SyntaxLanguage::Python => &[
                "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from",
                "as", "with", "try", "except", "raise", "True", "False", "None", "and", "or",
                "not", "in", "is", "lambda", "yield", "async", "await",
            ],
            SyntaxLanguage::JavaScript => &[
                "function",
                "const",
                "let",
                "var",
                "if",
                "else",
                "for",
                "while",
                "return",
                "class",
                "new",
                "this",
                "true",
                "false",
                "null",
                "undefined",
                "async",
                "await",
                "import",
                "export",
            ],
            _ => &[],
        };

        if keywords.contains(&token) {
            return Pixel::rgb(86, 156, 214); // Blue for keywords
        }

        // Numbers
        if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Pixel::rgb(181, 206, 168); // Green for numbers
        }

        Pixel::rgb(212, 212, 212) // Default foreground
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        fb.fill_rect(self.rect, self.bg_color);
        let gutter_w = if self.show_line_numbers { 48i32 } else { 0 };
        let line_h = 16;
        let visible_lines = (self.rect.height as usize / line_h as usize).max(1);

        // Gutter background
        if self.show_line_numbers {
            fb.fill_rect(
                Rect::new(self.rect.x, self.rect.y, gutter_w as u32, self.rect.height),
                self.gutter_color,
            );
        }

        for i in 0..visible_lines {
            let line_idx = self.scroll_y + i;
            if line_idx >= self.lines.len() {
                break;
            }
            let y = self.rect.y + (i as i32 * line_h);

            // Current line highlight
            if line_idx == self.cursor_line {
                fb.fill_rect(
                    Rect::new(
                        self.rect.x + gutter_w,
                        y,
                        self.rect.width - gutter_w as u32,
                        line_h as u32,
                    ),
                    Pixel::new(40, 40, 60, 128),
                );
            }

            // Line number
            if self.show_line_numbers {
                let num = alloc::format!("{:>4}", line_idx + 1);
                let num_color = if line_idx == self.cursor_line {
                    Pixel::rgb(200, 200, 220)
                } else {
                    Pixel::rgb(90, 90, 110)
                };
                fonts::draw_string_compact(fb, self.rect.x + 4, y + 2, &num, num_color, 1);
            }

            // Line text with syntax highlighting (simplified per-word coloring)
            let line = &self.lines[line_idx];
            let mut x = self.rect.x + gutter_w + 4;
            let mut in_comment = false;
            let mut in_string = false;

            for ch in line.chars() {
                if ch == '/' && line.contains("//") {
                    in_comment = true;
                }
                if ch == '#'
                    && (self.language == SyntaxLanguage::Python
                        || self.language == SyntaxLanguage::Shell)
                {
                    in_comment = true;
                }
                if ch == '"' || ch == '\'' {
                    in_string = !in_string;
                }

                let color = if in_comment {
                    Pixel::rgb(106, 135, 89)
                } else if in_string {
                    Pixel::rgb(206, 145, 120)
                } else if ch.is_ascii_digit() {
                    Pixel::rgb(181, 206, 168)
                } else if "{}()[]<>".contains(ch) {
                    Pixel::rgb(218, 218, 120)
                } else if "+-*/%=!&|^~".contains(ch) {
                    Pixel::rgb(180, 180, 180)
                } else {
                    Pixel::rgb(212, 212, 212)
                };

                fonts::draw_char(fb, x, y + 2, ch, color, 1);
                x += 8;
            }

            // Cursor
            if line_idx == self.cursor_line {
                let cx = self.rect.x + gutter_w + 4 + self.cursor_col as i32 * 8;
                fb.fill_rect(Rect::new(cx, y + 2, 2, 13), Pixel::rgb(200, 200, 255));
            }
        }
    }
}
