// SPDX-License-Identifier: MIT
//! Skeleton (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::theme::Theme;

/// Loading placeholder bar. `phase` is 0..=1; reduced-motion holds opacity.
pub struct Skeleton {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    secondary: bool,
    phase: f32,
    reduced_motion: bool,
}

impl Skeleton {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(120),
            height: px(12),
            secondary: false,
            phase: 0.0,
            reduced_motion: false,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn h(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn secondary(mut self) -> Self {
        self.secondary = true;
        self
    }

    pub fn phase(mut self, phase: f32) -> Self {
        self.phase = phase.clamp(0.0, 1.0);
        self
    }

    pub fn reduced_motion(mut self, on: bool) -> Self {
        self.reduced_motion = on;
        self
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Skeleton {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut bg = self.theme.colors.muted;
        if self.secondary {
            bg = bg.opacity(0.55);
        }
        let opacity = if self.reduced_motion {
            1.0
        } else {
            let wave = libm::sinf(self.phase * core::f32::consts::PI * 2.0);
            0.55 + 0.45 * (wave * 0.5 + 0.5)
        };
        div()
            .w(self.width)
            .h(self.height)
            .rounded(self.theme.radius.sm)
            .bg(bg)
            .opacity(opacity)
    }
}
