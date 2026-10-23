// SPDX-License-Identifier: MIT
//! Kbd (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Shortcuts are formatted for KnoxOS (`Ctrl+Alt+Shift+Super`),
//! not AppKit/Win32 glyphs.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, FontId, IntoElement, Key, Keystroke, Styled, div, px, text};

use crate::theme::Theme;

/// Keyboard shortcut chip (kit `Kbd`).
pub struct Kbd {
    theme: Theme,
    stroke: Keystroke,
    appearance: bool,
    outline: bool,
}

impl Kbd {
    pub fn new(stroke: Keystroke) -> Self {
        Self {
            theme: Theme::default(),
            stroke,
            appearance: true,
            outline: false,
        }
    }

    pub fn parse(spec: &str) -> Option<Self> {
        Keystroke::parse(spec).map(Self::new)
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    pub fn keystroke(&self) -> Keystroke {
        self.stroke
    }

    /// Individual keycap labels: `Ctrl`, `Shift`, `S`.
    pub fn parts(key: &Keystroke) -> Vec<String> {
        let mut parts = Vec::new();
        if key.modifiers.control {
            parts.push(String::from("Ctrl"));
        }
        if key.modifiers.alt {
            parts.push(String::from("Alt"));
        }
        if key.modifiers.shift {
            parts.push(String::from("Shift"));
        }
        if key.modifiers.platform {
            parts.push(String::from("Super"));
        }
        parts.push(key_label(key.key));
        parts
    }

    /// KnoxOS binding string: `Ctrl+Alt+Shift+Super+K`.
    pub fn format(key: &Keystroke) -> String {
        let parts = Self::parts(key);
        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                out.push('+');
            }
            out.push_str(part);
        }
        out
    }
}

fn key_label(key: Key) -> String {
    match key {
        Key::Char(c) => {
            let mut s = String::new();
            s.push(c.to_ascii_uppercase());
            s
        }
        Key::Tab => String::from("Tab"),
        Key::Enter => String::from("Enter"),
        Key::Escape => String::from("Esc"),
        Key::Backspace => String::from("Backspace"),
        Key::Delete => String::from("Delete"),
        Key::Left => String::from("Left"),
        Key::Right => String::from("Right"),
        Key::Up => String::from("Up"),
        Key::Down => String::from("Down"),
        Key::Home => String::from("Home"),
        Key::End => String::from("End"),
        Key::PageUp => String::from("Page Up"),
        Key::PageDown => String::from("Page Down"),
        Key::Space => String::from("Space"),
    }
}

fn keycap(theme: &Theme, label: String, outline: bool) -> Div {
    let c = theme.colors;
    let (bg, fg, border) = if outline {
        (c.background, c.muted_foreground, c.border)
    } else {
        (c.muted, c.muted_foreground, c.border)
    };
    let mut el = div()
        .h(px(22))
        .min_w(px(22))
        .pad_x(px(6))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radius.sm)
        .bg(bg)
        .border_1()
        .border_color(border)
        .child(
            text(label)
                .color(fg)
                .font(FontId::Mono)
                .size(theme.typo.caption),
        );
    if !outline {
        el = el.border_color(bg);
    }
    el
}

impl From<Keystroke> for Kbd {
    fn from(stroke: Keystroke) -> Self {
        Self::new(stroke)
    }
}

impl IntoElement for Kbd {
    type Element = Div;

    fn into_element(self) -> Div {
        let parts = Self::parts(&self.stroke);
        if !self.appearance {
            return div().child(
                text(Self::format(&self.stroke))
                    .font(FontId::Mono)
                    .size(self.theme.typo.caption),
            );
        }
        let mut row = div().flex().items_center().gap_1();
        for part in parts {
            row = row.child(keycap(&self.theme, part, self.outline));
        }
        row
    }
}

#[cfg(test)]
mod tests {
    use super::Kbd;
    use knox_gpui::Keystroke;

    #[test]
    fn format_knoxos_modifiers() {
        assert_eq!(Kbd::format(&Keystroke::parse("a").unwrap()), "A");
        assert_eq!(Kbd::format(&Keystroke::parse("ctrl-a").unwrap()), "Ctrl+A");
        assert_eq!(Kbd::format(&Keystroke::parse("cmd-k").unwrap()), "Super+K");
        assert_eq!(
            Kbd::format(&Keystroke::parse("ctrl-alt-shift-cmd-s").unwrap()),
            "Ctrl+Alt+Shift+Super+S"
        );
        assert_eq!(
            Kbd::format(&Keystroke::parse("shift-tab").unwrap()),
            "Shift+Tab"
        );
        assert_eq!(
            Kbd::format(&Keystroke::parse("alt-enter").unwrap()),
            "Alt+Enter"
        );
        assert_eq!(
            Kbd::parts(&Keystroke::parse("ctrl-shift-s").unwrap()),
            alloc::vec![
                alloc::string::String::from("Ctrl"),
                alloc::string::String::from("Shift"),
                alloc::string::String::from("S")
            ]
        );
    }
}
