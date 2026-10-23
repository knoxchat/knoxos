// SPDX-License-Identifier: MIT
//! Boot splash card (D12.3): Aurora surface + Progress / Tag.
//!
//! Rasterized by the kernel with `KnoxosTextSystem` and blitted onto the
//! framebuffer. Replaces the Python `gen_splash_type.py` packed bitmaps.

use alloc::format;
use alloc::string::String;

use knox_gpui::{Div, FontId, IntoElement, Pixels, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::progress::Progress;
use crate::separator::Separator;
use crate::size::Sizable;
use crate::tag::Tag;
use crate::theme::Theme;

/// Backing-store size of the centered splash card (outer aurora + inner panel).
pub const SPLASH_CARD_W: i32 = 560;
pub const SPLASH_CARD_H: i32 = 500;
const SPLASH_PANEL_W: i32 = 460;
const SPLASH_METER_W: i32 = 360;

/// Wordmark + mark used by splash, login, and lock.
pub fn brand_lockup(theme: &Theme, compact: bool) -> Div {
    let c = theme.colors;
    let typo = theme.typo;
    let (mark, icon, word) = if compact {
        (40, 20, 32u16)
    } else {
        (56, 28, 48)
    };
    let badge = div()
        .w(px(mark))
        .h(px(mark))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radius.full)
        .bg(c.primary.opacity(0.16))
        .border_1()
        .border_color(c.primary.opacity(0.50))
        .child(
            Icon::new(IconName::Sparkles)
                .themed(theme)
                .color(c.primary)
                .pixel_size(icon),
        );
    let mut col = div()
        .flex_col()
        .items_center()
        .gap(px(if compact { 8 } else { 14 }))
        .child(badge)
        .child(
            text("KnoxOS")
                .font(FontId::UiBold)
                .size(word)
                .color(c.foreground),
        );
    if !compact {
        col = col.child(
            text("SI-native Operating System")
                .font(FontId::Ui)
                .size(typo.ui)
                .color(c.muted_foreground),
        );
    }
    col
}

/// Status copy for a boot percent (0..=100).
pub fn splash_status(percent: u32) -> &'static str {
    match percent {
        0..=9 => "Starting kernel",
        10..=24 => "Mounting filesystem",
        25..=49 => "Starting processes",
        50..=74 => "Bringing up devices",
        75..=99 => "Loading drivers",
        _ => "Starting desktop",
    }
}

/// Centered boot splash (wordmark + progress + status).
pub struct Splash {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    progress: f32,
    version: String,
    status: String,
}

impl Splash {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(SPLASH_CARD_W),
            height: px(SPLASH_CARD_H),
            progress: 0.0,
            version: String::from("v0.2.6"),
            status: String::from(splash_status(0)),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn progress(mut self, value: f32) -> Self {
        self.progress = value.clamp(0.0, 100.0);
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }
}

impl Default for Splash {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Splash {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let typo = self.theme.typo;
        let pct = format!("{}%", self.progress as i32);
        let meter = div()
            .flex_col()
            .gap_1()
            .w(px(SPLASH_METER_W))
            .child(
                Progress::new()
                    .themed(&self.theme)
                    .value(self.progress)
                    .w(px(SPLASH_METER_W))
                    .large(),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .w(px(SPLASH_METER_W))
                    .child(
                        text(self.status.clone())
                            .size(typo.body)
                            .color(c.muted_foreground),
                    )
                    .child(text(pct).size(typo.body).color(c.muted_foreground)),
            );
        let panel = div()
            .w(px(SPLASH_PANEL_W))
            .min_h(px(400))
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20))
            .p(px(32))
            .bg(c.surface)
            .border_1()
            .border_color(c.border)
            .rounded(self.theme.radius.xl)
            .child(brand_lockup(&self.theme, false))
            .child(
                Tag::new()
                    .themed(&self.theme)
                    .outline()
                    .small()
                    .label(self.version),
            )
            .child(
                div()
                    .w(px(SPLASH_METER_W))
                    .child(Separator::horizontal().themed(&self.theme)),
            )
            .child(meter);
        div()
            .w(self.width)
            .h(self.height)
            .flex()
            .items_center()
            .justify_center()
            .bg(c.background)
            .child(panel)
    }
}

#[cfg(test)]
mod tests {
    use super::{SPLASH_CARD_H, SPLASH_CARD_W, Splash, splash_status};
    use crate::theme::Theme;
    use knox_gpui::{
        HostTextSystem, IntoElement, PaintEnv, Point, RasterBuffer, Scene, Size, SpriteAtlas,
        WindowInput, paint_element_with_input, px,
    };

    fn paint(progress: f32) -> RasterBuffer {
        let theme = Theme::aurora_dark();
        let mut el = Splash::new()
            .themed(&theme)
            .size(px(SPLASH_CARD_W), px(SPLASH_CARD_H))
            .progress(progress)
            .status(splash_status(progress as u32))
            .into_element();
        let mut atlas = SpriteAtlas::new();
        let sys = HostTextSystem;
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: Some(&sys),
        };
        let mut scene = Scene::new();
        let mut input = WindowInput::new();
        paint_element_with_input(
            &mut el,
            Point::ZERO,
            Size::wh(SPLASH_CARD_W, SPLASH_CARD_H),
            &mut scene,
            &mut input,
            &mut env,
        );
        let mut buf = RasterBuffer::new(SPLASH_CARD_W as usize, SPLASH_CARD_H as usize);
        buf.draw_with_atlas(&scene, Some(&atlas));
        buf
    }

    #[test]
    fn splash_card_has_ink() {
        let buf = paint(40.0);
        let mid = buf
            .pixel(SPLASH_CARD_W / 2, SPLASH_CARD_H / 2)
            .expect("pixel");
        assert!(mid.a > 0, "splash card must paint a surface");
    }

    #[test]
    fn splash_panel_is_raised_above_aurora() {
        let buf = paint(40.0);
        let corner = buf.pixel(2, 2).expect("corner");
        let mid = buf
            .pixel(SPLASH_CARD_W / 2, SPLASH_CARD_H / 2)
            .expect("mid");
        assert!(mid.a > 200, "inner panel must be opaque");
        assert!(
            mid.r > corner.r && mid.g > corner.g,
            "surface must sit above aurora (mid={mid:?} corner={corner:?})"
        );
    }

    #[test]
    fn splash_progress_paints_primary() {
        let buf = paint(40.0);
        let mut coral = 0u32;
        for y in 0..SPLASH_CARD_H {
            for x in 0..SPLASH_CARD_W {
                if let Some(p) = buf.pixel(x, y) {
                    if p.a > 180 && p.r > 180 && p.g > 70 && p.g < 170 && p.b < 150 {
                        coral += 1;
                    }
                }
            }
        }
        assert!(
            coral > 40,
            "progress / mark accent must paint coral, got {coral}"
        );
    }

    #[test]
    fn splash_status_tracks_percent() {
        assert_eq!(splash_status(0), "Starting kernel");
        assert_eq!(splash_status(10), "Mounting filesystem");
        assert_eq!(splash_status(100), "Starting desktop");
    }
}
