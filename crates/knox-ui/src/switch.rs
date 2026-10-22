// SPDX-License-Identifier: MIT
//! Switch (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{Div, FluentBuilder, IntoElement, Styled, div, px, text};

use crate::interact::{BoolHandler, bind_activate};
use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

/// Track + thumb on/off control (kit `Switch`).
pub struct Switch {
    id: String,
    theme: Theme,
    label: String,
    checked: bool,
    disabled: bool,
    size: ControlSize,
    on_change: Option<BoolHandler>,
}

impl Switch {
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

impl Sizable for Switch {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Switch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Switch {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let next = !self.checked;
        let (track_w, track_h, thumb) = self.size.switch_track();
        let track_bg = if self.disabled {
            c.muted.opacity(0.5)
        } else if self.checked {
            c.primary
        } else {
            c.muted
        };
        let thumb_bg = if self.disabled {
            c.muted_foreground
        } else {
            c.background
        };
        let hover = c.muted.opacity(0.40);
        let active = c.accent.opacity(0.55);

        let track = div()
            .w(px(track_w))
            .h(px(track_h))
            .rounded(self.theme.radius.full)
            .bg(track_bg)
            .pad_x(px(2))
            .items_center()
            .when(self.checked, |d| d.justify_end())
            .child(
                div()
                    .w(px(thumb))
                    .h(px(thumb))
                    .rounded(self.theme.radius.full)
                    .bg(thumb_bg),
            );

        let mut el = div().flex().items_center().gap_2().child(track);
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
                .hover(move |s| s.bg(hover))
                .active(move |s| s.bg(active))
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
