// ═══════════════════════════════════════════════════════════════════════
// Input snapshot — captured once per frame, read by all widgets
// ═══════════════════════════════════════════════════════════════════════

/// A snapshot of input state for the current frame.
/// Captured from the kernel's input module at the start of a frame.
#[derive(Clone, Debug)]
pub struct InputState {
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub pointer_primary_down: bool,
    pub pointer_secondary_down: bool,
    /// Was the primary button just pressed this frame (was up last frame)?
    pub pointer_primary_pressed: bool,
    /// Was the primary button just released this frame?
    pub pointer_primary_released: bool,
    /// Was the secondary button just pressed?
    pub pointer_secondary_pressed: bool,
    /// Was the secondary button just released?
    pub pointer_secondary_released: bool,
    /// Scroll wheel delta (positive = up/away from user)
    pub scroll_delta: i32,
    /// The last character typed (if any).
    pub char_typed: Option<char>,
    /// Whether backspace was pressed.
    pub backspace_pressed: bool,
    /// Whether Enter was pressed.
    pub enter_pressed: bool,
    /// Whether Tab was pressed.
    pub tab_pressed: bool,
    /// Whether Escape was pressed.
    pub escape_pressed: bool,
    /// Whether the left arrow key was pressed.
    pub left_pressed: bool,
    /// Whether the right arrow key was pressed.
    pub right_pressed: bool,
    /// Whether the up arrow key was pressed.
    pub up_pressed: bool,
    /// Whether the down arrow key was pressed.
    pub down_pressed: bool,
    /// Whether Home was pressed.
    pub home_pressed: bool,
    /// Whether End was pressed.
    pub end_pressed: bool,
    /// Whether Delete was pressed.
    pub delete_pressed: bool,
    /// Frame tick counter (for animations).
    pub frame_tick: u64,
    /// Double-click detected.
    pub double_click: bool,
}

impl InputState {
    /// Create a blank input state (no interaction).
    pub fn none() -> Self {
        Self {
            pointer_x: 0,
            pointer_y: 0,
            pointer_primary_down: false,
            pointer_secondary_down: false,
            pointer_primary_pressed: false,
            pointer_primary_released: false,
            pointer_secondary_pressed: false,
            pointer_secondary_released: false,
            scroll_delta: 0,
            char_typed: None,
            backspace_pressed: false,
            enter_pressed: false,
            tab_pressed: false,
            escape_pressed: false,
            left_pressed: false,
            right_pressed: false,
            up_pressed: false,
            down_pressed: false,
            home_pressed: false,
            end_pressed: false,
            delete_pressed: false,
            frame_tick: 0,
            double_click: false,
        }
    }
}
