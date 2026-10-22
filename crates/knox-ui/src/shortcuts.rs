// SPDX-License-Identifier: MIT
//! Keyboard cheat-sheet overlay (D16.2). Lists D6.3 keymap actions.
//!
//! Zed GPUI / Longbridge kit are design references only (Apache-2.0).

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::overlay::Dialog;
use crate::theme::Theme;

/// One keymap row shown in the overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutRow {
    pub keys: &'static str,
    pub action: &'static str,
}

/// D6.3 bindings plus session help (`?`).
pub const D6_ACTIONS: &[ShortcutRow] = &[
    ShortcutRow {
        keys: "cmd-k",
        action: "command_palette",
    },
    ShortcutRow {
        keys: "?",
        action: "shortcuts",
    },
    ShortcutRow {
        keys: "escape",
        action: "dismiss",
    },
];

const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "GPUI",
        &[
            ("cmd-k", "command_palette"),
            ("?", "shortcuts"),
            ("escape", "dismiss"),
        ],
    ),
    (
        "General",
        &[
            ("Ctrl+C / Ctrl+V", "Copy / Paste"),
            ("Ctrl+/", "Show Shortcuts"),
            ("Alt+F4", "Close Window"),
        ],
    ),
    (
        "Windows",
        &[
            ("Alt+Tab", "Switch Windows"),
            ("Super+D", "Show Desktop"),
            ("Super+L", "Lock Screen"),
        ],
    ),
    (
        "System",
        &[
            ("Super+Space", "App Launcher"),
            ("Ctrl+Alt+T", "New Terminal"),
            ("Super+E", "File Explorer"),
        ],
    ),
];

/// Dialog cheat sheet (`?` / Ctrl+/).
pub struct ShortcutsOverlay {
    theme: Theme,
    open: bool,
    viewport_w: Pixels,
    viewport_h: Pixels,
    on_dismiss: Option<crate::interact::Handler>,
}

impl ShortcutsOverlay {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            open: false,
            viewport_w: px(800),
            viewport_h: px(600),
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn viewport(mut self, width: Pixels, height: Pixels) -> Self {
        self.viewport_w = width;
        self.viewport_h = height;
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for ShortcutsOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for ShortcutsOverlay {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut body = div().flex_col().gap_2();
        for (title, items) in GROUPS {
            let mut col = div().flex_col().gap_1();
            col = col.child(text(*title).color(self.theme.colors.primary).size(13));
            for (keys, action) in *items {
                col = col.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            text(*keys)
                                .color(self.theme.colors.muted_foreground)
                                .size(11),
                        )
                        .child(text(*action).color(self.theme.colors.foreground).size(12)),
                );
            }
            body = body.child(col);
        }
        let mut close = Button::new("shortcuts-close")
            .themed(&self.theme)
            .ghost()
            .min_w(px(72))
            .label("Close");
        if let Some(d) = self.on_dismiss.clone() {
            close = close.on_click(move || d());
        }
        body = body.child(close);
        let mut dlg = Dialog::new()
            .themed(&self.theme)
            .title("Keyboard Shortcuts")
            .open(self.open)
            .w(px(520))
            .viewport(self.viewport_w, self.viewport_h)
            .child(body);
        if let Some(d) = self.on_dismiss {
            dlg = dlg.on_dismiss(move || d());
        }
        dlg.into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{D6_ACTIONS, ShortcutsOverlay};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn lists_d63_command_palette() {
        assert!(
            D6_ACTIONS
                .iter()
                .any(|r| r.keys == "cmd-k" && r.action == "command_palette")
        );
        assert!(D6_ACTIONS.iter().any(|r| r.keys == "?"));
    }

    #[test]
    fn close_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let theme = Theme::aurora_dark();
        let mut el = ShortcutsOverlay::new()
            .themed(&theme)
            .open(true)
            .viewport(px(800), px(600))
            .on_dismiss(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(800, 600), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 16 && hb.bounds.height() <= 48)
            .max_by_key(|hb| (hb.bounds.y(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("close");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst), "close / dismiss");
    }
}
