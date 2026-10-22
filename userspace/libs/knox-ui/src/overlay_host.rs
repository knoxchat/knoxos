// SPDX-License-Identifier: MIT
//! Root overlay host: dialogs / menus / toasts above window content.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::theme::Theme;

/// Layers an overlay (and optional dim) above page content (D8.11).
pub struct OverlayHost {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    dim: bool,
    content: Option<Div>,
    overlay: Option<Div>,
}

impl OverlayHost {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(640),
            height: px(400),
            dim: false,
            content: None,
            overlay: None,
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

    pub fn dim(mut self, dim: bool) -> Self {
        self.dim = dim;
        self
    }

    pub fn content(mut self, child: impl IntoElement) -> Self {
        self.content = Some(div().child(child));
        self
    }

    pub fn overlay(mut self, child: impl IntoElement) -> Self {
        self.overlay = Some(div().child(child));
        self
    }
}

impl Default for OverlayHost {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for OverlayHost {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut root = div()
            .relative()
            .w(self.width)
            .h(self.height)
            .overflow_hidden();
        if let Some(content) = self.content {
            root = root.child(content);
        }
        if let Some(overlay) = self.overlay {
            let mut layer = div()
                .absolute()
                .left(px(0))
                .top(px(0))
                .w(self.width)
                .h(self.height)
                .flex()
                .items_center()
                .justify_center();
            if self.dim {
                layer = layer.bg(knox_gpui::Hsla::BLACK.alpha(0.45));
            }
            root = root.child(layer.child(overlay));
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::OverlayHost;
    use crate::button::Button;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, text,
    };

    #[test]
    fn dimmed_dialog_still_receives_input() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let theme = Theme::aurora_dark();
        let mut el = OverlayHost::new()
            .themed(&theme)
            .dim(true)
            .content(text("under"))
            .overlay(
                Button::new("in-dlg")
                    .themed(&theme)
                    .label("Ok")
                    .on_click(move || flag.store(true, Ordering::SeqCst)),
            )
            .into_element();
        let mut input = WindowInput::new();
        let size = Size::wh(640, 400);
        prepare_element(&mut el, Point::ZERO, size, &mut input);
        // Center of the 640×400 host is where the dialog card sits.
        let pos = Point::xy(320, 200);
        dispatch_mouse_down(&mut el, &mut input, pos);
        dispatch_mouse_up(&mut el, &mut input, pos);
        assert!(hit.load(Ordering::SeqCst));
    }
}
