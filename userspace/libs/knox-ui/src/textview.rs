// SPDX-License-Identifier: MIT
//! TextView: no_std Markdown subset (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code: headings, lists, code, links. No HTML crate.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::theme::Theme;

/// Parsed markdown block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MdBlock {
    Heading(u8, String),
    Paragraph(String),
    List(Vec<String>),
    Code(String),
    Link { text: String, href: String },
}

/// Parse a small Markdown subset from VFS bytes / `&str`.
pub fn parse_markdown(src: &str) -> Vec<MdBlock> {
    let mut out = Vec::new();
    let mut list: Vec<String> = Vec::new();
    let mut code: Option<String> = None;
    let flush_list = |list: &mut Vec<String>, out: &mut Vec<MdBlock>| {
        if !list.is_empty() {
            out.push(MdBlock::List(core::mem::take(list)));
        }
    };
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("```") {
            if let Some(buf) = code.as_mut() {
                if rest.trim().is_empty() {
                    out.push(MdBlock::Code(core::mem::take(buf)));
                    code = None;
                } else {
                    buf.push('\n');
                    buf.push_str(line);
                }
            } else {
                flush_list(&mut list, &mut out);
                code = Some(String::new());
            }
            continue;
        }
        if let Some(buf) = code.as_mut() {
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(line);
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_list(&mut list, &mut out);
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            list.push(String::from(item));
            continue;
        }
        flush_list(&mut list, &mut out);
        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push(MdBlock::Heading(3, String::from(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push(MdBlock::Heading(2, String::from(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push(MdBlock::Heading(1, String::from(rest)));
        } else if let Some(inner) = parse_link(trimmed) {
            out.push(inner);
        } else {
            out.push(MdBlock::Paragraph(String::from(trimmed)));
        }
    }
    if let Some(buf) = code {
        out.push(MdBlock::Code(buf));
    }
    flush_list(&mut list, &mut out);
    out
}

fn parse_link(line: &str) -> Option<MdBlock> {
    let start = line.find('[')?;
    let mid = line[start..].find("](")?;
    let end = line[start + mid + 2..].find(')')?;
    let text = &line[start + 1..start + mid];
    let href = &line[start + mid + 2..start + mid + 2 + end];
    Some(MdBlock::Link {
        text: String::from(text),
        href: String::from(href),
    })
}

/// Markdown view (kit `TextView`).
pub struct TextView {
    theme: Theme,
    blocks: Vec<MdBlock>,
}

impl TextView {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            blocks: Vec::new(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn markdown(mut self, src: &str) -> Self {
        self.blocks = parse_markdown(src);
        self
    }

    pub fn blocks(mut self, blocks: Vec<MdBlock>) -> Self {
        self.blocks = blocks;
        self
    }
}

impl Default for TextView {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for TextView {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_2().w(px(360));
        for block in self.blocks {
            col = match block {
                MdBlock::Heading(level, s) => {
                    let size = match level {
                        1 => 18,
                        2 => 16,
                        _ => 14,
                    };
                    col.child(text(s).color(self.theme.colors.foreground).size(size))
                }
                MdBlock::Paragraph(s) => {
                    col.child(text(s).color(self.theme.colors.foreground).size(13))
                }
                MdBlock::List(items) => {
                    let mut list = div().flex_col().gap_1();
                    for item in items {
                        list = list.child(
                            text(alloc::format!("• {item}"))
                                .color(self.theme.colors.foreground)
                                .size(13),
                        );
                    }
                    col.child(list)
                }
                MdBlock::Code(s) => col.child(
                    div()
                        .p(self.theme.spacing.xs)
                        .bg(self.theme.colors.muted)
                        .rounded(self.theme.radius.sm)
                        .child(text(s).color(self.theme.colors.foreground).size(12)),
                ),
                MdBlock::Link { text: label, href } => col.child(
                    text(alloc::format!("{label} ({href})"))
                        .color(self.theme.colors.primary)
                        .size(13),
                ),
            };
        }
        col
    }
}

#[cfg(test)]
mod tests {
    use super::{MdBlock, TextView, parse_markdown};
    use crate::theme::Theme;
    use knox_gpui::IntoElement;

    const FIXTURE: &str =
        "# Title\n\nHello\n\n- one\n- two\n\n```\ncode\n```\n\n[Docs](knoxos://help)\n";

    #[test]
    fn renders_fixture_markdown() {
        let blocks = parse_markdown(FIXTURE);
        assert!(matches!(blocks.first(), Some(MdBlock::Heading(1, t)) if t == "Title"));
        assert!(blocks.iter().any(|b| matches!(b, MdBlock::List(_))));
        assert!(blocks.iter().any(|b| matches!(b, MdBlock::Code(_))));
        assert!(blocks.iter().any(|b| matches!(b, MdBlock::Link { .. })));
        let _ = TextView::new()
            .themed(&Theme::aurora_dark())
            .markdown(FIXTURE)
            .into_element();
    }
}
