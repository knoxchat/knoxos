// SPDX-License-Identifier: MIT
//! NumberInput (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::button::{Button, ButtonVariants};
use crate::input::{FieldState, Input};
use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

type I64Handler = Arc<dyn Fn(i64) + Send + Sync>;
type FieldHandler = Arc<dyn Fn(FieldState) + Send + Sync>;

/// Numeric stepper wrapping an [`Input`].
pub struct NumberInput {
    id: String,
    theme: Theme,
    value: i64,
    min: i64,
    max: i64,
    step: i64,
    text: FieldState,
    size: ControlSize,
    disabled: bool,
    width: Pixels,
    on_change: Option<I64Handler>,
    on_text: Option<FieldHandler>,
}

impl NumberInput {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            value: 0,
            min: i64::MIN / 4,
            max: i64::MAX / 4,
            step: 1,
            text: FieldState::from("0"),
            size: ControlSize::Medium,
            disabled: false,
            width: px(120),
            on_change: None,
            on_text: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn value(mut self, value: i64) -> Self {
        self.value = value;
        self.text = FieldState::from(format!("{value}"));
        self
    }

    pub fn text(mut self, text: FieldState) -> Self {
        self.text = text;
        self
    }

    pub fn min(mut self, min: i64) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: i64) -> Self {
        self.max = max;
        self
    }

    pub fn step(mut self, step: i64) -> Self {
        self.step = step.max(1);
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn on_change(mut self, f: impl Fn(i64) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn on_text(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_text = Some(Arc::new(f));
        self
    }

    pub fn value_of(&self) -> i64 {
        self.value.clamp(self.min, self.max)
    }

    fn commit(
        value: i64,
        min: i64,
        max: i64,
        on_change: &Option<I64Handler>,
        on_text: &Option<FieldHandler>,
    ) {
        let value = value.clamp(min, max);
        if let Some(handler) = on_change {
            handler(value);
        }
        if let Some(handler) = on_text {
            handler(FieldState::from(format!("{value}")));
        }
    }
}

impl Sizable for NumberInput {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for NumberInput {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for NumberInput {
    type Element = Div;

    fn into_element(self) -> Div {
        let theme = self.theme;
        let min = self.min;
        let max = self.max;
        let step = self.step;
        let value = self.value.clamp(min, max);
        let on_change = self.on_change.clone();
        let on_text = self.on_text.clone();
        let minus = {
            let on_change = on_change.clone();
            let on_text = on_text.clone();
            Button::new(format!("{}-dec", self.id))
                .themed(&theme)
                .ghost()
                .label("-")
                .with_size(self.size)
                .disabled(self.disabled || value <= min)
                .on_click(move || {
                    NumberInput::commit(value.saturating_sub(step), min, max, &on_change, &on_text);
                })
        };
        let plus = {
            let on_change = on_change.clone();
            let on_text = on_text.clone();
            Button::new(format!("{}-inc", self.id))
                .themed(&theme)
                .ghost()
                .label("+")
                .with_size(self.size)
                .disabled(self.disabled || value >= max)
                .on_click(move || {
                    NumberInput::commit(value.saturating_add(step), min, max, &on_change, &on_text);
                })
        };
        let field = Input::new(format!("{}-in", self.id))
            .themed(&theme)
            .with_size(self.size)
            .disabled(self.disabled)
            .w(self.width)
            .state(self.text)
            .on_change({
                let on_change = on_change.clone();
                let on_text = on_text.clone();
                move |s| {
                    if let Some(handler) = &on_text {
                        handler(s.clone());
                    }
                    let raw = s.value();
                    let parsed = if raw.is_empty() || raw == "-" {
                        return;
                    } else {
                        raw.parse::<i64>().ok()
                    };
                    if let Some(n) = parsed {
                        if let Some(handler) = &on_change {
                            handler(n.clamp(min, max));
                        }
                    }
                }
            });

        div()
            .flex()
            .items_center()
            .gap_1()
            .child(minus)
            .child(field)
            .child(plus)
    }
}

#[cfg(test)]
mod tests {
    use super::NumberInput;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicI64, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element,
    };

    #[test]
    fn stepper_commits_value() {
        let n = Arc::new(AtomicI64::new(0));
        let c = n.clone();
        let mut el = NumberInput::new("n")
            .themed(&Theme::aurora_dark())
            .value(4)
            .min(0)
            .max(10)
            .on_change(move |v| {
                c.store(v, Ordering::SeqCst);
            })
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(200, 40), &mut input);
        // last interactive control is "+"
        let plus = input.hitboxes().last().copied().unwrap();
        let p = Point::xy(plus.bounds.x() + 4, plus.bounds.y() + 4);
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(n.load(Ordering::SeqCst), 5);
    }
}
