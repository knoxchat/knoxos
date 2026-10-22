// SPDX-License-Identifier: MIT
//! Toggle and ToggleGroup (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::interact::{BoolHandler, bind_activate};
use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleVariant {
    #[default]
    Ghost,
    Outline,
}

pub trait ToggleVariants: Sized {
    fn with_variant(self, variant: ToggleVariant) -> Self;

    fn ghost(self) -> Self {
        self.with_variant(ToggleVariant::Ghost)
    }

    fn outline(self) -> Self {
        self.with_variant(ToggleVariant::Outline)
    }
}

/// Button-style latch (kit `Toggle`). Distinct from [`crate::Switch`].
pub struct Toggle {
    id: String,
    theme: Theme,
    label: String,
    checked: bool,
    disabled: bool,
    size: ControlSize,
    variant: ToggleVariant,
    on_change: Option<BoolHandler>,
}

impl Toggle {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            label: String::new(),
            checked: false,
            disabled: false,
            size: ControlSize::Medium,
            variant: ToggleVariant::Ghost,
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

impl ToggleVariants for Toggle {
    fn with_variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Sizable for Toggle {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Toggle {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Toggle {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let radius = self.theme.radius.md;
        let next = !self.checked;
        let (bg, fg, border, hover, active, has_border) = if self.disabled {
            (
                if self.checked {
                    c.muted
                } else {
                    knox_gpui::Hsla::TRANSPARENT
                },
                c.muted_foreground,
                c.border.opacity(0.5),
                c.muted,
                c.muted,
                matches!(self.variant, ToggleVariant::Outline),
            )
        } else if self.checked {
            (
                c.accent,
                c.accent_foreground,
                c.border,
                c.accent.lighten(0.06),
                c.accent.darken(0.06),
                matches!(self.variant, ToggleVariant::Outline),
            )
        } else {
            (
                knox_gpui::Hsla::TRANSPARENT,
                c.foreground,
                c.border,
                c.muted,
                c.accent,
                matches!(self.variant, ToggleVariant::Outline),
            )
        };

        let mut el = div()
            .h(px(self.size.height()))
            .pad_x(px(self.size.pad_x(false)))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius)
            .bg(bg);
        if has_border {
            el = el.border_1().border_color(border);
        }
        if !self.disabled {
            el = el
                .hover(move |s| s.bg(hover))
                .active(move |s| s.bg(active))
                .focus(move |s| s.border_color(c.ring));
        }
        if !self.label.is_empty() {
            el = el.child(text(self.label).color(fg).size(self.size.font()));
        }
        if !self.disabled {
            if let Some(on) = self.on_change {
                el = bind_activate(el, Arc::new(move || on(next)));
            } else {
                el = el.focusable();
            }
        }
        el
    }
}

/// Unstyled (visually: a flex row) group of toggles (kit `ToggleGroup`).
pub struct ToggleGroup {
    id: String,
    children: Vec<Toggle>,
    vertical: bool,
}

impl ToggleGroup {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            children: Vec::new(),
            vertical: false,
        }
    }

    pub fn child(mut self, child: Toggle) -> Self {
        self.children.push(child);
        self
    }

    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl IntoElement for ToggleGroup {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut row = if self.vertical {
            div().flex_col()
        } else {
            div().flex()
        };
        row = row.gap_1().items_center();
        for child in self.children {
            row = row.child(child);
        }
        row
    }
}
