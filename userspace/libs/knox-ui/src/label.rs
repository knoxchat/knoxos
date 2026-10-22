// SPDX-License-Identifier: MIT
//! Label (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use knox_gpui::{Div, FontId, Hsla, IntoElement, Styled, div, text};

use crate::theme::Theme;

const MASKED: char = '\u{2022}';

/// Match kind for [`Label::highlights`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HighlightsMatch {
    Prefix(String),
    Full(String),
}

impl HighlightsMatch {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Prefix(s) | Self::Full(s) => s,
        }
    }

    pub fn is_prefix(&self) -> bool {
        matches!(self, Self::Prefix(_))
    }
}

impl From<&str> for HighlightsMatch {
    fn from(value: &str) -> Self {
        Self::Full(String::from(value))
    }
}

impl From<String> for HighlightsMatch {
    fn from(value: String) -> Self {
        Self::Full(value)
    }
}

/// Text label with optional secondary line, masking, and search highlights.
pub struct Label {
    theme: Theme,
    label: String,
    secondary: Option<String>,
    masked: bool,
    highlights_text: Option<HighlightsMatch>,
    font: FontId,
    size_px: u16,
}

impl Label {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            label: label.into(),
            secondary: None,
            masked: false,
            highlights_text: None,
            font: FontId::Ui,
            size_px: 13,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn secondary(mut self, secondary: impl Into<String>) -> Self {
        self.secondary = Some(secondary.into());
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub fn highlights(mut self, text: impl Into<HighlightsMatch>) -> Self {
        self.highlights_text = Some(text.into());
        self
    }

    pub fn font(mut self, font: FontId) -> Self {
        self.font = font;
        self
    }

    pub fn size(mut self, size_px: u16) -> Self {
        self.size_px = size_px;
        self
    }

    pub fn full_text(&self) -> String {
        match &self.secondary {
            Some(secondary) => {
                let mut s = self.label.clone();
                s.push(' ');
                s.push_str(secondary);
                s
            }
            None => self.label.clone(),
        }
    }

    fn highlight_ranges(&self, total_length: usize) -> Vec<Range<usize>> {
        let mut ranges = Vec::new();
        let full_text = self.full_text();

        if self.secondary.is_some() {
            ranges.push(0..self.label.len());
            ranges.push(self.label.len()..total_length);
        }

        if let Some(matched) = &self.highlights_text {
            let matched_str = matched.as_str();
            if !matched_str.is_empty() {
                let search_lower = ascii_lower(matched_str);
                let full_lower = ascii_lower(&full_text);
                if matched.is_prefix() {
                    if full_lower.starts_with(search_lower.as_str()) {
                        ranges.push(0..matched_str.len().min(full_text.len()));
                    }
                } else {
                    let mut search_start = 0;
                    while search_start < full_lower.len() {
                        let rest = &full_lower[search_start..];
                        if let Some(pos) = rest.find(search_lower.as_str()) {
                            let match_start = search_start + pos;
                            let match_end = (match_start + matched_str.len()).min(full_text.len());
                            if match_end <= full_text.len() && match_start < match_end {
                                ranges.push(match_start..match_end);
                            }
                            search_start = match_start + 1;
                            while search_start < full_text.len()
                                && !full_text.is_char_boundary(search_start)
                            {
                                search_start += 1;
                            }
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        ranges
    }
}

fn ascii_lower(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

impl IntoElement for Label {
    type Element = Div;

    fn into_element(self) -> Div {
        let full = self.full_text();
        let chars = full.chars().count();
        let fg = self.theme.colors.foreground;
        let muted = self.theme.colors.muted_foreground;
        let accent = self.theme.colors.primary;
        let font = self.font;
        let size = self.size_px;

        if self.masked {
            let bullets: String = core::iter::repeat_n(MASKED, chars).collect();
            return div().child(text(bullets).color(fg).font(font).size(size));
        }

        let ranges = self.highlight_ranges(full.len());
        if ranges.is_empty() {
            return div().child(text(full).color(fg).font(font).size(size));
        }

        let mut marks: Vec<(usize, usize, Hsla)> = Vec::new();
        let mut skip = 0;
        if self.secondary.is_some() && ranges.len() >= 2 {
            marks.push((ranges[0].start, ranges[0].end, fg));
            marks.push((ranges[1].start, ranges[1].end, muted));
            skip = 2;
        }
        for r in ranges.iter().skip(skip) {
            marks.push((r.start, r.end, accent));
        }

        let mut colors = alloc::vec![fg; full.len()];
        for (start, end, color) in marks {
            for i in start.min(full.len())..end.min(full.len()) {
                colors[i] = color;
            }
        }

        let mut row = div().flex().items_center();
        let bytes = full.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !full.is_char_boundary(i) {
                i += 1;
                continue;
            }
            let color = colors[i];
            let mut j = i + 1;
            while j < bytes.len() {
                if full.is_char_boundary(j) && colors[j] != color {
                    break;
                }
                j += 1;
            }
            if let Ok(slice) = core::str::from_utf8(&bytes[i..j]) {
                row = row.child(text(slice).color(color).font(font).size(size));
            }
            i = j;
        }
        row
    }
}

#[cfg(test)]
mod tests {
    use super::{HighlightsMatch, Label};
    use alloc::string::String;

    #[test]
    fn highlight_ranges_secondary_and_full() {
        let label = Label::new("Hello").secondary("World");
        let total = "Hello World".len();
        let result = label.highlight_ranges(total);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], 0..5);
        assert_eq!(result[1], 5..11);

        let label = Label::new("Hello World").highlights("WORLD");
        let result = label.highlight_ranges("Hello World".len());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 6..11);

        let label = Label::new("Hello Hello Hello").highlights("Hello");
        let result = label.highlight_ranges("Hello Hello Hello".len());
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 0..5);
        assert_eq!(result[1], 6..11);
        assert_eq!(result[2], 12..17);
    }

    #[test]
    fn highlight_ranges_prefix() {
        let label = Label::new("aaaa").highlights(HighlightsMatch::Prefix(String::from("aa")));
        let result = label.highlight_ranges("aaaa".len());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0..2);

        let label =
            Label::new("Hello Hello").highlights(HighlightsMatch::Prefix(String::from("Hello")));
        let result = label.highlight_ranges("Hello Hello".len());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0..5);
    }

    #[test]
    fn masked_hides_highlights() {
        let label = Label::new("Hello").secondary("World").masked(true);
        assert_eq!(label.full_text(), "Hello World");
        assert!(label.masked);
    }
}
