// SPDX-License-Identifier: MIT
//! Editor: gutter, folding, selection, caret, no_std highlighter.
//!
//! the Longbridge kit / Zed editor are design references only. This file is new
//! `no_std` code: no tree-sitter, no LSP crate.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::input::{FieldState, GapBuffer};
use crate::scroll::{ScrollArea, visible_range};
use crate::theme::Theme;

/// Syntax class for a span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    Normal,
    Keyword,
    Comment,
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "if", "else", "for", "while", "struct", "enum", "impl", "pub", "use",
    "return", "match", "const",
];

/// Keyword / comment highlighter for one line.
pub fn highlight_line(line: &str) -> Vec<(Range<usize>, Highlight)> {
    if let Some(at) = line.find("//") {
        let mut out = highlight_line(&line[..at]);
        out.push((at..line.len(), Highlight::Comment));
        return out;
    }
    let mut out = Vec::new();
    let mut start = 0;
    for (i, ch) in line.char_indices() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            continue;
        }
        if start < i {
            push_token(&mut out, line, start, i);
        }
        if !ch.is_whitespace() {
            out.push((i..i + ch.len_utf8(), Highlight::Normal));
        }
        start = i + ch.len_utf8();
    }
    if start < line.len() {
        push_token(&mut out, line, start, line.len());
    }
    out
}

fn push_token(out: &mut Vec<(Range<usize>, Highlight)>, line: &str, start: usize, end: usize) {
    let tok = &line[start..end];
    let kind = if KEYWORDS.contains(&tok) {
        Highlight::Keyword
    } else {
        Highlight::Normal
    };
    out.push((start..end, kind));
}

/// Split a buffer into lines (no trailing empty line unless the text ends with `\n`).
pub fn lines_of(text: &str) -> Vec<String> {
    if text.is_empty() {
        return alloc::vec![String::new()];
    }
    text.split('\n').map(String::from).collect()
}

/// Fold set: collapsed start lines.
#[derive(Clone, Debug, Default)]
pub struct FoldSet {
    pub starts: Vec<usize>,
}

impl FoldSet {
    pub fn toggle(&mut self, line: usize) {
        if let Some(i) = self.starts.iter().position(|l| *l == line) {
            self.starts.remove(i);
        } else {
            self.starts.push(line);
        }
    }

    pub fn is_folded(&self, line: usize) -> bool {
        self.starts.contains(&line)
    }
}

/// Simple editor chrome around a gap-buffer (D8.22).
pub struct Editor {
    theme: Theme,
    state: FieldState,
    caret_line: usize,
    caret_col: usize,
    folds: FoldSet,
    offset: i32,
    width: Pixels,
    height: Pixels,
    on_change: Option<Arc<dyn Fn(FieldState) + Send + Sync>>,
    on_offset: Option<Arc<dyn Fn(i32) + Send + Sync>>,
    on_fold: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            state: FieldState::empty(),
            caret_line: 0,
            caret_col: 0,
            folds: FoldSet::default(),
            offset: 0,
            width: px(420),
            height: px(160),
            on_change: None,
            on_offset: None,
            on_fold: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn state(mut self, state: FieldState) -> Self {
        self.state = state;
        self
    }

    pub fn caret(mut self, line: usize, col: usize) -> Self {
        self.caret_line = line;
        self.caret_col = col;
        self
    }

    pub fn folds(mut self, folds: FoldSet) -> Self {
        self.folds = folds;
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn on_change(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(Arc::new(f));
        self
    }

    pub fn on_fold(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_fold = Some(Arc::new(f));
        self
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Editor {
    type Element = Div;

    fn into_element(self) -> Div {
        let value = self.state.value();
        let lines = lines_of(&value);
        let row_h = 16;
        let range = visible_range(self.offset, self.height.0, row_h, lines.len());
        let mut col = div().flex_col();
        for i in range {
            if self.folds.is_folded(i) && i != self.caret_line {
                continue;
            }
            let line = lines.get(i).cloned().unwrap_or_default();
            let gutter = format!("{:>4}", i + 1);
            let mut row = div().flex().h(px(row_h)).child(
                text(gutter)
                    .color(self.theme.colors.muted_foreground)
                    .size(11),
            );
            let fold = self.on_fold.clone();
            row = row.child(
                div()
                    .w(px(12))
                    .child(
                        text(if self.folds.is_folded(i) { ">" } else { "v" })
                            .color(self.theme.colors.muted_foreground)
                            .size(11),
                    )
                    .on_click(move |_| {
                        if let Some(f) = &fold {
                            f(i);
                        }
                    }),
            );
            let spans = highlight_line(&line);
            let mut body = div().flex();
            if spans.is_empty() {
                let shown = if i == self.caret_line {
                    let mut s = line.clone();
                    let col = self.caret_col.min(s.len());
                    s.insert(col, '|');
                    s
                } else {
                    line
                };
                body = body.child(text(shown).color(self.theme.colors.foreground).size(12));
            } else {
                for (span, kind) in spans {
                    let color = match kind {
                        Highlight::Keyword => self.theme.colors.primary,
                        Highlight::Comment => self.theme.colors.muted_foreground,
                        Highlight::Normal => self.theme.colors.foreground,
                    };
                    let slice = line.get(span.clone()).unwrap_or("");
                    body = body.child(text(String::from(slice)).color(color).size(12));
                }
            }
            col = col.child(row.child(body));
        }
        let mut area = ScrollArea::new()
            .themed(&self.theme)
            .size(self.width, self.height)
            .offset(self.offset)
            .child(col);
        if let Some(on) = self.on_offset {
            area = area.on_offset(move |o| on(o));
        }
        area.into_element()
    }
}

/// Build a 10k-line buffer for the gallery host test.
pub fn many_lines(n: usize) -> FieldState {
    let mut buf = GapBuffer::from_str("fn main() {\n");
    for i in 0..n {
        buf.insert_str(&format!("    // line {i}\n"));
    }
    buf.insert_str("}\n");
    FieldState::from(buf.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Editor, FoldSet, Highlight, highlight_line, many_lines};
    use crate::theme::Theme;
    use knox_gpui::{IntoElement, px};

    #[test]
    fn ten_k_buffer_types_and_scrolls() {
        let state = many_lines(10_000);
        assert!(state.value().lines().count() > 10_000);
        let spans = highlight_line("fn foo() { // hi");
        assert!(spans.iter().any(|(_, k)| *k == Highlight::Keyword));
        assert!(spans.iter().any(|(_, k)| *k == Highlight::Comment));
        let mut folds = FoldSet::default();
        folds.toggle(0);
        assert!(folds.is_folded(0));
        let _ = Editor::new()
            .themed(&Theme::aurora_dark())
            .state(state)
            .height(px(80))
            .offset(0)
            .folds(folds)
            .into_element();
    }
}
