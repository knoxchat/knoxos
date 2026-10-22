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

    /// KnoxOS binding string: `Ctrl+Alt+Shift+Super+K`.
    pub fn format(key: &Keystroke) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if key.modifiers.control {
            parts.push("Ctrl");
        }
        if key.modifiers.alt {
            parts.push("Alt");
        }
        if key.modifiers.shift {
            parts.push("Shift");
        }
        if key.modifiers.platform {
            parts.push("Super");
        }

        let key_label = match key.key {
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
        };

        if parts.is_empty() {
            return key_label;
        }
        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                out.push('+');
            }
            out.push_str(part);
        }
        out.push('+');
        out.push_str(&key_label);
        out
    }
}

impl From<Keystroke> for Kbd {
    fn from(stroke: Keystroke) -> Self {
        Self::new(stroke)
    }
}

impl IntoElement for Kbd {
    type Element = Div;

    fn into_element(self) -> Div {
        let label = Self::format(&self.stroke);
        if !self.appearance {
            return div().child(text(label).font(FontId::Mono).size(11));
        }
        let c = self.theme.colors;
        let (bg, fg, border) = if self.outline {
            (c.background, c.muted_foreground, c.border)
        } else {
            (c.muted, c.muted_foreground, c.muted)
        };
        let mut el = div()
            .h(px(20))
            .min_w(px(20))
            .pad_x(px(6))
            .flex()
            .items_center()
            .justify_center()
            .rounded(self.theme.radius.sm)
            .bg(bg)
            .child(text(label).color(fg).font(FontId::Mono).size(11));
        if self.outline {
            el = el.border_1().border_color(border);
        }
        el
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
    }
}
