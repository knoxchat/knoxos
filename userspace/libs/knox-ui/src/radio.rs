// SPDX-License-Identifier: MIT
//! Radio and RadioGroup (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::interact::{BoolHandler, IndexHandler, bind_activate};
use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

/// Single exclusive option (kit `Radio`). Grouping is [`RadioGroup`].
pub struct Radio {
    id: String,
    theme: Theme,
    label: String,
    checked: bool,
    disabled: bool,
    size: ControlSize,
    on_change: Option<BoolHandler>,
}

impl Radio {
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

impl Sizable for Radio {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Radio {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Radio {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let side = self.size.indicator();
        let dot = (side / 2).max(6);
        let (disc_bg, disc_border) = if self.disabled {
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
            .rounded(self.theme.radius.full)
            .border_1()
            .border_color(disc_border)
            .bg(disc_bg)
            .flex()
            .items_center()
            .justify_center();
        if self.checked {
            indicator = indicator.child(
                div()
                    .w(px(dot))
                    .h(px(dot))
                    .rounded(self.theme.radius.full)
                    .bg(if self.disabled {
                        c.muted_foreground
                    } else {
                        c.primary_foreground
                    }),
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
                let next = true;
                el = bind_activate(el, Arc::new(move || on(next)));
            } else {
                el = el.focusable();
            }
        }
        el
    }
}

/// Exclusive set of radios (kit `RadioGroup`).
pub struct RadioGroup {
    id: String,
    theme: Theme,
    children: Vec<Radio>,
    selected: Option<usize>,
    vertical: bool,
    disabled: bool,
    on_change: Option<IndexHandler>,
}

impl RadioGroup {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            children: Vec::new(),
            selected: None,
            vertical: false,
            disabled: false,
            on_change: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn child(mut self, child: Radio) -> Self {
        self.children.push(child);
        self
    }

    pub fn selected(mut self, index: Option<usize>) -> Self {
        self.selected = index;
        self
    }

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    pub fn on_change(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Disableable for RadioGroup {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for RadioGroup {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut row = if self.vertical {
            div().flex_col()
        } else {
            div().flex()
        };
        row = row.gap_2().items_center();
        let group = self.on_change;
        for (i, mut radio) in self.children.into_iter().enumerate() {
            radio.theme = self.theme;
            radio.checked = self.selected == Some(i);
            if self.disabled {
                radio.disabled = true;
            }
            if let Some(g) = &group {
                let g = g.clone();
                let own = radio.on_change.clone();
                radio.on_change = Some(Arc::new(move |on| {
                    if let Some(own) = &own {
                        own(on);
                    }
                    g(i);
                }));
            }
            row = row.child(radio);
        }
        row
    }
}
