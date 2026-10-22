// SPDX-License-Identifier: MIT
//! Actions + keymap with a context string.
//!
//! GPUI `KeyBinding` / context predicates are the design reference
//! (Apache-2.0). This file is new `no_std` code: exact context match, no
//! `std` HashMap.

use alloc::vec::Vec;

use crate::input::{Keystroke, Modifiers};

/// Named action fired by a binding (`command_palette`, …).
pub type ActionName = &'static str;

/// One keystroke → action, only in `context`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub keystroke: Keystroke,
    pub action: ActionName,
    pub context: &'static str,
}

/// Ordered list of bindings. First match wins.
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn bind(&mut self, spec: &str, action: ActionName, context: &'static str) -> &mut Self {
        if let Some(keystroke) = Keystroke::parse(spec) {
            self.bindings.push(Binding {
                keystroke,
                action,
                context,
            });
        }
        self
    }

    pub fn bind_keystroke(
        &mut self,
        keystroke: Keystroke,
        action: ActionName,
        context: &'static str,
    ) -> &mut Self {
        self.bindings.push(Binding {
            keystroke,
            action,
            context,
        });
        self
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Action for `keystroke` in `context`, or `None` if no binding matches.
    pub fn action_for(&self, keystroke: &Keystroke, context: &str) -> Option<ActionName> {
        self.bindings.iter().find_map(|b| {
            if b.context == context && b.keystroke == *keystroke {
                Some(b.action)
            } else {
                None
            }
        })
    }
}

impl Keystroke {
    pub fn with_platform(mut self) -> Self {
        self.modifiers.platform = true;
        self
    }
}

impl Modifiers {
    pub const PLATFORM: Self = Self {
        control: false,
        alt: false,
        shift: false,
        platform: true,
    };
}

#[cfg(test)]
mod tests {
    use super::Keymap;
    use crate::input::{Key, Keystroke, Modifiers};

    /// D6.3 done-when: `cmd-k` fires only in the matching context.
    #[test]
    fn cmd_k_fires_only_in_matching_context() {
        let mut map = Keymap::new();
        map.bind("cmd-k", "command_palette", "Window");

        let cmd_k = Keystroke::parse("cmd-k").expect("parse cmd-k");
        assert_eq!(cmd_k.key, Key::Char('k'));
        assert!(cmd_k.modifiers.platform);
        assert!(!cmd_k.modifiers.control);

        assert_eq!(map.action_for(&cmd_k, "Window"), Some("command_palette"));
        assert_eq!(map.action_for(&cmd_k, "Launcher"), None);
        assert_eq!(map.action_for(&cmd_k, "TextInput"), None);

        let ctrl_k = Keystroke::new(Key::Char('k'), Modifiers::new(true, false, false, false));
        assert_eq!(map.action_for(&ctrl_k, "Window"), None);
    }
}
