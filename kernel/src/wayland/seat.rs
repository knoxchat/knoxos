/// Seat (pointer/keyboard/touch) and pointer event enums.
use alloc::string::String;

// ─── Seat (input routing) ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum PointerButton {
    Left = 272,
    Right = 273,
    Middle = 274,
}

#[derive(Debug, Clone, Copy)]
pub enum PointerAxis {
    VerticalScroll,
    HorizontalScroll,
}

#[derive(Debug, Clone)]
pub struct Seat {
    pub name: String,
    pub has_pointer: bool,
    pub has_keyboard: bool,
    pub has_touch: bool,
    pub pointer_focus: Option<u32>,  // surface id
    pub keyboard_focus: Option<u32>, // surface id
    pub pointer_x: f64,
    pub pointer_y: f64,
    pub serial: u32,
}

impl Default for Seat {
    fn default() -> Self {
        Self::new()
    }
}

impl Seat {
    pub fn new() -> Self {
        Self {
            name: String::from("seat0"),
            has_pointer: true,
            has_keyboard: true,
            has_touch: false,
            pointer_focus: None,
            keyboard_focus: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
            serial: 1,
        }
    }

    pub fn next_serial(&mut self) -> u32 {
        self.serial += 1;
        self.serial
    }

    /// Record a pointer button for a focused client (Ozone `wl_pointer`).
    pub fn pointer_click(&mut self, _button: PointerButton, pressed: bool) {
        self.has_pointer = true;
        if pressed {
            let _ = self.next_serial();
        }
    }

    /// Record a key for a focused client (Ozone `wl_keyboard`).
    pub fn key_event(&mut self, _keycode: u32, pressed: bool) {
        self.has_keyboard = true;
        if pressed {
            let _ = self.next_serial();
        }
    }
}
