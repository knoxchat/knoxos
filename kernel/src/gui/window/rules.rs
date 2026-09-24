/// Window rules engine — per-app default size, position, workspace, floating
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::window_attrs::WindowLevel;

use super::types::{Window, WindowContentType};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Window Rules Engine — Per-app default size, position, workspace, floating
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// A rule that matches windows by content type or title substring and
/// applies default properties when the window is created.
#[derive(Clone)]
pub struct WindowRule {
    /// Match by content type (None = match all)
    pub match_content_type: Option<WindowContentType>,
    /// Match by title substring (empty = match all)
    pub match_title: String,
    /// Override default width
    pub width: Option<u32>,
    /// Override default height
    pub height: Option<u32>,
    /// Override default x position
    pub x: Option<i32>,
    /// Override default y position
    pub y: Option<i32>,
    /// Target workspace (None = current)
    pub workspace: Option<u8>,
    /// Start maximized
    pub maximized: bool,
    /// Force floating (override tiling mode)
    pub floating: bool,
    /// Window level override
    pub always_on_top: bool,
}

impl WindowRule {
    pub fn new() -> Self {
        Self {
            match_content_type: None,
            match_title: String::new(),
            width: None,
            height: None,
            x: None,
            y: None,
            workspace: None,
            maximized: false,
            floating: false,
            always_on_top: false,
        }
    }

    /// Check if this rule matches a given window
    pub fn matches(&self, content_type: WindowContentType, title: &str) -> bool {
        let type_match = self
            .match_content_type
            .is_none_or(|ct| ct as u8 == content_type as u8);
        let title_match = self.match_title.is_empty() || title.contains(self.match_title.as_str());
        type_match && title_match
    }
}

lazy_static::lazy_static! {
    /// Global window rules list
    pub static ref WINDOW_RULES: Mutex<Vec<WindowRule>> = Mutex::new(Vec::new());
}

/// Add a window rule
pub fn add_window_rule(rule: WindowRule) {
    WINDOW_RULES.lock().push(rule);
}

/// Remove all window rules
pub fn clear_window_rules() {
    WINDOW_RULES.lock().clear();
}

/// Apply matching window rules to a window before it's added to the WM.
/// Called from add_window().
pub(crate) fn apply_window_rules(window: &mut Window) {
    let rules = WINDOW_RULES.lock();
    for rule in rules.iter() {
        if rule.matches(window.content_type, &window.title) {
            if let Some(w) = rule.width {
                window.rect.width = w;
            }
            if let Some(h) = rule.height {
                window.rect.height = h;
            }
            if let Some(x) = rule.x {
                window.rect.x = x;
            }
            if let Some(y) = rule.y {
                window.rect.y = y;
            }
            if let Some(ws) = rule.workspace {
                window.workspace = ws;
            }
            if rule.always_on_top {
                window.window_level = WindowLevel::AlwaysOnTop;
            }
            // First matching rule wins
            break;
        }
    }
}
