// SPDX-License-Identifier: MIT
//! Unified patch view (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, Hsla, IntoElement, Styled, div, px, text};

use crate::theme::Theme;

/// One displayed diff line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffLine {
    Hunk(String),
    Context(String),
    Add(String),
    Remove(String),
}

/// Parse a unified patch.
pub fn parse_unified(patch: &str) -> Vec<DiffLine> {
    let mut out = Vec::new();
    for line in patch.lines() {
        if line.starts_with("@@") {
            out.push(DiffLine::Hunk(String::from(line)));
        } else if let Some(rest) = line.strip_prefix('+') {
            if line.starts_with("+++") {
                out.push(DiffLine::Context(String::from(line)));
            } else {
                out.push(DiffLine::Add(String::from(rest)));
            }
        } else if let Some(rest) = line.strip_prefix('-') {
            if line.starts_with("---") {
                out.push(DiffLine::Context(String::from(line)));
            } else {
                out.push(DiffLine::Remove(String::from(rest)));
            }
        } else {
            let body = line.strip_prefix(' ').unwrap_or(line);
            out.push(DiffLine::Context(String::from(body)));
        }
    }
    out
}

/// Unified diff widget (kit `Diff`).
pub struct Diff {
    theme: Theme,
    lines: Vec<DiffLine>,
}

impl Diff {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            lines: Vec::new(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn patch(mut self, patch: &str) -> Self {
        self.lines = parse_unified(patch);
        self
    }

    pub fn lines(mut self, lines: Vec<DiffLine>) -> Self {
        self.lines = lines;
        self
    }
}

impl Default for Diff {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Diff {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().w(px(400));
        for line in self.lines {
            let (bg, fg, prefix, body) = match line {
                DiffLine::Hunk(s) => (
                    self.theme.colors.muted.opacity(0.5),
                    self.theme.colors.primary,
                    " ",
                    s,
                ),
                DiffLine::Add(s) => (
                    Hsla::from_rgb(40, 90, 50).opacity(0.45),
                    self.theme.colors.foreground,
                    "+",
                    s,
                ),
                DiffLine::Remove(s) => (
                    Hsla::from_rgb(120, 40, 40).opacity(0.45),
                    self.theme.colors.foreground,
                    "-",
                    s,
                ),
                DiffLine::Context(s) => (
                    knox_gpui::Hsla::TRANSPARENT,
                    self.theme.colors.muted_foreground,
                    " ",
                    s,
                ),
            };
            col = col.child(
                div()
                    .bg(bg)
                    .child(text(alloc::format!("{prefix}{body}")).color(fg).size(12)),
            );
        }
        col
    }
}

#[cfg(test)]
mod tests {
    use super::{Diff, DiffLine, parse_unified};
    use crate::theme::Theme;
    use knox_gpui::IntoElement;

    const PATCH: &str = "--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n context\n-old\n+new\n";

    #[test]
    fn fixture_patch_highlights_add_remove() {
        let lines = parse_unified(PATCH);
        assert!(
            lines
                .iter()
                .any(|l| matches!(l, DiffLine::Add(s) if s == "new"))
        );
        assert!(
            lines
                .iter()
                .any(|l| matches!(l, DiffLine::Remove(s) if s == "old"))
        );
        let _ = Diff::new()
            .themed(&Theme::aurora_dark())
            .patch(PATCH)
            .into_element();
    }
}
