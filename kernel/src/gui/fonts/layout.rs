use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::metrics::{
    FONT_HEIGHT, FONT_HEIGHT_COMPACT, FONT_WIDTH, FONT_WIDTH_COMPACT, LINE_SPACING_EXTRA,
    LINE_SPACING_EXTRA_COMPACT,
};
use super::string::{draw_string, draw_string_compact};

/// Measure string width (standard font)
pub fn measure_string_width(text: &str, scale: u32) -> u32 {
    text.len() as u32 * FONT_WIDTH as u32 * scale
}

/// Measure string width (compact font)
pub fn measure_string_width_compact(text: &str, scale: u32) -> u32 {
    text.len() as u32 * FONT_WIDTH_COMPACT as u32 * scale
}

/// Measure string height (standard)
pub fn measure_string_height(scale: u32) -> u32 {
    FONT_HEIGHT as u32 * scale
}

/// Measure string height (compact)
pub fn measure_string_height_compact(scale: u32) -> u32 {
    FONT_HEIGHT_COMPACT as u32 * scale
}

/// Word-wrap text to fit within a given width.
pub fn word_wrap(text: &str, max_width: u32, scale: u32) -> alloc::vec::Vec<alloc::string::String> {
    let mut lines = alloc::vec::Vec::new();
    let char_width = FONT_WIDTH as u32 * scale;
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if max_chars == 0 {
        return lines;
    }
    for line in text.split('\n') {
        if line.len() <= max_chars {
            lines.push(alloc::string::String::from(line));
        } else {
            let words: alloc::vec::Vec<&str> = line.split_whitespace().collect();
            let mut current_line = alloc::string::String::new();
            for word in words {
                if current_line.is_empty() {
                    if word.len() > max_chars {
                        let mut remaining = word;
                        while remaining.len() > max_chars {
                            lines.push(alloc::string::String::from(&remaining[..max_chars]));
                            remaining = &remaining[max_chars..];
                        }
                        current_line = alloc::string::String::from(remaining);
                    } else {
                        current_line.push_str(word);
                    }
                } else if current_line.len() + 1 + word.len() <= max_chars {
                    current_line.push(' ');
                    current_line.push_str(word);
                } else {
                    lines.push(current_line);
                    current_line = alloc::string::String::from(word);
                }
            }
            if !current_line.is_empty() {
                lines.push(current_line);
            }
        }
    }
    lines
}

/// Draw text with word wrapping. Returns total height consumed.
pub fn draw_text_wrapped(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    max_width: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) -> i32 {
    let lines = word_wrap(text, max_width, scale);
    let line_h = (FONT_HEIGHT as u32 * scale) as i32 + LINE_SPACING_EXTRA;
    for (i, line) in lines.iter().enumerate() {
        draw_string(fb, x, y + i as i32 * line_h, line, color, scale);
    }
    lines.len() as i32 * line_h
}

/// Draw text with word wrapping (compact font).
pub fn draw_text_wrapped_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    max_width: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) -> i32 {
    let char_width = FONT_WIDTH_COMPACT as u32 * scale;
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if max_chars == 0 {
        return 0;
    }
    let mut lines = alloc::vec::Vec::new();
    for line in text.split('\n') {
        if line.len() <= max_chars {
            lines.push(alloc::string::String::from(line));
        } else {
            let words: alloc::vec::Vec<&str> = line.split_whitespace().collect();
            let mut current = alloc::string::String::new();
            for word in words {
                if current.is_empty() {
                    if word.len() > max_chars {
                        let mut rem = word;
                        while rem.len() > max_chars {
                            lines.push(alloc::string::String::from(&rem[..max_chars]));
                            rem = &rem[max_chars..];
                        }
                        current = alloc::string::String::from(rem);
                    } else {
                        current.push_str(word);
                    }
                } else if current.len() + 1 + word.len() <= max_chars {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    lines.push(current);
                    current = alloc::string::String::from(word);
                }
            }
            if !current.is_empty() {
                lines.push(current);
            }
        }
    }
    let line_h = (FONT_HEIGHT_COMPACT as u32 * scale) as i32 + LINE_SPACING_EXTRA_COMPACT;
    for (i, line) in lines.iter().enumerate() {
        draw_string_compact(fb, x, y + i as i32 * line_h, line, color, scale);
    }
    lines.len() as i32 * line_h
}

/// Truncate with ellipsis (standard font)
pub fn truncate_with_ellipsis(text: &str, max_width: u32, scale: u32) -> alloc::string::String {
    let char_width = FONT_WIDTH as u32 * scale;
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if text.len() <= max_chars || max_chars < 4 {
        return alloc::string::String::from(text);
    }
    let mut result = alloc::string::String::from(&text[..max_chars - 3]);
    result.push_str("...");
    result
}

/// Truncate with ellipsis (compact font)
pub fn truncate_with_ellipsis_compact(
    text: &str,
    max_width: u32,
    scale: u32,
) -> alloc::string::String {
    let char_width = FONT_WIDTH_COMPACT as u32 * scale;
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if text.len() <= max_chars || max_chars < 4 {
        return alloc::string::String::from(text);
    }
    let mut result = alloc::string::String::from(&text[..max_chars - 3]);
    result.push_str("...");
    result
}
