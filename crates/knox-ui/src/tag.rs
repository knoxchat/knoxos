// SPDX-License-Identifier: MIT
//! Tag (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;

use knox_gpui::{Div, Hsla, IntoElement, Styled, div, px, text};

use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

/// Semantic / custom chip variant (kit `TagVariant`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TagVariant {
    Primary,
    #[default]
    Secondary,
    Danger,
    Success,
    Warning,
    Info,
    Custom {
        color: Hsla,
        foreground: Hsla,
        border: Hsla,
    },
}

impl TagVariant {
    fn face(self, outline: bool, theme: &Theme) -> (Hsla, Hsla, Hsla) {
        let c = theme.colors;
        let success = Hsla::from_rgb(34, 160, 110);
        let warning = Hsla::from_rgb(210, 150, 50);
        let info = Hsla::from_rgb(70, 140, 210);
        let white = Hsla::from_rgb(250, 246, 242);
        match self {
            Self::Primary => {
                if outline {
                    (Hsla::TRANSPARENT, c.primary, c.primary)
                } else {
                    (c.primary, c.primary_foreground, c.primary)
                }
            }
            Self::Secondary => {
                if outline {
                    (Hsla::TRANSPARENT, c.muted_foreground, c.border)
                } else {
                    (c.secondary, c.secondary_foreground, c.border)
                }
            }
            Self::Danger => {
                if outline {
                    (Hsla::TRANSPARENT, c.destructive, c.destructive)
                } else {
                    (c.destructive, c.destructive_foreground, c.destructive)
                }
            }
            Self::Success => {
                if outline {
                    (Hsla::TRANSPARENT, success, success)
                } else {
                    (success, white, success)
                }
            }
            Self::Warning => {
                if outline {
                    (Hsla::TRANSPARENT, warning, warning)
                } else {
                    (warning, c.primary_foreground, warning)
                }
            }
            Self::Info => {
                if outline {
                    (Hsla::TRANSPARENT, info, info)
                } else {
                    (info, white, info)
                }
            }
            Self::Custom {
                color,
                foreground,
                border,
            } => {
                if outline {
                    (Hsla::TRANSPARENT, foreground, border)
                } else {
                    (color, foreground, border)
                }
            }
        }
    }
}

/// Small status chip (kit `Tag`).
pub struct Tag {
    theme: Theme,
    variant: TagVariant,
    outline: bool,
    size: ControlSize,
    label: String,
}

impl Tag {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            variant: TagVariant::Secondary,
            outline: false,
            size: ControlSize::Medium,
            label: String::new(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_variant(mut self, variant: TagVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn primary() -> Self {
        Self::new().with_variant(TagVariant::Primary)
    }

    pub fn secondary() -> Self {
        Self::new().with_variant(TagVariant::Secondary)
    }

    pub fn danger() -> Self {
        Self::new().with_variant(TagVariant::Danger)
    }

    pub fn success() -> Self {
        Self::new().with_variant(TagVariant::Success)
    }

    pub fn warning() -> Self {
        Self::new().with_variant(TagVariant::Warning)
    }

    pub fn info() -> Self {
        Self::new().with_variant(TagVariant::Info)
    }

    pub fn custom(color: Hsla, foreground: Hsla, border: Hsla) -> Self {
        Self::new().with_variant(TagVariant::Custom {
            color,
            foreground,
            border,
        })
    }

    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }
}

impl Default for Tag {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Tag {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl IntoElement for Tag {
    type Element = Div;

    fn into_element(self) -> Div {
        let (bg, fg, border) = self.variant.face(self.outline, &self.theme);
        let (pad_x, pad_y, font, radius) = match self.size {
            ControlSize::Small => (px(6), px(2), 10u16, self.theme.radius.sm),
            ControlSize::Large => (px(12), px(6), 13, self.theme.radius.md),
            ControlSize::Medium => (px(10), px(4), 11, self.theme.radius.md),
        };
        div()
            .flex()
            .items_center()
            .pad_x(pad_x)
            .pad_y(pad_y)
            .rounded(radius)
            .bg(bg)
            .border_1()
            .border_color(border)
            .hover(move |s| s.opacity(0.9))
            .child(text(self.label).color(fg).size(font))
    }
}
