/// Foundational Input Types — Adapted from winit's event system
///
/// Provides structured enums for element states, mouse buttons, scroll deltas,
/// keyboard events, pointer sources, and touch phases. These replace raw booleans
/// and ad-hoc integer codes throughout the input system with a proper type-safe API.
///
/// Directly adapted from `winit/winit-core/src/event.rs`.
use alloc::string::String;

// ═══════════════════════════════════════════════════════════════════════
// ElementState — Pressed / Released
// ═══════════════════════════════════════════════════════════════════════

/// Describes a button or key state.
///
/// Replaces raw `bool` (true = pressed) with a self-documenting enum.
/// Adapted from `winit::event::ElementState`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ElementState {
    /// The button/key was pressed.
    Pressed,
    /// The button/key was released.
    Released,
}

impl ElementState {
    /// Returns `true` if the state is [`Pressed`](Self::Pressed).
    #[inline]
    pub fn is_pressed(self) -> bool {
        self == ElementState::Pressed
    }
}

impl From<bool> for ElementState {
    /// Convert from boolean: `true` → Pressed, `false` → Released.
    #[inline]
    fn from(pressed: bool) -> Self {
        if pressed {
            ElementState::Pressed
        } else {
            ElementState::Released
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MouseButton — Left / Right / Middle / Back / Forward / Other
// ═══════════════════════════════════════════════════════════════════════

/// Describes a mouse button.
///
/// Adapted from `winit::event::MouseButton`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left mouse button (primary).
    Left,
    /// Right mouse button (secondary / context).
    Right,
    /// Middle mouse button (scroll wheel click).
    Middle,
    /// Back button (side button, often "Browser Back").
    Back,
    /// Forward button (side button, often "Browser Forward").
    Forward,
    /// Other / extended button by number (0-based beyond the 5 standard).
    Other(u16),
}

impl MouseButton {
    /// Convert from a PS/2 button flags byte.
    /// Bit 0 = left, bit 1 = right, bit 2 = middle.
    pub fn from_ps2_flags(flags: u8, bit: u8) -> Option<Self> {
        match bit {
            0 => {
                if flags & 0x01 != 0 {
                    Some(MouseButton::Left)
                } else {
                    None
                }
            }
            1 => {
                if flags & 0x02 != 0 {
                    Some(MouseButton::Right)
                } else {
                    None
                }
            }
            2 => {
                if flags & 0x04 != 0 {
                    Some(MouseButton::Middle)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MouseScrollDelta — LineDelta / PixelDelta
// ═══════════════════════════════════════════════════════════════════════

/// Describes a difference in the mouse scroll wheel state.
///
/// Adapted from `winit::event::MouseScrollDelta`.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MouseScrollDelta {
    /// Discrete scroll by "lines" or "rows" (mouse wheel notches).
    /// Positive y = scroll up, negative y = scroll down.
    /// x is for horizontal scroll (tilt wheel).
    LineDelta(f32, f32),

    /// Continuous scroll by exact pixel amount (trackpad / touch).
    /// Positive y = scroll up (content moves down), negative y = scroll down.
    PixelDelta { x: f64, y: f64 },
}

impl MouseScrollDelta {
    /// Create a LineDelta from PS/2 scroll wheel (vertical only).
    /// scroll_z: positive = up, negative = down (PS/2 convention).
    #[inline]
    pub fn from_ps2_scroll(scroll_z: i8) -> Self {
        MouseScrollDelta::LineDelta(0.0, scroll_z as f32)
    }

    /// Get the vertical delta in "lines" (for LineDelta) or approximate lines (for PixelDelta).
    /// Returns positive for up, negative for down.
    #[inline]
    pub fn vertical_lines(&self) -> f32 {
        match self {
            MouseScrollDelta::LineDelta(_, y) => *y,
            MouseScrollDelta::PixelDelta { y, .. } => (*y / 20.0) as f32, // ~20px per line
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PointerSource — Mouse / Touch / Unknown
// ═══════════════════════════════════════════════════════════════════════

/// The source of a pointer event (which device generated it).
///
/// Adapted from `winit::event::PointerSource` / `ButtonSource`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PointerSource {
    /// A mouse device.
    Mouse,
    /// A touch screen.
    Touch {
        /// Unique finger identifier.
        finger_id: u32,
    },
    /// Source is unknown (e.g., VirtIO tablet).
    Unknown,
}

// ═══════════════════════════════════════════════════════════════════════
// TouchPhase — Started / Moved / Ended / Cancelled
// ═══════════════════════════════════════════════════════════════════════

/// Describes touch-point state changes.
///
/// Adapted from `winit::event::TouchPhase`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TouchPhase {
    /// A finger touched the surface.
    Started,
    /// A finger moved on the surface.
    Moved,
    /// A finger was lifted from the surface.
    Ended,
    /// The system cancelled tracking this touch.
    Cancelled,
}

// ═══════════════════════════════════════════════════════════════════════
// KeyLocation — Standard / Left / Right / Numpad
// ═══════════════════════════════════════════════════════════════════════

/// The physical location of a key on the keyboard.
///
/// Adapted from `winit::keyboard::KeyLocation`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum KeyLocation {
    /// The key is in the standard area (main block).
    Standard,
    /// The key is on the left side (e.g., Left Shift, Left Ctrl).
    Left,
    /// The key is on the right side (e.g., Right Shift, Right Ctrl).
    Right,
    /// The key is on the numpad.
    Numpad,
}

// ═══════════════════════════════════════════════════════════════════════
// KeyEvent — A keyboard key press/release
// ═══════════════════════════════════════════════════════════════════════

/// Describes a keyboard event.
///
/// Adapted from `winit::event::KeyEvent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// The PS/2 scancode (physical key code).
    pub scancode: u8,
    /// The logical character this key produces (if printable), e.g. 'a', '1', '/'.
    /// None for non-printable keys (Shift, Ctrl, etc.)
    pub logical_char: Option<char>,
    /// Human-readable key name (e.g., "A", "Enter", "LeftShift").
    pub key_name: KeyCode,
    /// Whether this is a key repeat event.
    pub repeat: bool,
    /// The state of the key (pressed or released).
    pub state: ElementState,
    /// Physical location on the keyboard.
    pub location: KeyLocation,
    /// Text produced by this key event (after dead keys / compose sequences).
    /// Only set on Pressed events that produce text.
    pub text: Option<char>,
}

// ═══════════════════════════════════════════════════════════════════════
// KeyCode — Named key identifiers
// ═══════════════════════════════════════════════════════════════════════

/// Named key identifiers for common keys.
///
/// Adapted from `winit::keyboard::NamedKey` and `keyboard_types::Code`.
/// This covers the most common keys; extend as needed.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum KeyCode {
    // ── Letters ──
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // ── Digits ──
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,

    // ── Function keys ──
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // ── Modifiers ──
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
    CapsLock,
    NumLock,
    ScrollLock,

    // ── Navigation ──
    Enter,
    Escape,
    Backspace,
    Tab,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,

    // ── Punctuation / Symbols ──
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Backquote,
    Comma,
    Period,
    Slash,

    // ── Numpad ──
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadEnter,
    NumpadDecimal,

    // ── Misc ──
    PrintScreen,
    Pause,
    ContextMenu,

    /// Unknown / unmapped scancode
    Unknown(u8),
}

impl KeyCode {
    /// Convert a PS/2 set-1 scancode (make code, without 0xE0 prefix) to a KeyCode.
    /// For extended keys (prefixed with 0xE0), pass `extended = true`.
    pub fn from_scancode(scancode: u8, extended: bool) -> Self {
        if extended {
            match scancode {
                0x1C => KeyCode::NumpadEnter,
                0x1D => KeyCode::ControlRight,
                0x35 => KeyCode::NumpadDivide,
                0x38 => KeyCode::AltRight,
                0x47 => KeyCode::Home,
                0x48 => KeyCode::ArrowUp,
                0x49 => KeyCode::PageUp,
                0x4B => KeyCode::ArrowLeft,
                0x4D => KeyCode::ArrowRight,
                0x4F => KeyCode::End,
                0x50 => KeyCode::ArrowDown,
                0x51 => KeyCode::PageDown,
                0x52 => KeyCode::Insert,
                0x53 => KeyCode::Delete,
                0x5B => KeyCode::SuperLeft,
                0x5C => KeyCode::SuperRight,
                0x5D => KeyCode::ContextMenu,
                _ => KeyCode::Unknown(scancode),
            }
        } else {
            match scancode {
                0x01 => KeyCode::Escape,
                0x02 => KeyCode::Digit1,
                0x03 => KeyCode::Digit2,
                0x04 => KeyCode::Digit3,
                0x05 => KeyCode::Digit4,
                0x06 => KeyCode::Digit5,
                0x07 => KeyCode::Digit6,
                0x08 => KeyCode::Digit7,
                0x09 => KeyCode::Digit8,
                0x0A => KeyCode::Digit9,
                0x0B => KeyCode::Digit0,
                0x0C => KeyCode::Minus,
                0x0D => KeyCode::Equal,
                0x0E => KeyCode::Backspace,
                0x0F => KeyCode::Tab,
                0x10 => KeyCode::Q,
                0x11 => KeyCode::W,
                0x12 => KeyCode::E,
                0x13 => KeyCode::R,
                0x14 => KeyCode::T,
                0x15 => KeyCode::Y,
                0x16 => KeyCode::U,
                0x17 => KeyCode::I,
                0x18 => KeyCode::O,
                0x19 => KeyCode::P,
                0x1A => KeyCode::BracketLeft,
                0x1B => KeyCode::BracketRight,
                0x1C => KeyCode::Enter,
                0x1D => KeyCode::ControlLeft,
                0x1E => KeyCode::A,
                0x1F => KeyCode::S,
                0x20 => KeyCode::D,
                0x21 => KeyCode::F,
                0x22 => KeyCode::G,
                0x23 => KeyCode::H,
                0x24 => KeyCode::J,
                0x25 => KeyCode::K,
                0x26 => KeyCode::L,
                0x27 => KeyCode::Semicolon,
                0x28 => KeyCode::Quote,
                0x29 => KeyCode::Backquote,
                0x2A => KeyCode::ShiftLeft,
                0x2B => KeyCode::Backslash,
                0x2C => KeyCode::Z,
                0x2D => KeyCode::X,
                0x2E => KeyCode::C,
                0x2F => KeyCode::V,
                0x30 => KeyCode::B,
                0x31 => KeyCode::N,
                0x32 => KeyCode::M,
                0x33 => KeyCode::Comma,
                0x34 => KeyCode::Period,
                0x35 => KeyCode::Slash,
                0x36 => KeyCode::ShiftRight,
                0x37 => KeyCode::NumpadMultiply,
                0x38 => KeyCode::AltLeft,
                0x39 => KeyCode::Space,
                0x3A => KeyCode::CapsLock,
                0x3B => KeyCode::F1,
                0x3C => KeyCode::F2,
                0x3D => KeyCode::F3,
                0x3E => KeyCode::F4,
                0x3F => KeyCode::F5,
                0x40 => KeyCode::F6,
                0x41 => KeyCode::F7,
                0x42 => KeyCode::F8,
                0x43 => KeyCode::F9,
                0x44 => KeyCode::F10,
                0x45 => KeyCode::NumLock,
                0x46 => KeyCode::ScrollLock,
                0x47 => KeyCode::Numpad7,
                0x48 => KeyCode::Numpad8,
                0x49 => KeyCode::Numpad9,
                0x4A => KeyCode::NumpadSubtract,
                0x4B => KeyCode::Numpad4,
                0x4C => KeyCode::Numpad5,
                0x4D => KeyCode::Numpad6,
                0x4E => KeyCode::NumpadAdd,
                0x4F => KeyCode::Numpad1,
                0x50 => KeyCode::Numpad2,
                0x51 => KeyCode::Numpad3,
                0x52 => KeyCode::Numpad0,
                0x53 => KeyCode::NumpadDecimal,
                0x57 => KeyCode::F11,
                0x58 => KeyCode::F12,
                _ => KeyCode::Unknown(scancode),
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Modifiers — Shift / Ctrl / Alt / Super with L/R distinction
// ═══════════════════════════════════════════════════════════════════════

/// The state of keyboard modifier keys.
///
/// Adapted from `winit::event::Modifiers` with per-key L/R tracking.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub shift_left: bool,
    pub shift_right: bool,
    pub ctrl_left: bool,
    pub ctrl_right: bool,
    pub alt_left: bool,
    pub alt_right: bool,
    pub super_left: bool,
    pub super_right: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl Modifiers {
    /// Returns true if any Shift key is held.
    #[inline]
    pub fn shift(&self) -> bool {
        self.shift_left || self.shift_right
    }
    /// Returns true if any Ctrl key is held.
    #[inline]
    pub fn ctrl(&self) -> bool {
        self.ctrl_left || self.ctrl_right
    }
    /// Returns true if any Alt key is held.
    #[inline]
    pub fn alt(&self) -> bool {
        self.alt_left || self.alt_right
    }
    /// Returns true if any Super/Meta key is held.
    #[inline]
    pub fn super_key(&self) -> bool {
        self.super_left || self.super_right
    }
    /// Returns true if no modifiers are held.
    #[inline]
    pub fn is_empty(&self) -> bool {
        !self.shift() && !self.ctrl() && !self.alt() && !self.super_key()
    }
}
