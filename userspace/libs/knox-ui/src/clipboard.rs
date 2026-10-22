// SPDX-License-Identifier: MIT
//! Clipboard copy button (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. The host (kernel `gui/clipboard.rs`) owns the actual buffer.

use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{CursorStyle, Div, IntoElement, Styled, div, px};

use crate::icon::{Icon, IconName};
use crate::interact::{StringHandler, bind_activate};
use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

/// Icon button that copies `value` through `on_copy`.
pub struct Clipboard {
    theme: Theme,
    value: String,
    copied: bool,
    on_copy: Option<StringHandler>,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            value: String::new(),
            copied: false,
            on_copy: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    pub fn on_copy(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_copy = Some(Arc::new(f));
        self
    }

    pub fn value_ref(&self) -> &str {
        &self.value
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Clipboard {
    type Element = Div;

    fn into_element(self) -> Div {
        let icon = if self.copied {
            IconName::Check
        } else {
            IconName::Copy
        };
        let color = if self.copied {
            HslaOk::success()
        } else {
            self.theme.colors.muted_foreground
        };
        let mut el = div()
            .w(px(24))
            .h(px(24))
            .flex()
            .items_center()
            .justify_center()
            .rounded(self.theme.radius.sm)
            .cursor(CursorStyle::PointingHand)
            .hover(move |s| s.bg(self.theme.colors.accent))
            .child(
                Icon::new(icon)
                    .themed(&self.theme)
                    .color(color)
                    .with_size(ControlSize::Small),
            );
        if let Some(handler) = self.on_copy {
            let value = self.value;
            el = bind_activate(el, Arc::new(move || handler(&value)));
        }
        el
    }
}

struct HslaOk;
impl HslaOk {
    fn success() -> knox_gpui::Hsla {
        knox_gpui::Hsla::from_rgb(34, 160, 110)
    }
}

#[cfg(test)]
mod tests {
    use super::Clipboard;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element,
    };

    #[test]
    fn copy_fires_value() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = Clipboard::new()
            .value("knox")
            .on_copy(move |s| {
                assert_eq!(s, "knox");
                flag.store(true, Ordering::SeqCst);
            })
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(24, 24), &mut input);
        dispatch_mouse_down(&mut el, &mut input, Point::xy(8, 8));
        dispatch_mouse_up(&mut el, &mut input, Point::xy(8, 8));
        assert!(hit.load(Ordering::SeqCst));
    }
}
