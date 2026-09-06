/// Global Keyboard Shortcuts Configuration
///
/// Manages system-wide and per-application keyboard shortcuts.
/// Users can view, modify, and add custom key bindings.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Modifier keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1 << 0);
    pub const ALT: Self = Self(1 << 1);
    pub const SHIFT: Self = Self(1 << 2);
    pub const SUPER: Self = Self(1 << 3);

    pub fn contains(&self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// A key binding
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyBinding {
    pub modifiers: Modifiers,
    pub key: String,
}

impl KeyBinding {
    pub fn new(mods: Modifiers, key: &str) -> Self {
        Self {
            modifiers: mods,
            key: String::from(key),
        }
    }

    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.contains(Modifiers::SUPER) {
            parts.push("Super");
        }
        if self.modifiers.contains(Modifiers::CTRL) {
            parts.push("Ctrl");
        }
        if self.modifiers.contains(Modifiers::ALT) {
            parts.push("Alt");
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            parts.push("Shift");
        }
        parts.push(&self.key);
        let mut s = String::new();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                s.push('+');
            }
            s.push_str(p);
        }
        s
    }
}

/// Shortcut categories
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShortcutCategory {
    System,
    WindowManagement,
    Navigation,
    Applications,
    Custom,
}

/// A keyboard shortcut definition
#[derive(Debug, Clone)]
pub struct Shortcut {
    pub id: String,
    pub category: ShortcutCategory,
    pub description: String,
    pub binding: KeyBinding,
    pub default_binding: KeyBinding,
    pub enabled: bool,
}

lazy_static::lazy_static! {
    static ref SHORTCUTS: Mutex<Vec<Shortcut>> = Mutex::new(Vec::new());
}

/// Register a keyboard shortcut
pub fn register(id: &str, cat: ShortcutCategory, desc: &str, binding: KeyBinding) {
    SHORTCUTS.lock().push(Shortcut {
        id: String::from(id),
        category: cat,
        description: String::from(desc),
        default_binding: binding.clone(),
        binding,
        enabled: true,
    });
}

/// Change a shortcut's binding
pub fn rebind(id: &str, new_binding: KeyBinding) -> Result<(), &'static str> {
    let mut shortcuts = SHORTCUTS.lock();
    // Check for conflicts
    for s in shortcuts.iter() {
        if s.id != id && s.enabled && s.binding == new_binding {
            return Err("Binding conflicts with another shortcut");
        }
    }
    if let Some(s) = shortcuts.iter_mut().find(|s| s.id == id) {
        s.binding = new_binding;
        Ok(())
    } else {
        Err("Shortcut not found")
    }
}

/// Reset a shortcut to default
pub fn reset_to_default(id: &str) -> Result<(), &'static str> {
    let mut shortcuts = SHORTCUTS.lock();
    if let Some(s) = shortcuts.iter_mut().find(|s| s.id == id) {
        s.binding = s.default_binding.clone();
        Ok(())
    } else {
        Err("Shortcut not found")
    }
}

/// Find which shortcut matches a key press
pub fn find_match(mods: Modifiers, key: &str) -> Option<String> {
    let shortcuts = SHORTCUTS.lock();
    for s in shortcuts.iter() {
        if s.enabled && s.binding.modifiers == mods && s.binding.key == key {
            return Some(s.id.clone());
        }
    }
    None
}

/// Get all shortcuts in a category
pub fn list_category(cat: ShortcutCategory) -> Vec<Shortcut> {
    SHORTCUTS
        .lock()
        .iter()
        .filter(|s| s.category == cat)
        .cloned()
        .collect()
}

pub fn init() {
    // Register default system shortcuts
    register(
        "close-window",
        ShortcutCategory::WindowManagement,
        "Close window",
        KeyBinding::new(Modifiers::ALT, "F4"),
    );
    register(
        "switch-app",
        ShortcutCategory::System,
        "Switch application",
        KeyBinding::new(Modifiers::ALT, "Tab"),
    );
    register(
        "open-terminal",
        ShortcutCategory::Applications,
        "Open terminal",
        KeyBinding::new(Modifiers::CTRL.union(Modifiers::ALT), "T"),
    );
    register(
        "screenshot",
        ShortcutCategory::System,
        "Take screenshot",
        KeyBinding::new(Modifiers::NONE, "Print"),
    );
    register(
        "lock-screen",
        ShortcutCategory::System,
        "Lock screen",
        KeyBinding::new(Modifiers::SUPER, "L"),
    );
    register(
        "file-manager",
        ShortcutCategory::Applications,
        "Open file manager",
        KeyBinding::new(Modifiers::SUPER, "E"),
    );
    register(
        "maximize",
        ShortcutCategory::WindowManagement,
        "Maximize window",
        KeyBinding::new(Modifiers::SUPER, "Up"),
    );
    register(
        "minimize",
        ShortcutCategory::WindowManagement,
        "Minimize window",
        KeyBinding::new(Modifiers::SUPER, "Down"),
    );
    crate::serial_println!("[SHORTCUTS] Keyboard shortcuts config loaded");
}
