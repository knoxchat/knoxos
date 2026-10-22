// SPDX-License-Identifier: MIT
//! Knox UI theme: semantic tokens, Aurora / Arctic palettes, `ActiveTheme`.
//!
//! Palettes are Rust consts (no kit JSON watcher). `cx.theme().colors.primary`
//! is the coral accent on Aurora Dark.

use knox_gpui::{App, Hsla};

use crate::motion::MotionTokens;

mod tokens;

pub use tokens::{ColorTokens, RadiusTokens, ShadowToken, ShadowTokens, SpacingTokens, TypeTokens};

/// Light / dark appearance (kit `ThemeMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    #[default]
    Dark,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Active theme configuration installed as an [`App`] global.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub colors: ColorTokens,
    pub typo: TypeTokens,
    pub radius: RadiusTokens,
    pub spacing: SpacingTokens,
    pub shadow: ShadowTokens,
    pub motion: MotionTokens,
}

impl Default for Theme {
    fn default() -> Self {
        Self::aurora_dark()
    }
}

impl Theme {
    /// Aurora Dark — warm slate, coral accent (default).
    pub fn aurora_dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            colors: aurora_dark_colors(),
            typo: TypeTokens::knox(),
            radius: RadiusTokens::default(),
            spacing: SpacingTokens::default(),
            shadow: ShadowTokens::elevations(Hsla::BLACK.alpha(0.40)),
            motion: MotionTokens::default(),
        }
    }

    /// Arctic Light — warm paper, darker coral accent.
    pub fn arctic_light() -> Self {
        Self {
            mode: ThemeMode::Light,
            colors: arctic_light_colors(),
            typo: TypeTokens::knox(),
            radius: RadiusTokens::default(),
            spacing: SpacingTokens::default(),
            shadow: ShadowTokens::elevations(Hsla::BLACK.alpha(0.18)),
            motion: MotionTokens::default(),
        }
    }

    /// Sunset Warm — dark amber (kernel third palette; mode stays dark).
    pub fn sunset_warm() -> Self {
        Self {
            mode: ThemeMode::Dark,
            colors: sunset_warm_colors(),
            typo: TypeTokens::knox(),
            radius: RadiusTokens::default(),
            spacing: SpacingTokens::default(),
            shadow: ShadowTokens::elevations(Hsla::BLACK.alpha(0.45)),
            motion: MotionTokens::default(),
        }
    }

    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::aurora_dark(),
            ThemeMode::Light => Self::arctic_light(),
        }
    }

    /// Replace the App global (kit `Theme::change` role, palettes not JSON).
    pub fn change(cx: &mut App, mode: ThemeMode) {
        Self::install(cx, Self::for_mode(mode));
    }

    pub fn install(cx: &mut App, theme: Self) {
        cx.set_global(theme);
    }

    pub fn motion_tokens(&self) -> MotionTokens {
        self.motion
    }

    /// Zero-duration motion (Settings reduced-motion / D16.1).
    pub fn with_reduced_motion(mut self, on: bool) -> Self {
        if on {
            self.motion = MotionTokens::reduced();
        }
        self
    }

    /// High-contrast token set (D16.1). Ring stays a focus-visible yellow.
    pub fn with_high_contrast(mut self, on: bool) -> Self {
        if on {
            self.colors = high_contrast_colors(self.mode);
        }
        self
    }
}

/// `cx.theme()` — kit `ActiveTheme`.
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
            .expect("knox-ui Theme is not installed on App")
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Hsla {
    Hsla::from_rgb(r, g, b)
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Hsla {
    Hsla::from_rgba(r, g, b, a)
}

/// Map of kernel Aurora Dark `ThemeColors` onto kit semantic roles.
fn aurora_dark_colors() -> ColorTokens {
    ColorTokens {
        background: rgb(22, 20, 26),
        foreground: rgb(240, 232, 224),
        surface: rgb(44, 40, 50),
        surface_foreground: rgb(240, 232, 224),
        primary: rgb(232, 121, 100),
        primary_foreground: rgb(22, 20, 26),
        secondary: rgb(36, 33, 42),
        secondary_foreground: rgb(240, 232, 224),
        muted: rgb(55, 50, 62),
        muted_foreground: rgb(140, 130, 120),
        accent: rgb(70, 64, 80),
        accent_foreground: rgb(240, 232, 224),
        destructive: rgb(220, 85, 85),
        destructive_foreground: rgb(240, 232, 224),
        border: rgb(70, 65, 78),
        input: rgb(28, 26, 32),
        ring: rgb(232, 121, 100),
        selection: rgba(232, 121, 100, 50),
    }
}

fn arctic_light_colors() -> ColorTokens {
    ColorTokens {
        background: rgb(250, 246, 242),
        foreground: rgb(42, 36, 32),
        surface: rgb(255, 252, 248),
        surface_foreground: rgb(42, 36, 32),
        primary: rgb(200, 95, 75),
        primary_foreground: rgb(250, 246, 242),
        secondary: rgb(236, 228, 220),
        secondary_foreground: rgb(42, 36, 32),
        muted: rgb(244, 238, 232),
        muted_foreground: rgb(140, 130, 120),
        accent: rgb(238, 230, 222),
        accent_foreground: rgb(42, 36, 32),
        destructive: rgb(200, 50, 50),
        destructive_foreground: rgb(250, 246, 242),
        border: rgb(200, 192, 184),
        input: rgb(255, 255, 252),
        ring: rgb(200, 95, 75),
        selection: rgba(200, 95, 75, 35),
    }
}

fn high_contrast_colors(mode: ThemeMode) -> ColorTokens {
    match mode {
        ThemeMode::Dark => ColorTokens {
            background: rgb(0, 0, 0),
            foreground: rgb(255, 255, 255),
            surface: rgb(0, 0, 0),
            surface_foreground: rgb(255, 255, 255),
            primary: rgb(255, 220, 0),
            primary_foreground: rgb(0, 0, 0),
            secondary: rgb(0, 0, 0),
            secondary_foreground: rgb(255, 255, 255),
            muted: rgb(0, 0, 0),
            muted_foreground: rgb(255, 255, 255),
            accent: rgb(0, 0, 0),
            accent_foreground: rgb(255, 255, 255),
            destructive: rgb(255, 80, 80),
            destructive_foreground: rgb(255, 255, 255),
            border: rgb(255, 255, 255),
            input: rgb(0, 0, 0),
            ring: rgb(255, 220, 0),
            selection: rgba(255, 220, 0, 80),
        },
        ThemeMode::Light => ColorTokens {
            background: rgb(255, 255, 255),
            foreground: rgb(0, 0, 0),
            surface: rgb(255, 255, 255),
            surface_foreground: rgb(0, 0, 0),
            primary: rgb(0, 0, 180),
            primary_foreground: rgb(255, 255, 255),
            secondary: rgb(255, 255, 255),
            secondary_foreground: rgb(0, 0, 0),
            muted: rgb(255, 255, 255),
            muted_foreground: rgb(0, 0, 0),
            accent: rgb(255, 255, 255),
            accent_foreground: rgb(0, 0, 0),
            destructive: rgb(180, 0, 0),
            destructive_foreground: rgb(255, 255, 255),
            border: rgb(0, 0, 0),
            input: rgb(255, 255, 255),
            ring: rgb(0, 0, 180),
            selection: rgba(0, 0, 180, 60),
        },
    }
}

fn sunset_warm_colors() -> ColorTokens {
    ColorTokens {
        background: rgb(30, 22, 18),
        foreground: rgb(252, 240, 220),
        surface: rgb(60, 44, 32),
        surface_foreground: rgb(252, 240, 220),
        primary: rgb(230, 140, 65),
        primary_foreground: rgb(30, 22, 18),
        secondary: rgb(52, 38, 28),
        secondary_foreground: rgb(252, 240, 220),
        muted: rgb(75, 55, 40),
        muted_foreground: rgb(165, 145, 125),
        accent: rgb(105, 75, 48),
        accent_foreground: rgb(252, 240, 220),
        destructive: rgb(230, 75, 75),
        destructive_foreground: rgb(252, 240, 220),
        border: rgb(95, 75, 56),
        input: rgb(40, 30, 24),
        ring: rgb(230, 140, 65),
        selection: rgba(185, 125, 55, 80),
    }
}

#[cfg(test)]
mod tests {
    use super::{ActiveTheme, Theme, ThemeMode};
    use knox_gpui::{App, Hsla, px};

    /// D7.1 done-when: `cx.theme().colors.primary` is the coral accent.
    #[test]
    fn aurora_primary_is_coral_accent() {
        let mut cx = App::new();
        Theme::install(&mut cx, Theme::aurora_dark());
        let coral = Hsla::from_rgb(232, 121, 100).to_bgra();
        assert_eq!(cx.theme().colors.primary.to_bgra(), coral);
        assert_eq!(cx.theme().colors.ring.to_bgra(), coral);
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
    }

    /// D7.2 done-when (host): light/dark switch changes the palette.
    #[test]
    fn theme_mode_switch_follows_active_theme() {
        let mut cx = App::new();
        Theme::install(&mut cx, Theme::aurora_dark());
        assert!(cx.theme().mode.is_dark());

        Theme::change(&mut cx, ThemeMode::Light);
        assert_eq!(cx.theme().mode, ThemeMode::Light);
        assert_eq!(
            cx.theme().colors.primary.to_bgra(),
            Hsla::from_rgb(200, 95, 75).to_bgra()
        );
        assert!(
            cx.theme().colors.background.l > cx.theme().colors.foreground.l,
            "arctic paper is lighter than ink"
        );

        Theme::change(&mut cx, ThemeMode::Dark);
        assert_eq!(
            cx.theme().colors.primary.to_bgra(),
            Hsla::from_rgb(232, 121, 100).to_bgra()
        );
    }

    #[test]
    fn radius_and_spacing_match_kit_scale() {
        let r = super::RadiusTokens::default();
        assert_eq!(r.none, px(0));
        assert_eq!(r.sm, px(3));
        assert_eq!(r.md, px(6));
        assert_eq!(r.lg, px(8));
        assert_eq!(r.xl, px(12));
        assert_eq!(r.full, px(9999));
        let s = super::SpacingTokens::default();
        assert_eq!(s.xs, px(4));
        assert_eq!(s.sm, px(8));
        assert_eq!(s.md, px(12));
        assert_eq!(s.xl, px(24));
        let t = super::TypeTokens::knox();
        assert_eq!(t.caption, 14);
        assert_eq!(t.body, 15);
        assert_eq!(t.ui, 16);
        assert_eq!(t.title, 18);
        assert_eq!(t.desktop, 20);
        assert_eq!(t.display, 36);
    }

    #[test]
    fn high_contrast_uses_yellow_ring() {
        let t = Theme::aurora_dark().with_high_contrast(true);
        assert_eq!(
            t.colors.ring.to_bgra(),
            Hsla::from_rgb(255, 220, 0).to_bgra()
        );
        assert_eq!(
            t.colors.background.to_bgra(),
            Hsla::from_rgb(0, 0, 0).to_bgra()
        );
        let t = Theme::aurora_dark().with_high_contrast(false);
        assert_eq!(
            t.colors.primary.to_bgra(),
            Hsla::from_rgb(232, 121, 100).to_bgra()
        );
    }
}
