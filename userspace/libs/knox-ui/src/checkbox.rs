// SPDX-License-Identifier: MIT
//! Checkbox (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::interact::{BoolHandler, bind_activate};
use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

/// Binary tick box (kit `Checkbox`).
pub struct Checkbox {
    id: String,
    theme: Theme,
    label: String,
    checked: bool,
    disabled: bool,
    size: ControlSize,
    on_change: Option<BoolHandler>,
}

impl Checkbox {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            label: String::new(),
            checked: false,
            disabled: false,
            size: ControlSize::Medium,
            on_change: None,
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

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn on_click(self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_change(f)
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Sizable for Checkbox {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Checkbox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Checkbox {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let next = !self.checked;
        let side = self.size.indicator();
        let (box_bg, box_border) = if self.disabled {
            (c.muted.opacity(0.45), c.border.opacity(0.5))
        } else if self.checked {
            (c.primary, c.primary)
        } else {
            (c.input, c.border)
        };
        let hover = if self.checked {
            c.primary.lighten(0.08)
        } else {
            c.muted
        };

        let mut indicator = div()
            .w(px(side))
            .h(px(side))
            .rounded(self.theme.radius.sm)
            .border_1()
            .border_color(box_border)
            .bg(box_bg)
            .flex()
            .items_center()
            .justify_center();
        if self.checked {
            let mark = if self.disabled {
                c.muted_foreground
            } else {
                c.primary_foreground
            };
            indicator = indicator.child(
                Icon::new(IconName::Check)
                    .themed(&self.theme)
                    .color(mark)
                    .pixel_size((side - 4).max(8)),
            );
        }

        let mut el = div().flex().items_center().gap_2().child(indicator);
        if !self.label.is_empty() {
            let fg = if self.disabled {
                c.muted_foreground
            } else {
                c.foreground
            };
            el = el.child(text(self.label).color(fg).size(self.size.font()));
        }
        if !self.disabled {
            el = el
                .hover(move |s| s.bg(hover.opacity(0.25)))
                .active(move |s| s.bg(hover.opacity(0.4)))
                .focus(move |s| s.border_color(c.ring));
            if let Some(on) = self.on_change {
                el = bind_activate(el, Arc::new(move || on(next)));
            } else {
                el = el.focusable();
            }
        }
        el
    }
}
