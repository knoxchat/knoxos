/// Text Selection — Highlight and copy text from GUI labels and widgets
///
/// Provides text selection capabilities:
///   - Click and drag to select text
///   - Double-click to select word
///   - Triple-click to select line
///   - Shift+click to extend selection
///   - Ctrl+A to select all
///   - Ctrl+C to copy selection to clipboard
///   - Visual highlight rendering
///   - Selection anchoring across redraws
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

/// Selection state for a text region
#[derive(Clone)]
pub struct TextSelection {
    /// Start position (character index)
    pub start: usize,
    /// End position (character index)  
    pub end: usize,
    /// Whether a selection is active
    pub active: bool,
    /// Anchor point for drag selection
    pub anchor: usize,
    /// The selected text content
    pub text: String,
    /// The full text being selected from
    pub source: String,
}

impl TextSelection {
    pub fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            active: false,
            anchor: 0,
            text: String::new(),
            source: String::new(),
        }
    }

    /// Begin a new selection at the given character index
    pub fn begin(&mut self, index: usize, source: &str) {
        self.anchor = index;
        self.start = index;
        self.end = index;
        self.active = true;
        self.source = String::from(source);
        self.text = String::new();
    }

    /// Extend selection to the given character index
    pub fn extend_to(&mut self, index: usize) {
        if !self.active {
            return;
        }
        if index < self.anchor {
            self.start = index;
            self.end = self.anchor;
        } else {
            self.start = self.anchor;
            self.end = index;
        }
        self.update_text();
    }

    /// Select a word at the given character index
    pub fn select_word(&mut self, index: usize, source: &str) {
        self.source = String::from(source);
        self.active = true;
        let bytes = source.as_bytes();

        // Find word start
        let mut start = index;
        while start > 0 && is_word_char(bytes[start - 1]) {
            start -= 1;
        }

        // Find word end
        let mut end = index;
        while end < bytes.len() && is_word_char(bytes[end]) {
            end += 1;
        }

        self.start = start;
        self.end = end;
        self.anchor = start;
        self.update_text();
    }

    /// Select an entire line
    pub fn select_line(&mut self, index: usize, source: &str) {
        self.source = String::from(source);
        self.active = true;
        let bytes = source.as_bytes();

        let mut start = index;
        while start > 0 && bytes[start - 1] != b'\n' {
            start -= 1;
        }

        let mut end = index;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }

        self.start = start;
        self.end = end;
        self.anchor = start;
        self.update_text();
    }

    /// Select all text
    pub fn select_all(&mut self, source: &str) {
        self.source = String::from(source);
        self.active = true;
        self.start = 0;
        self.end = source.len();
        self.anchor = 0;
        self.update_text();
    }

    /// Clear the selection
    pub fn clear(&mut self) {
        self.active = false;
        self.start = 0;
        self.end = 0;
        self.text = String::new();
    }

    /// Copy selection to clipboard
    pub fn copy_to_clipboard(&self) {
        if self.active && !self.text.is_empty() {
            crate::clipboard::copy_text(&self.text);
        }
    }

    /// Check if a character index is within the selection
    pub fn contains(&self, index: usize) -> bool {
        self.active && index >= self.start && index < self.end
    }

    /// Get the selected text
    pub fn selected_text(&self) -> &str {
        &self.text
    }

    /// Has any text selected?
    pub fn has_selection(&self) -> bool {
        self.active && self.start != self.end
    }

    fn update_text(&mut self) {
        if self.start < self.end && self.end <= self.source.len() {
            self.text = String::from(&self.source[self.start..self.end]);
        } else {
            self.text = String::new();
        }
    }
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Selection highlight color (semi-transparent blue)
const SELECTION_COLOR: Pixel = Pixel::new(0, 120, 215, 80);

/// Render a text selection highlight
pub fn draw_selection_highlight(
    fb: &mut FrameBuffer,
    selection: &TextSelection,
    x: i32,
    y: i32,
    font_width: usize,
    font_height: usize,
) {
    if !selection.has_selection() {
        return;
    }

    let start_px = x + (selection.start * font_width) as i32;
    let end_px = x + (selection.end * font_width) as i32;
    let width = (end_px - start_px).max(0) as u32;

    fb.fill_rect(
        Rect::new(start_px, y, width, font_height as u32),
        SELECTION_COLOR,
    );
}

/// Global selection state for the active widget
lazy_static::lazy_static! {
    pub static ref ACTIVE_SELECTION: Mutex<TextSelection> = Mutex::new(TextSelection::new());
}

/// Convert pixel coordinates to character index in a text line
pub fn pixel_to_char_index(click_x: i32, text_x: i32, font_width: usize) -> usize {
    let offset = (click_x - text_x).max(0) as usize;
    offset / font_width.max(1)
}
