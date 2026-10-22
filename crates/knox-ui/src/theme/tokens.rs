// SPDX-License-Identifier: MIT
//! Semantic design tokens (GPUI kit `theme_tokens.rs` roles, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code: integer [`Pixels`], no JSON loader, no `serde`.

use knox_gpui::{Hsla, Pixels, Point, px};

/// Kit `ColorTokens` roles. Product chrome uses these names, not `TASKBAR_*`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorTokens {
    pub background: Hsla,
    pub foreground: Hsla,
    pub surface: Hsla,
    pub surface_foreground: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub secondary: Hsla,
    pub secondary_foreground: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub destructive: Hsla,
    pub destructive_foreground: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub ring: Hsla,
    /// Translucent wash behind selected text.
    pub selection: Hsla,
}

/// Kit `RadiusTokens` scale (integer px).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadiusTokens {
    pub none: Pixels,
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
    pub xl: Pixels,
    pub full: Pixels,
}

impl Default for RadiusTokens {
    fn default() -> Self {
        Self {
            none: px(0),
            sm: px(3),
            md: px(6),
            lg: px(8),
            xl: px(12),
            full: px(9999),
        }
    }
}

/// Kit `SpacingTokens` scale (integer px).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpacingTokens {
    pub xxs: Pixels,
    pub xs: Pixels,
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
    pub xl: Pixels,
    pub xxl: Pixels,
}

impl Default for SpacingTokens {
    fn default() -> Self {
        Self {
            xxs: px(2),
            xs: px(4),
            sm: px(8),
            md: px(12),
            lg: px(16),
            xl: px(24),
            xxl: px(32),
        }
    }
}

/// One elevation (kit `BoxShadow` role, software raster).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowToken {
    pub offset: Point,
    pub blur: Pixels,
    pub color: Hsla,
}

/// Kit `ShadowTokens` elevations (`sm` / `md` / `lg`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowTokens {
    pub sm: ShadowToken,
    pub md: ShadowToken,
    pub lg: ShadowToken,
}

impl ShadowTokens {
    pub fn elevations(color: Hsla) -> Self {
        Self {
            sm: ShadowToken {
                offset: Point::xy(0, 1),
                blur: px(2),
                color,
            },
            md: ShadowToken {
                offset: Point::xy(0, 4),
                blur: px(12),
                color,
            },
            lg: ShadowToken {
                offset: Point::xy(0, 12),
                blur: px(24),
                color,
            },
        }
    }
}
