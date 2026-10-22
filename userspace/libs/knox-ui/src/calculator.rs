// SPDX-License-Identifier: MIT
//! Calculator keypad + display (D11.1). Domain math stays in the kernel.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use alloc::string::String;

use knox_gpui::{Div, FontId, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::interact::CharHandler;
use crate::theme::Theme;

/// Matching the frozen in-kernel calculator grid.
pub const DISPLAY_H: i32 = 80;
pub const KEY_ROWS: &[&[(&str, char)]] = &[
    &[("C", 'C'), ("±", 'N'), ("%", '%'), ("÷", '/')],
    &[("7", '7'), ("8", '8'), ("9", '9'), ("×", '*')],
    &[("4", '4'), ("5", '5'), ("6", '6'), ("−", '-')],
    &[("1", '1'), ("2", '2'), ("3", '3'), ("+", '+')],
    &[("0", '0'), (".", '.'), ("⌫", 'B'), ("=", '=')],
];

/// Standard keypad view. Hover/active come from GPUI Button faces.
pub struct Calculator {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    display: String,
    history: String,
    on_key: Option<CharHandler>,
}

impl Calculator {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(320),
            height: px(400),
            display: String::from("0"),
            history: String::new(),
            on_key: None,
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

    pub fn display(mut self, value: impl Into<String>) -> Self {
        self.display = value.into();
        self
    }

    pub fn history(mut self, value: impl Into<String>) -> Self {
        self.history = value.into();
        self
    }

    pub fn on_key(mut self, f: impl Fn(char) + Send + Sync + 'static) -> Self {
        self.on_key = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for Calculator {
    fn default() -> Self {
        Self::new()
    }
}

fn clip_end(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[s.len() - max..]
    }
}

fn key_variant(label: &str) -> ButtonVariantsKind {
    match label {
        "C" | "±" | "%" => ButtonVariantsKind::Secondary,
        "÷" | "×" | "−" | "+" | "=" => ButtonVariantsKind::Primary,
        _ => ButtonVariantsKind::Default,
    }
}

enum ButtonVariantsKind {
    Default,
    Secondary,
    Primary,
}

impl IntoElement for Calculator {
    type Element = Div;

    fn into_element(self) -> Div {
        let on_key = self.on_key;
        let history = String::from(clip_end(&self.history, 36));
        let display = String::from(clip_end(&self.display, 18));

        let mut grid = div().flex_col().grow().gap(px(4)).p(self.theme.spacing.xs);
        for (ri, row) in KEY_ROWS.iter().enumerate() {
            let mut line = div().flex().grow().gap(px(4));
            for (ci, (label, code)) in row.iter().enumerate() {
                let code = *code;
                let handler = on_key.clone();
                let mut btn = Button::new(alloc::format!("calc-{ri}-{ci}"))
                    .themed(&self.theme)
                    .label(*label)
                    .fill();
                btn = match key_variant(label) {
                    ButtonVariantsKind::Primary => btn.primary(),
                    ButtonVariantsKind::Secondary => btn.secondary(),
                    ButtonVariantsKind::Default => btn,
                };
                if let Some(h) = handler {
                    btn = btn.on_click(move || h(code));
                }
                line = line.child(btn);
            }
            grid = grid.child(line);
        }

        div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.surface)
            .child(
                div()
                    .flex_col()
                    .justify_end()
                    .h(px(DISPLAY_H))
                    .p(self.theme.spacing.sm)
                    .gap_1()
                    .bg(self.theme.colors.background)
                    .child(
                        div().flex().justify_end().child(
                            text(history)
                                .color(self.theme.colors.muted_foreground)
                                .size(12),
                        ),
                    )
                    .child(
                        div().flex().justify_end().child(
                            text(display)
                                .font(FontId::UiBold)
                                .color(self.theme.colors.foreground)
                                .size(22),
                        ),
                    ),
            )
            .child(grid)
    }
}

#[cfg(test)]
mod tests {
    use super::{Calculator, DISPLAY_H, KEY_ROWS};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU32, Ordering};
    use knox_gpui::{
        Element, HostTextSystem, IntoElement, PaintEnv, Point, RasterBuffer, Scene, Size,
        SpriteAtlas, WindowInput, dispatch_mouse_down, dispatch_mouse_up, paint_element_with_input,
        px,
    };

    fn paint(
        mut el: knox_gpui::Div,
        origin: Point,
        size: Size,
        input: &mut WindowInput,
    ) -> RasterBuffer {
        let mut atlas = SpriteAtlas::new();
        let sys = HostTextSystem;
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: Some(&sys),
        };
        let mut scene = Scene::new();
        paint_element_with_input(&mut el, origin, size, &mut scene, input, &mut env);
        let mut buf =
            RasterBuffer::new(size.width.0.max(1) as usize, size.height.0.max(1) as usize);
        buf.draw_with_atlas(&scene, Some(&atlas));
        buf
    }

    #[test]
    fn key_press_fires_code() {
        let hit = Arc::new(AtomicU32::new(0));
        let id = hit.clone();
        let theme = Theme::aurora_dark();
        let size = Size::wh(320, 400);
        let mut el = Calculator::new()
            .themed(&theme)
            .size(px(320), px(400))
            .on_key(move |c| id.store(c as u32, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        el.request_layout(Point::ZERO, size);
        el.prepaint(el.computed(), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.y() >= DISPLAY_H && hb.bounds.height() >= 12)
            .min_by_key(|hb| (hb.bounds.y(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("key");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), KEY_ROWS[0][0].1 as u32);
    }

    #[test]
    fn keys_hover_changes_fill() {
        let theme = Theme::aurora_dark();
        let size = Size::wh(320, 400);
        let rest = paint(
            Calculator::new()
                .themed(&theme)
                .size(px(320), px(400))
                .into_element(),
            Point::ZERO,
            size,
            &mut WindowInput::new(),
        );
        let mut hover_in = WindowInput::new();
        let mut laid = Calculator::new()
            .themed(&theme)
            .size(px(320), px(400))
            .into_element();
        laid.request_layout(Point::ZERO, size);
        laid.prepaint(laid.computed(), &mut hover_in);
        let p = hover_in
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.y() >= DISPLAY_H && hb.bounds.height() >= 12)
            .min_by_key(|hb| (hb.bounds.y(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("key");
        hover_in.set_pointer_position(p);
        let hover = paint(
            Calculator::new()
                .themed(&theme)
                .size(px(320), px(400))
                .into_element(),
            Point::ZERO,
            size,
            &mut hover_in,
        );
        let a = rest.pixel(p.x.0, p.y.0).unwrap();
        let b = hover.pixel(p.x.0, p.y.0).unwrap();
        assert_ne!(a, b, "hovered key fill must differ");
        assert!(b.a > 0);
    }
}
