// SPDX-License-Identifier: MIT
//! RTL text support (item 17.4)
//!
//! Implements bidirectional text (BiDi) algorithm for right-to-left
//! script rendering (Arabic, Hebrew, etc.) following Unicode Bidi Algorithm.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

static BIDI_RUNS: AtomicU64 = AtomicU64::new(0);

/// Bidi character type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiClass {
    /// Strong left-to-right (Latin, CJK, etc.)
    L,
    /// Strong right-to-left (Hebrew)
    R,
    /// Arabic letter (right-to-left)
    AL,
    /// European number
    EN,
    /// European separator
    ES,
    /// European terminator
    ET,
    /// Arabic number
    AN,
    /// Common separator
    CS,
    /// Nonspacing mark
    NSM,
    /// Boundary neutral
    BN,
    /// Paragraph separator
    B,
    /// Segment separator
    S,
    /// Whitespace
    WS,
    /// Other neutral
    ON,
    /// Left-to-right embedding
    LRE,
    /// Left-to-right override
    LRO,
    /// Right-to-left embedding
    RLE,
    /// Right-to-left override
    RLO,
    /// Pop directional format
    PDF,
    /// Left-to-right isolate
    LRI,
    /// Right-to-left isolate
    RLI,
    /// First strong isolate
    FSI,
    /// Pop directional isolate
    PDI,
}

/// Text direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

/// A directional run (contiguous characters with same direction)
#[derive(Debug, Clone)]
pub struct BidiRun {
    pub start: usize,
    pub end: usize,
    pub direction: TextDirection,
    pub level: u8,
}

/// Get the bidi class for a Unicode code point
pub fn bidi_class(c: char) -> BidiClass {
    let cp = c as u32;
    match cp {
        // Hebrew block
        0x0590..=0x05FF => BidiClass::R,
        // Arabic block
        0x0600..=0x06FF => BidiClass::AL,
        // Arabic Supplement
        0x0750..=0x077F => BidiClass::AL,
        // Arabic Extended
        0x08A0..=0x08FF => BidiClass::AL,
        // Thaana
        0x0780..=0x07BF => BidiClass::AL,
        // Syriac
        0x0700..=0x074F => BidiClass::R,
        // NKo
        0x07C0..=0x07FF => BidiClass::R,
        // ASCII digits
        0x0030..=0x0039 => BidiClass::EN,
        // Latin letters
        0x0041..=0x005A | 0x0061..=0x007A => BidiClass::L,
        // Common punctuation/symbols
        0x0020 => BidiClass::WS,
        0x000A | 0x000D => BidiClass::B,
        0x0009 => BidiClass::S,
        0x002C | 0x002E | 0x003A | 0x003B => BidiClass::CS,
        0x002B | 0x002D => BidiClass::ES,
        0x0023..=0x0025 | 0x00A2..=0x00A5 => BidiClass::ET,
        // Default: most other chars are left-to-right
        _ => {
            if cp < 0x0590 {
                BidiClass::L
            } else {
                BidiClass::ON
            }
        }
    }
}

/// Determine paragraph direction from first strong character
pub fn paragraph_direction(text: &str) -> TextDirection {
    for c in text.chars() {
        match bidi_class(c) {
            BidiClass::L => return TextDirection::LeftToRight,
            BidiClass::R | BidiClass::AL => return TextDirection::RightToLeft,
            _ => continue,
        }
    }
    TextDirection::LeftToRight // default
}

/// Simplified Unicode Bidirectional Algorithm
///
/// Splits text into directional runs suitable for rendering.
/// This implements a simplified version of UAX #9.
pub fn resolve_bidi(text: &str) -> Vec<BidiRun> {
    BIDI_RUNS.fetch_add(1, Ordering::Relaxed);

    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let para_dir = paragraph_direction(text);
    let para_level: u8 = match para_dir {
        TextDirection::LeftToRight => 0,
        TextDirection::RightToLeft => 1,
    };

    // Assign initial embedding levels
    let mut levels: Vec<u8> = Vec::with_capacity(chars.len());
    for c in &chars {
        let cls = bidi_class(*c);
        let level = match cls {
            BidiClass::R | BidiClass::AL => {
                if para_level % 2 == 0 {
                    1
                } else {
                    para_level
                }
            }
            BidiClass::L => {
                if para_level % 2 == 1 {
                    para_level + 1
                } else {
                    0
                }
            }
            BidiClass::EN | BidiClass::AN => para_level,
            _ => para_level,
        };
        levels.push(level);
    }

    // Build runs from contiguous same-level segments
    let mut runs = Vec::new();
    let mut run_start = 0;

    for i in 1..=chars.len() {
        if i == chars.len() || levels[i] != levels[run_start] {
            let direction = if levels[run_start] % 2 == 0 {
                TextDirection::LeftToRight
            } else {
                TextDirection::RightToLeft
            };
            runs.push(BidiRun {
                start: run_start,
                end: i,
                direction,
                level: levels[run_start],
            });
            if i < chars.len() {
                run_start = i;
            }
        }
    }

    runs
}

/// Reorder characters for visual display according to bidi runs
pub fn reorder_for_display(text: &str, runs: &[BidiRun]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());

    // Find max level for reordering
    let max_level = runs.iter().map(|r| r.level).max().unwrap_or(0);

    // Reverse runs at each level from max down to 0
    let mut visual_order: Vec<usize> = (0..runs.len()).collect();

    for level in (0..=max_level).rev() {
        let mut i = 0;
        while i < visual_order.len() {
            if runs[visual_order[i]].level >= level {
                // Find end of sequence at this level
                let start = i;
                while i < visual_order.len() && runs[visual_order[i]].level >= level {
                    i += 1;
                }
                // Reverse the subsequence
                visual_order[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }

    // Build result string in visual order
    for &run_idx in &visual_order {
        let run = &runs[run_idx];
        if run.direction == TextDirection::RightToLeft {
            // Reverse characters within RTL run
            for i in (run.start..run.end).rev() {
                if i < chars.len() {
                    // Mirror brackets for RTL
                    result.push(mirror_char(chars[i]));
                }
            }
        } else {
            for i in run.start..run.end {
                if i < chars.len() {
                    result.push(chars[i]);
                }
            }
        }
    }

    result
}

/// Mirror a character for RTL display (brackets, parens, etc.)
fn mirror_char(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        _ => c,
    }
}

/// Check if a character is an RTL character
pub fn is_rtl_char(c: char) -> bool {
    matches!(bidi_class(c), BidiClass::R | BidiClass::AL)
}

/// Check if a string contains any RTL characters
pub fn contains_rtl(text: &str) -> bool {
    text.chars().any(is_rtl_char)
}

/// Get the text direction for cursor positioning
pub fn cursor_direction(text: &str, cursor_pos: usize) -> TextDirection {
    let chars: Vec<char> = text.chars().collect();
    if cursor_pos < chars.len() {
        if is_rtl_char(chars[cursor_pos]) {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        }
    } else {
        paragraph_direction(text)
    }
}

pub fn stats() -> u64 {
    BIDI_RUNS.load(Ordering::Relaxed)
}

/// Initialize the RTL text subsystem
pub fn init() {
    crate::serial_println!("[rtl_text] bidirectional text support initialized");
}
