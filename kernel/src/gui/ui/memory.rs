use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::id::Id;

use super::InputState;

// ═══════════════════════════════════════════════════════════════════════
// Persistent UI state — survives across frames
// ═══════════════════════════════════════════════════════════════════════

/// Per-widget persistent state entry.
#[derive(Clone, Debug)]
pub enum WidgetState {
    Bool(bool),
    I32(i32),
    U32(u32),
    String(String),
    ScrollOffset(i32, i32),
}

/// Persistent state store for all immediate-mode widgets.
/// Keyed by `Id`. Stored globally and persists across frames.
pub struct UiMemory {
    /// Widget states keyed by Id hash.
    entries: Vec<(u64, WidgetState)>,
    /// Which widget currently has keyboard focus.
    pub focused_id: Option<Id>,
    /// Focus last frame (for `gained_focus` / `lost_focus`).
    pub prev_focused_id: Option<Id>,
    /// Which widget was pressed on (for drag tracking).
    pub active_id: Option<Id>,
    /// Pointer position when active_id was pressed.
    pub active_start_x: i32,
    pub active_start_y: i32,
    /// Whether active_id has moved enough to count as a drag.
    pub active_is_dragging: bool,
    /// Pointer position last frame (for drag delta).
    pub prev_pointer_x: i32,
    pub prev_pointer_y: i32,
    /// Previous frame's primary button state (for press/release detection).
    pub prev_primary_down: bool,
    pub prev_secondary_down: bool,
}

impl UiMemory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            focused_id: None,
            prev_focused_id: None,
            active_id: None,
            active_start_x: 0,
            active_start_y: 0,
            active_is_dragging: false,
            prev_pointer_x: 0,
            prev_pointer_y: 0,
            prev_primary_down: false,
            prev_secondary_down: false,
        }
    }

    /// Get a state entry.
    pub fn get(&self, id: Id) -> Option<&WidgetState> {
        let key = id.value();
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// Set a state entry.
    pub fn set(&mut self, id: Id, state: WidgetState) {
        let key = id.value();
        if let Some(entry) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = state;
        } else {
            self.entries.push((key, state));
        }
    }

    /// Get a bool state, defaulting to `default` if not present.
    pub fn get_bool(&self, id: Id, default: bool) -> bool {
        match self.get(id) {
            Some(WidgetState::Bool(v)) => *v,
            _ => default,
        }
    }

    /// Set a bool state.
    pub fn set_bool(&mut self, id: Id, value: bool) {
        self.set(id, WidgetState::Bool(value));
    }

    /// Get an i32 state, defaulting to `default` if not present.
    pub fn get_i32(&self, id: Id, default: i32) -> i32 {
        match self.get(id) {
            Some(WidgetState::I32(v)) => *v,
            _ => default,
        }
    }

    /// Set an i32 state.
    pub fn set_i32(&mut self, id: Id, value: i32) {
        self.set(id, WidgetState::I32(value));
    }

    /// Get scroll offset for a scrollable region.
    pub fn get_scroll(&self, id: Id) -> (i32, i32) {
        match self.get(id) {
            Some(WidgetState::ScrollOffset(x, y)) => (*x, *y),
            _ => (0, 0),
        }
    }

    /// Set scroll offset.
    pub fn set_scroll(&mut self, id: Id, x: i32, y: i32) {
        self.set(id, WidgetState::ScrollOffset(x, y));
    }

    /// Get a String state.
    pub fn get_string(&self, id: Id) -> Option<&str> {
        match self.get(id) {
            Some(WidgetState::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Set a String state.
    pub fn set_string(&mut self, id: Id, value: String) {
        self.set(id, WidgetState::String(value));
    }

    /// Begin a new frame — update prev states.
    pub fn begin_frame(&mut self, input: &InputState) {
        self.prev_focused_id = self.focused_id;
        self.prev_pointer_x = input.pointer_x;
        self.prev_pointer_y = input.pointer_y;

        // Detect press/release transitions
        // (input already has these, so just track for drag logic)
        if !input.pointer_primary_down && self.prev_primary_down {
            // Released — clear active
            self.active_id = None;
            self.active_is_dragging = false;
        }
        self.prev_primary_down = input.pointer_primary_down;
        self.prev_secondary_down = input.pointer_secondary_down;
    }
}

lazy_static::lazy_static! {
    /// Global persistent UI memory — shared across all frames.
    pub static ref UI_MEMORY: spin::Mutex<UiMemory> = spin::Mutex::new(UiMemory::new());
}
