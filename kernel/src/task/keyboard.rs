/// Keyboard Input - Async keyboard scancode processing
/// Routes keyboard events to the focused terminal window or desktop actions
use conquer_once::spin::OnceCell;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use crossbeam_queue::ArrayQueue;
#[cfg(target_arch = "x86_64")]
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, PS2Keyboard, ScancodeSet1, layouts};

// ─── Stub types for non-x86_64 (no PS/2 keyboard) ──────────────────
#[cfg(not(target_arch = "x86_64"))]
mod pc_keyboard_stubs {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[allow(dead_code)]
    pub enum KeyCode {
        LControl,
        RControl,
        LShift,
        RShift,
        LAlt,
        RAltGr,
        LWin,
        RWin,
        Backspace,
        Tab,
        Return,
        NumpadEnter,
        Escape,
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
        Key1,
        Key2,
        Key3,
        Key4,
        Key5,
        Key6,
        Key7,
        Key8,
        Key9,
        Key0,
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
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        ArrowDown,
        Home,
        End,
        Delete,
        PageUp,
        PageDown,
        Oem2,
        Oem7,
        Unknown,
    }
    #[derive(Debug, Clone)]
    pub enum DecodedKey {
        Unicode(char),
        RawKey(KeyCode),
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum KeyState {
        Down,
        Up,
        SingleShot,
    }
    pub struct KeyEvent {
        pub code: KeyCode,
        pub state: KeyState,
    }
    pub struct ScancodeSet1;
    impl ScancodeSet1 {
        pub fn new() -> Self {
            ScancodeSet1
        }
    }
    pub mod layouts {
        pub struct Us104Key;
    }
    pub enum HandleControl {
        MapLettersToUnicode,
    }
    pub struct PS2Keyboard<L, S> {
        _l: core::marker::PhantomData<L>,
        _s: core::marker::PhantomData<S>,
    }
    impl<L, S> PS2Keyboard<L, S> {
        pub fn new(_set: S, _layout: L, _ctrl: HandleControl) -> Self {
            PS2Keyboard {
                _l: core::marker::PhantomData,
                _s: core::marker::PhantomData,
            }
        }
        pub fn add_byte(&mut self, _byte: u8) -> Result<Option<KeyEvent>, ()> {
            Ok(None)
        }
        pub fn process_keyevent(&mut self, _ev: KeyEvent) -> Option<DecodedKey> {
            None
        }
    }
}
#[cfg(not(target_arch = "x86_64"))]
use pc_keyboard_stubs as pc_keyboard;
#[cfg(not(target_arch = "x86_64"))]
use pc_keyboard_stubs::{DecodedKey, HandleControl, KeyCode, PS2Keyboard, ScancodeSet1, layouts};

use crate::serial_println;
use crate::terminal::TerminalKey;
use spin::Mutex;

use core::sync::atomic::{AtomicBool, Ordering};

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

/// Global Ctrl key state — used by GUI code (e.g. Ctrl+Click on URLs)
static CTRL_HELD_GLOBAL: AtomicBool = AtomicBool::new(false);

/// Check if Ctrl is currently held down
pub fn is_ctrl_held() -> bool {
    CTRL_HELD_GLOBAL.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════════
// P8.5 — Keyboard layout switching
// ═══════════════════════════════════════════════════════════════════════════

/// Available keyboard layouts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayout {
    Us,     // US QWERTY (default)
    Uk,     // UK QWERTY
    De,     // German QWERTZ
    Fr,     // French AZERTY
    Es,     // Spanish QWERTY
    Dvorak, // US Dvorak
}

impl KeyboardLayout {
    pub fn name(&self) -> &'static str {
        match self {
            KeyboardLayout::Us => "us",
            KeyboardLayout::Uk => "uk",
            KeyboardLayout::De => "de",
            KeyboardLayout::Fr => "fr",
            KeyboardLayout::Es => "es",
            KeyboardLayout::Dvorak => "dvorak",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "us" => Some(KeyboardLayout::Us),
            "uk" => Some(KeyboardLayout::Uk),
            "de" => Some(KeyboardLayout::De),
            "fr" => Some(KeyboardLayout::Fr),
            "es" => Some(KeyboardLayout::Es),
            "dvorak" => Some(KeyboardLayout::Dvorak),
            _ => None,
        }
    }

    /// Remap a character based on the current layout
    /// The pc_keyboard crate always decodes as US layout;
    /// we remap the output character for other layouts.
    pub fn remap(&self, ch: char, shift: bool) -> char {
        match self {
            KeyboardLayout::Us => ch,
            KeyboardLayout::De => match (ch, shift) {
                ('y', false) => 'z',
                ('z', false) => 'y',
                ('Y', true) | ('Y', false) => 'Z',
                ('Z', true) | ('Z', false) => 'Y',
                ('[', _) => 'ü',
                (']', _) => '+',
                (';', _) => 'ö',
                ('\'', _) => 'ä',
                ('-', false) => 'ß',
                _ => ch,
            },
            KeyboardLayout::Fr => match (ch, shift) {
                ('q', false) => 'a',
                ('a', false) => 'q',
                ('w', false) => 'z',
                ('z', false) => 'w',
                ('Q', true) | ('Q', false) => 'A',
                ('A', true) | ('A', false) => 'Q',
                ('W', true) | ('W', false) => 'Z',
                ('Z', true) | ('Z', false) => 'W',
                _ => ch,
            },
            KeyboardLayout::Dvorak => match (ch, shift) {
                ('q', false) => '\'',
                ('w', false) => ',',
                ('e', false) => '.',
                ('r', false) => 'p',
                ('t', false) => 'y',
                ('y', false) => 'f',
                ('u', false) => 'g',
                ('i', false) => 'c',
                ('o', false) => 'r',
                ('p', false) => 'l',
                ('s', false) => 'o',
                ('d', false) => 'e',
                ('f', false) => 'u',
                ('g', false) => 'i',
                ('h', false) => 'd',
                ('j', false) => 'h',
                ('k', false) => 't',
                ('l', false) => 'n',
                (';', false) => 's',
                ('z', false) => ';',
                ('x', false) => 'q',
                ('c', false) => 'j',
                ('v', false) => 'k',
                ('b', false) => 'x',
                ('n', false) => 'b',
                _ => ch,
            },
            _ => ch, // Uk, Es: mostly same as US with minor differences
        }
    }
}

lazy_static::lazy_static! {
    /// Current keyboard layout
    static ref CURRENT_LAYOUT: Mutex<KeyboardLayout> = Mutex::new(KeyboardLayout::Us);
}

/// Set the keyboard layout
pub fn set_layout(layout: KeyboardLayout) {
    *CURRENT_LAYOUT.lock() = layout;
    serial_println!("[KnoxOS] Keyboard layout set to: {}", layout.name());
}

/// Get the current keyboard layout
pub fn get_layout() -> KeyboardLayout {
    *CURRENT_LAYOUT.lock()
}

// ═══════════════════════════════════════════════════════════════════════════
// P8.6 — Compose / dead key support
// ═══════════════════════════════════════════════════════════════════════════

/// Compose key state machine for international character input
pub struct ComposeState {
    /// Whether we're in a compose sequence
    pub active: bool,
    /// First character of compose pair
    pub first: Option<char>,
}

impl ComposeState {
    pub fn new() -> Self {
        Self {
            active: false,
            first: None,
        }
    }

    /// Start a compose sequence (triggered by e.g. Right Alt + key or dedicated compose)
    pub fn begin(&mut self) {
        self.active = true;
        self.first = None;
    }

    /// Feed a character to the compose state machine
    /// Returns Some(composed_char) when a compose is complete, None if still composing
    pub fn feed(&mut self, ch: char) -> Option<char> {
        if !self.active {
            return None;
        }
        match self.first {
            None => {
                self.first = Some(ch);
                None // waiting for second char
            }
            Some(first) => {
                self.active = false;
                self.first = None;
                // Look up the compose pair
                Some(compose_lookup(first, ch))
            }
        }
    }

    /// Cancel the current compose sequence
    pub fn cancel(&mut self) {
        self.active = false;
        self.first = None;
    }
}

/// Look up a composed character from two input characters
fn compose_lookup(a: char, b: char) -> char {
    match (a, b) {
        // Acute accents
        ('\'', 'a') => 'á',
        ('\'', 'e') => 'é',
        ('\'', 'i') => 'í',
        ('\'', 'o') => 'ó',
        ('\'', 'u') => 'ú',
        ('\'', 'y') => 'ý',
        ('\'', 'A') => 'Á',
        ('\'', 'E') => 'É',
        ('\'', 'I') => 'Í',
        ('\'', 'O') => 'Ó',
        ('\'', 'U') => 'Ú',
        ('\'', 'Y') => 'Ý',
        // Grave accents
        ('`', 'a') => 'à',
        ('`', 'e') => 'è',
        ('`', 'i') => 'ì',
        ('`', 'o') => 'ò',
        ('`', 'u') => 'ù',
        ('`', 'A') => 'À',
        ('`', 'E') => 'È',
        ('`', 'I') => 'Ì',
        ('`', 'O') => 'Ò',
        ('`', 'U') => 'Ù',
        // Circumflex
        ('^', 'a') => 'â',
        ('^', 'e') => 'ê',
        ('^', 'i') => 'î',
        ('^', 'o') => 'ô',
        ('^', 'u') => 'û',
        ('^', 'A') => 'Â',
        ('^', 'E') => 'Ê',
        ('^', 'I') => 'Î',
        ('^', 'O') => 'Ô',
        ('^', 'U') => 'Û',
        // Diaeresis / umlaut
        ('"', 'a') => 'ä',
        ('"', 'e') => 'ë',
        ('"', 'i') => 'ï',
        ('"', 'o') => 'ö',
        ('"', 'u') => 'ü',
        ('"', 'y') => 'ÿ',
        ('"', 'A') => 'Ä',
        ('"', 'E') => 'Ë',
        ('"', 'I') => 'Ï',
        ('"', 'O') => 'Ö',
        ('"', 'U') => 'Ü',
        // Tilde
        ('~', 'a') => 'ã',
        ('~', 'n') => 'ñ',
        ('~', 'o') => 'õ',
        ('~', 'A') => 'Ã',
        ('~', 'N') => 'Ñ',
        ('~', 'O') => 'Õ',
        // Cedilla
        (',', 'c') => 'ç',
        (',', 'C') => 'Ç',
        // Ring
        ('o', 'a') => 'å',
        ('o', 'A') => 'Å',
        // Slash
        ('/', 'o') => 'ø',
        ('/', 'O') => 'Ø',
        // Ligatures
        ('a', 'e') => 'æ',
        ('A', 'E') => 'Æ',
        ('o', 'e') => 'œ',
        ('O', 'E') => 'Œ',
        // Currency
        ('c', '|') | ('c', '/') => '¢',
        ('l', '-') | ('L', '-') => '£',
        ('e', '=') | ('E', '=') => '€',
        ('y', '=') | ('Y', '=') => '¥',
        // Punctuation
        ('<', '<') => '«',
        ('>', '>') => '»',
        ('!', '!') => '¡',
        ('?', '?') => '¿',
        // Math
        ('+', '-') | ('-', '+') => '±',
        ('x', 'x') => '×',
        ('-', ':') => '÷',
        ('.', '.') => '…',
        ('1', '2') => '½',
        ('1', '4') => '¼',
        ('3', '4') => '¾',
        // Misc
        ('o', 'c') | ('O', 'C') => '©',
        ('o', 'r') | ('O', 'R') => '®',
        ('t', 'm') | ('T', 'M') => '™',
        ('s', 's') => 'ß',
        // If no match, return second character
        _ => b,
    }
}

lazy_static::lazy_static! {
    /// Global compose state
    static ref COMPOSE_STATE: Mutex<ComposeState> = Mutex::new(ComposeState::new());
}

/// Initialize the scancode queue early (called during boot, before interrupts)
pub fn init_scancode_queue() {
    SCANCODE_QUEUE
        .try_init_once(|| ArrayQueue::new(100))
        .expect("Scancode queue already initialized");
}

/// Called by the keyboard interrupt handler
pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if queue.push(scancode).is_err() {
            serial_println!("[KnoxOS] WARNING: scancode queue full; dropping input");
        }
    } else {
        serial_println!("[KnoxOS] WARNING: scancode queue uninitialized");
    }
}

/// Async stream of scancodes
pub struct ScancodeStream {
    _private: (),
}

impl Default for ScancodeStream {
    fn default() -> Self {
        Self::new()
    }
}

impl ScancodeStream {
    pub fn new() -> Self {
        // Queue may already be initialized by init_scancode_queue()
        let _ = SCANCODE_QUEUE.try_init_once(|| ArrayQueue::new(100));
        ScancodeStream { _private: () }
    }
}

impl ScancodeStream {
    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE.try_get().expect("not initialized");
        // Always return Ready - None means no data available
        // This lets the caller decide whether to yield
        Poll::Ready(queue.pop())
    }
}

/// Process keyboard input - main keyboard task
pub async fn process_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = PS2Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::MapLettersToUnicode,
    );

    // Track modifier state
    let mut ctrl_held = false;
    let mut shift_held = false;
    let mut alt_held = false;
    let mut super_held = false;

    serial_println!("[KnoxOS] Keyboard handler started");

    loop {
        // Drain all available scancodes
        loop {
            let scancode = core::future::poll_fn(|cx| Pin::new(&mut scancodes).poll_next(cx)).await;

            match scancode {
                Some(scancode) => {
                    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                        // Track modifier key state from the raw event
                        let is_press = key_event.state == pc_keyboard::KeyState::Down;
                        match key_event.code {
                            KeyCode::LControl | KeyCode::RControl => {
                                ctrl_held = is_press;
                                CTRL_HELD_GLOBAL.store(is_press, Ordering::Relaxed);
                            }
                            KeyCode::LShift | KeyCode::RShift => {
                                shift_held = is_press;
                            }
                            KeyCode::LAlt | KeyCode::RAltGr => {
                                alt_held = is_press;
                                // Alt released while Alt+Tab overlay is visible → confirm selection
                                if !is_press && crate::gui::alt_tab::is_visible() {
                                    if let Some(wid) = crate::gui::alt_tab::confirm() {
                                        let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
                                        wm.focus_window(wid);
                                        drop(wm);
                                        crate::gui::taskbar::set_active(wid);
                                    }
                                    crate::gui::request_redraw();
                                }
                            }
                            KeyCode::LWin | KeyCode::RWin => {
                                super_held = is_press;
                            }
                            _ => {}
                        }

                        // ── Login screen key routing ──
                        // If not logged in, route all keys to the login screen
                        if !crate::gui::login::is_logged_in() {
                            if is_press {
                                match key_event.code {
                                    KeyCode::Backspace => crate::gui::login::handle_backspace(),
                                    KeyCode::Tab => crate::gui::login::handle_tab(),
                                    KeyCode::Return | KeyCode::NumpadEnter => {
                                        crate::gui::login::handle_enter()
                                    }
                                    KeyCode::Escape => crate::gui::login::handle_escape(),
                                    _ => {
                                        // Process to get unicode char
                                        if let Some(DecodedKey::Unicode(ch)) =
                                            keyboard.process_keyevent(key_event)
                                        {
                                            if ch >= ' ' && ch != '\x7f' {
                                                crate::gui::login::handle_char(ch);
                                            }
                                        }
                                        continue;
                                    }
                                }
                            }
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // ── Lock screen key routing ──
                        if crate::gui::lock_screen::is_locked() {
                            if is_press {
                                match key_event.code {
                                    KeyCode::Backspace => {
                                        crate::gui::lock_screen::handle_backspace()
                                    }
                                    KeyCode::Return | KeyCode::NumpadEnter => {
                                        crate::gui::lock_screen::handle_enter()
                                    }
                                    KeyCode::Escape => crate::gui::lock_screen::handle_escape(),
                                    _ => {
                                        if let Some(DecodedKey::Unicode(ch)) =
                                            keyboard.process_keyevent(key_event)
                                        {
                                            if ch >= ' ' && ch != '\x7f' {
                                                crate::gui::lock_screen::handle_char(ch);
                                            }
                                        }
                                        continue;
                                    }
                                }
                            }
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // ── P8.1: VT switching (Ctrl+Alt+F1-F6) ──
                        if is_press && ctrl_held && alt_held {
                            let vt = match key_event.code {
                                KeyCode::F1 => Some(0u32),
                                KeyCode::F2 => Some(1),
                                KeyCode::F3 => Some(2),
                                KeyCode::F4 => Some(3),
                                KeyCode::F5 => Some(4),
                                KeyCode::F6 => Some(5),
                                _ => None,
                            };
                            if let Some(vt_num) = vt {
                                crate::tty::switch_console(vt_num);
                                crate::gui::request_redraw();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }
                        }

                        // Global keyboard shortcuts (work regardless of focus)
                        if is_press && ctrl_held && alt_held && key_event.code == KeyCode::T {
                            // Ctrl+Alt+T: Open terminal
                            crate::gui::desktop::open_application(
                                "Terminal",
                                crate::gui::desktop::IconType::Terminal,
                            );
                            crate::gui::request_redraw();
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // Ctrl+Alt+D: Show desktop (toggle minimize/restore all windows)
                        if is_press && ctrl_held && alt_held && key_event.code == KeyCode::D {
                            let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
                            wm.toggle_show_desktop();
                            crate::gui::request_redraw();
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // Ctrl+Alt+H: Toggle high contrast mode (17.7)
                        if is_press && ctrl_held && alt_held && key_event.code == KeyCode::H {
                            crate::gui::accessibility::toggle_high_contrast();
                            crate::gui::request_redraw();
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // ── Virtual workspace switching (Super+1/2/3/4) ──
                        if is_press && super_held {
                            let ws = match key_event.code {
                                KeyCode::Key1 => Some(0u8),
                                KeyCode::Key2 => Some(1u8),
                                KeyCode::Key3 => Some(2u8),
                                KeyCode::Key4 => Some(3u8),
                                _ => None,
                            };
                            if let Some(ws_num) = ws {
                                let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
                                if shift_held {
                                    // Super+Shift+N: Move focused window to workspace N
                                    wm.move_focused_to_workspace(ws_num, false);
                                } else {
                                    // Super+N: Switch to workspace N
                                    wm.switch_workspace(ws_num);
                                }
                                drop(wm);
                                crate::gui::request_redraw();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }

                            // Super+L: Lock screen
                            if key_event.code == KeyCode::L {
                                crate::gui::lock_screen::lock();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }

                            // Super+P: Toggle PIP (Picture-in-Picture) mode on focused window
                            if key_event.code == KeyCode::P {
                                let (sw, sh) = crate::gui::screen_size();
                                let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
                                if let Some(wid) = wm.focused_window {
                                    wm.toggle_pip(wid, sw as u32, sh as u32);
                                }
                                drop(wm);
                                crate::gui::request_redraw();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }

                            // Super+E: Toggle Exposé / Mission Control
                            if key_event.code == KeyCode::E {
                                crate::gui::expose::toggle();
                                crate::gui::request_redraw();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }
                        }

                        // Alt+Tab: Show/cycle Alt+Tab window switcher overlay
                        if is_press && alt_held && key_event.code == KeyCode::Tab {
                            if crate::gui::alt_tab::is_visible() {
                                // Already visible: cycle to next/previous
                                crate::gui::alt_tab::cycle(shift_held);
                            } else {
                                // Show the overlay
                                crate::gui::alt_tab::show(shift_held);
                            }
                            crate::gui::request_redraw();
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // Ctrl+Shift+Escape: Open Task Manager
                        if is_press && ctrl_held && shift_held && key_event.code == KeyCode::Escape
                        {
                            crate::gui::task_manager::open();
                            crate::gui::request_redraw();
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // Ctrl+W: Close focused window (alternative to Alt+F4)
                        // BUT skip for Terminal (Ctrl+W = delete word) and Browser (Ctrl+W = close tab)
                        if is_press && ctrl_held && !alt_held && key_event.code == KeyCode::W {
                            let wm = crate::gui::window::WINDOW_MANAGER.lock();
                            if let Some(wid) = wm.focused_window {
                                let ct = wm
                                    .windows
                                    .iter()
                                    .find(|w| w.id == wid)
                                    .map(|w| w.content_type);
                                let is_terminal =
                                    ct == Some(crate::gui::window::WindowContentType::Terminal);
                                let is_browser =
                                    ct == Some(crate::gui::window::WindowContentType::Browser);
                                drop(wm);
                                // Terminal: Ctrl+W = delete previous word; Browser: Ctrl+W = close tab
                                // Let these windows handle the key themselves instead of closing
                                if !is_terminal && !is_browser {
                                    crate::gui::window::WINDOW_MANAGER.lock().close_window(wid);
                                    crate::gui::taskbar::remove_entry(wid);
                                    crate::gui::request_redraw();
                                    let _ = keyboard.process_keyevent(key_event);
                                    continue;
                                }
                                // Fall through to normal key dispatch for terminal/browser
                            } else {
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }
                        }

                        // Ctrl+/: Toggle keyboard shortcuts overlay
                        if is_press && ctrl_held && !alt_held && key_event.code == KeyCode::Oem2 {
                            crate::gui::shortcuts_overlay::toggle();
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // Alt+F4: Close focused window
                        if is_press && alt_held && key_event.code == KeyCode::F4 {
                            let wm = crate::gui::window::WINDOW_MANAGER.lock();
                            if let Some(wid) = wm.focused_window {
                                let ct = wm
                                    .windows
                                    .iter()
                                    .find(|w| w.id == wid)
                                    .map(|w| w.content_type);
                                let is_terminal =
                                    ct == Some(crate::gui::window::WindowContentType::Terminal);
                                let is_browser =
                                    ct == Some(crate::gui::window::WindowContentType::Browser);
                                drop(wm);
                                crate::gui::window::WINDOW_MANAGER.lock().close_window(wid);
                                crate::gui::taskbar::remove_entry(wid);
                                if is_terminal {
                                    crate::terminal::destroy_for_window(wid);
                                }
                                if is_browser {
                                    crate::gui::browser::destroy_for_window(wid);
                                }
                                crate::gui::request_redraw();
                            }
                            let _ = keyboard.process_keyevent(key_event);
                            continue;
                        }

                        // Escape: Cancel drag/resize operations, close menus
                        if is_press && key_event.code == KeyCode::Escape {
                            // Close Exposé first
                            if crate::gui::expose::is_visible() {
                                crate::gui::expose::dismiss();
                                crate::gui::request_redraw();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }
                            // Close shortcuts overlay first
                            if crate::gui::shortcuts_overlay::is_visible() {
                                crate::gui::shortcuts_overlay::hide();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }
                            // Close file picker
                            if crate::gui::file_picker::is_visible() {
                                crate::gui::file_picker::close();
                                crate::gui::request_redraw();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }

                            let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
                            let was_interacting = wm.any_dragging() || wm.any_resizing();
                            wm.cancel_all_interactions();
                            drop(wm);

                            // Also close start menu, context menu, popups
                            crate::gui::startmenu::close();
                            crate::gui::desktop::close_context_menu();
                            crate::gui::popups::close_all_popups();
                            crate::gui::drag_and_drop::cancel();

                            if was_interacting {
                                crate::gui::request_redraw();
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }
                        }

                        // Super+Arrow keys: window snapping & management
                        if is_press && ctrl_held && !alt_held {
                            let (sw, sh) = crate::gui::screen_size();
                            let handled = match key_event.code {
                                KeyCode::ArrowLeft if shift_held => {
                                    // Ctrl+Shift+Left: Snap focused window left
                                    let wm = crate::gui::window::WINDOW_MANAGER.lock();
                                    if let Some(wid) = wm.focused_window {
                                        drop(wm);
                                        crate::gui::window::WINDOW_MANAGER
                                            .lock()
                                            .snap_left(wid, sw as u32, sh as u32);
                                        crate::gui::request_redraw();
                                    }
                                    true
                                }
                                KeyCode::ArrowRight if shift_held => {
                                    // Ctrl+Shift+Right: Snap focused window right
                                    let wm = crate::gui::window::WINDOW_MANAGER.lock();
                                    if let Some(wid) = wm.focused_window {
                                        drop(wm);
                                        crate::gui::window::WINDOW_MANAGER
                                            .lock()
                                            .snap_right(wid, sw as u32, sh as u32);
                                        crate::gui::request_redraw();
                                    }
                                    true
                                }
                                KeyCode::ArrowUp if shift_held => {
                                    // Ctrl+Shift+Up: Maximize focused window
                                    let wm = crate::gui::window::WINDOW_MANAGER.lock();
                                    if let Some(wid) = wm.focused_window {
                                        let state = wm
                                            .windows
                                            .iter()
                                            .find(|w| w.id == wid)
                                            .map(|w| w.state);
                                        drop(wm);
                                        if state != Some(crate::gui::window::WindowState::Maximized)
                                        {
                                            crate::gui::window::WINDOW_MANAGER
                                                .lock()
                                                .toggle_maximize(wid, sw as u32, sh as u32);
                                            crate::gui::request_redraw();
                                        }
                                    }
                                    true
                                }
                                KeyCode::ArrowDown if shift_held => {
                                    // Ctrl+Shift+Down: Restore/minimize
                                    let wm = crate::gui::window::WINDOW_MANAGER.lock();
                                    if let Some(wid) = wm.focused_window {
                                        let state = wm
                                            .windows
                                            .iter()
                                            .find(|w| w.id == wid)
                                            .map(|w| w.state);
                                        drop(wm);
                                        match state {
                                            Some(crate::gui::window::WindowState::Maximized)
                                            | Some(crate::gui::window::WindowState::SnappedLeft)
                                            | Some(crate::gui::window::WindowState::SnappedRight)
                                            | Some(
                                                crate::gui::window::WindowState::SnappedTopLeft,
                                            )
                                            | Some(
                                                crate::gui::window::WindowState::SnappedTopRight,
                                            )
                                            | Some(
                                                crate::gui::window::WindowState::SnappedBottomLeft,
                                            )
                                            | Some(
                                                crate::gui::window::WindowState::SnappedBottomRight,
                                            ) => {
                                                crate::gui::window::WINDOW_MANAGER
                                                    .lock()
                                                    .toggle_maximize(wid, sw as u32, sh as u32);
                                            }
                                            _ => {
                                                crate::gui::window::WINDOW_MANAGER
                                                    .lock()
                                                    .minimize_window(wid);
                                            }
                                        }
                                        crate::gui::request_redraw();
                                    }
                                    true
                                }
                                _ => false,
                            };
                            if handled {
                                let _ = keyboard.process_keyevent(key_event);
                                continue;
                            }
                        }

                        // Check for special keys (arrows, home, end, etc.)
                        // that need handling before process_keyevent
                        let mut handled = false;
                        if is_press {
                            // ── Route to Command Center (start menu) first ──
                            if crate::gui::startmenu::is_visible() {
                                let sc = scancode;
                                if crate::gui::startmenu::handle_key(sc, None) {
                                    handled = true;
                                    let _ = keyboard.process_keyevent(key_event);
                                    crate::gui::request_redraw();
                                    continue;
                                }
                            }

                            let terminal_key = match key_event.code {
                                KeyCode::ArrowLeft if ctrl_held => Some(TerminalKey::CtrlLeft),
                                KeyCode::ArrowRight if ctrl_held => Some(TerminalKey::CtrlRight),
                                KeyCode::ArrowLeft => Some(TerminalKey::Left),
                                KeyCode::ArrowRight => Some(TerminalKey::Right),
                                KeyCode::ArrowUp => Some(TerminalKey::Up),
                                KeyCode::ArrowDown => Some(TerminalKey::Down),
                                KeyCode::Home => Some(TerminalKey::Home),
                                KeyCode::End => Some(TerminalKey::End),
                                KeyCode::Delete => Some(TerminalKey::Delete),
                                KeyCode::PageUp if shift_held => Some(TerminalKey::ShiftPgUp),
                                KeyCode::PageDown if shift_held => Some(TerminalKey::ShiftPgDown),
                                _ => None,
                            };

                            if let Some(tkey) = terminal_key {
                                // Route to terminal if focused
                                if focused_terminal_window_id().is_some() {
                                    handle_terminal_key(tkey);
                                    handled = true;
                                }
                                // Route to browser if focused
                                else if let Some(bwid) = focused_browser_window_id() {
                                    let browser_key = match tkey {
                                        TerminalKey::Left => {
                                            if alt_held {
                                                Some(crate::gui::browser::BrowserKey::AltLeft)
                                            } else {
                                                Some(crate::gui::browser::BrowserKey::Left)
                                            }
                                        }
                                        TerminalKey::Right => {
                                            if alt_held {
                                                Some(crate::gui::browser::BrowserKey::AltRight)
                                            } else {
                                                Some(crate::gui::browser::BrowserKey::Right)
                                            }
                                        }
                                        TerminalKey::Home => {
                                            Some(crate::gui::browser::BrowserKey::Home)
                                        }
                                        TerminalKey::End => {
                                            Some(crate::gui::browser::BrowserKey::End)
                                        }
                                        TerminalKey::Delete => {
                                            Some(crate::gui::browser::BrowserKey::Delete)
                                        }
                                        _ => None,
                                    };
                                    if let Some(bk) = browser_key {
                                        crate::gui::browser::handle_browser_key(bwid, bk);
                                        crate::gui::request_redraw();
                                        handled = true;
                                    }
                                }
                                // Route to file explorer if focused
                                else if let Some(ewid) = focused_explorer_window_id() {
                                    use crate::gui::event_types::KeyCode as EK;
                                    let explorer_key = match key_event.code {
                                        KeyCode::ArrowUp => Some(EK::ArrowUp),
                                        KeyCode::ArrowDown => Some(EK::ArrowDown),
                                        KeyCode::Delete => Some(EK::Delete),
                                        KeyCode::F2 => Some(EK::F2),
                                        _ => None,
                                    };
                                    if let Some(ek) = explorer_key {
                                        if crate::gui::explorer::handle_key(
                                            ewid, ek, ctrl_held, shift_held,
                                        ) {
                                            crate::gui::request_redraw();
                                            handled = true;
                                        }
                                    }
                                }
                                // Route to text editor if focused
                                else if let Some(ed_wid) = focused_editor_window_id() {
                                    use crate::gui::event_types::KeyCode as EK;
                                    if key_event.code == KeyCode::Delete {
                                        crate::gui::editor::handle_delete(ed_wid);
                                        handled = true;
                                    } else {
                                        let editor_key = match key_event.code {
                                            KeyCode::ArrowUp => Some(EK::ArrowUp),
                                            KeyCode::ArrowDown => Some(EK::ArrowDown),
                                            KeyCode::ArrowLeft => Some(EK::ArrowLeft),
                                            KeyCode::ArrowRight => Some(EK::ArrowRight),
                                            KeyCode::Home => Some(EK::Home),
                                            KeyCode::End => Some(EK::End),
                                            KeyCode::PageUp => Some(EK::PageUp),
                                            KeyCode::PageDown => Some(EK::PageDown),
                                            _ => None,
                                        };
                                        if let Some(ek) = editor_key {
                                            crate::gui::editor::handle_arrow(ed_wid, ek);
                                            handled = true;
                                        }
                                    }
                                }
                            }
                            // Route arrow keys to settings if focused
                            else if focused_settings_window_id().is_some() {
                                let scancode_byte = scancode;
                                if crate::gui::settings::handle_settings_key(
                                    scancode_byte,
                                    None,
                                    shift_held,
                                ) {
                                    crate::gui::request_redraw();
                                    handled = true;
                                }
                            }
                            // Route arrow keys to AI assistant if focused
                            else if let Some(ai_wid) =
                                crate::gui::ai_assistant::focused_ai_window_id()
                            {
                                match key_event.code {
                                    KeyCode::ArrowLeft => {
                                        crate::gui::ai_assistant::handle_arrow_left(ai_wid);
                                        handled = true;
                                    }
                                    KeyCode::ArrowRight => {
                                        crate::gui::ai_assistant::handle_arrow_right(ai_wid);
                                        handled = true;
                                    }
                                    KeyCode::Home => {
                                        crate::gui::ai_assistant::handle_home(ai_wid);
                                        handled = true;
                                    }
                                    KeyCode::End => {
                                        crate::gui::ai_assistant::handle_end(ai_wid);
                                        handled = true;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // Always call process_keyevent to maintain keyboard state
                        if let Some(key) = keyboard.process_keyevent(key_event) {
                            if !handled {
                                match key {
                                    DecodedKey::Unicode(character) => {
                                        // Route character input to file picker when visible (Save mode)
                                        if crate::gui::file_picker::is_visible() {
                                            crate::gui::file_picker::handle_key(character);
                                            crate::gui::request_redraw();
                                        } else {
                                            handle_key_press(character, ctrl_held, shift_held);
                                        }
                                    }
                                    DecodedKey::RawKey(key) => {
                                        // Handle raw keys not caught above
                                        let terminal_key = match key {
                                            KeyCode::ArrowLeft => Some(TerminalKey::Left),
                                            KeyCode::ArrowRight => Some(TerminalKey::Right),
                                            KeyCode::ArrowUp => Some(TerminalKey::Up),
                                            KeyCode::ArrowDown => Some(TerminalKey::Down),
                                            KeyCode::Home => Some(TerminalKey::Home),
                                            KeyCode::End => Some(TerminalKey::End),
                                            KeyCode::Delete => Some(TerminalKey::Delete),
                                            KeyCode::PageUp => {
                                                if shift_held {
                                                    Some(TerminalKey::ShiftPgUp)
                                                } else {
                                                    None
                                                }
                                            }
                                            KeyCode::PageDown => {
                                                if shift_held {
                                                    Some(TerminalKey::ShiftPgDown)
                                                } else {
                                                    None
                                                }
                                            }
                                            _ => None,
                                        };
                                        if let Some(tkey) = terminal_key {
                                            handle_terminal_key(tkey);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                None => break, // No more scancodes, yield
            }
        }
        // Yield until next poll
        crate::yield_once().await;
    }
}

/// Check if a terminal window is currently focused, returning its window ID
fn focused_terminal_window_id() -> Option<u32> {
    let wm = crate::gui::window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == crate::gui::window::WindowContentType::Terminal
                && win.state != crate::gui::window::WindowState::Minimized
            {
                return Some(wid);
            }
        }
    }
    None
}

/// Check if a browser window is currently focused, returning its window ID
fn focused_browser_window_id() -> Option<u32> {
    let wm = crate::gui::window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == crate::gui::window::WindowContentType::Browser
                && win.state != crate::gui::window::WindowState::Minimized
            {
                return Some(wid);
            }
        }
    }
    None
}

/// Check if a file explorer window is currently focused, returning its window ID
fn focused_explorer_window_id() -> Option<u32> {
    let wm = crate::gui::window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == crate::gui::window::WindowContentType::FileExplorer
                && win.state != crate::gui::window::WindowState::Minimized
            {
                return Some(wid);
            }
        }
    }
    None
}

/// Check if a text editor window is currently focused, returning its window ID
fn focused_editor_window_id() -> Option<u32> {
    let wm = crate::gui::window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == crate::gui::window::WindowContentType::TextEditor
                && win.state != crate::gui::window::WindowState::Minimized
            {
                return Some(wid);
            }
        }
    }
    None
}

/// Check if a settings window is currently focused, returning its window ID
fn focused_settings_window_id() -> Option<u32> {
    let wm = crate::gui::window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == crate::gui::window::WindowContentType::Settings
                && win.state != crate::gui::window::WindowState::Minimized
            {
                return Some(wid);
            }
        }
    }
    None
}

/// Check if a terminal window is currently focused
fn is_terminal_focused() -> bool {
    focused_terminal_window_id().is_some()
}

/// Send a terminal key event to the active terminal (accounting for tabs)
fn handle_terminal_key(key: TerminalKey) {
    if let Some(wid) = focused_terminal_window_id() {
        let active_term_id = {
            let wm = crate::gui::window::WINDOW_MANAGER.lock();
            wm.windows
                .iter()
                .find(|w| w.id == wid)
                .and_then(|w| {
                    if w.terminal_tabs.is_empty() {
                        Some(wid)
                    } else {
                        w.terminal_tabs.get(w.terminal_active_tab).copied()
                    }
                })
                .unwrap_or(wid)
        };
        crate::terminal::handle_key_for_window(active_term_id, key);
        crate::gui::request_redraw();
    }
}

/// Handle a decoded key press
fn handle_key_press(character: char, ctrl_held: bool, shift_held: bool) {
    // P8.5: Apply keyboard layout remapping
    let character = if !ctrl_held {
        CURRENT_LAYOUT.lock().remap(character, shift_held)
    } else {
        character
    };

    // P8.6: Compose key handling — Right Alt triggers compose mode
    // Check if compose sequence is active
    {
        let mut compose = COMPOSE_STATE.lock();
        if compose.active {
            if character == '\x1B' {
                // Escape cancels compose
                compose.cancel();
                return;
            }
            if let Some(composed) = compose.feed(character) {
                // Got a composed character — send it to the terminal
                drop(compose);
                if let Some(wid) = focused_terminal_window_id() {
                    crate::terminal::handle_key_for_window(wid, TerminalKey::Char(composed));
                    crate::gui::request_redraw();
                }
                return;
            }
            return; // still waiting for second char
        }
    }

    // ── Global zoom shortcuts (Ctrl+=/Ctrl+-/Ctrl+0) (17.9) ──
    if ctrl_held {
        match character {
            '=' | '+' => {
                crate::gui::accessibility::zoom_in();
                crate::gui::request_redraw();
                return;
            }
            '-' | '_' => {
                crate::gui::accessibility::zoom_out();
                crate::gui::request_redraw();
                return;
            }
            '0' => {
                crate::gui::accessibility::zoom_reset();
                crate::gui::request_redraw();
                return;
            }
            _ => {}
        }
    }

    // ── Route to Command Center when visible ──
    if crate::gui::startmenu::is_visible() {
        // Map common characters to scancodes for handle_key
        let scancode = match character {
            '\x1B' => Some(0x01u8),          // Escape
            '\t' => Some(0x0Fu8),            // Tab
            '\n' | '\r' => Some(0x1Cu8),     // Enter
            '\x08' | '\x7F' => Some(0x0Eu8), // Backspace
            _ => None,
        };
        let ch = if !character.is_control() {
            Some(character)
        } else {
            None
        };
        let sc = scancode.unwrap_or(0);
        if crate::gui::startmenu::handle_key(sc, ch) {
            crate::gui::request_redraw();
            return;
        }
    }

    // Check if a terminal window is focused
    if let Some(wid) = focused_terminal_window_id() {
        // ── Terminal tab shortcuts (intercept before sending to terminal) ──
        if ctrl_held {
            match character {
                // Ctrl+T — New terminal tab
                't' | '\x14' => {
                    let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        let tab_idx = win.terminal_tabs.len();
                        let tab_id = crate::terminal::create_tab(wid, tab_idx);
                        win.terminal_tabs.push(tab_id);
                        win.terminal_active_tab = tab_idx;
                    }
                    drop(wm);
                    crate::gui::request_redraw();
                    return;
                }
                // Ctrl+Shift+W — Close current terminal tab (only if >1 tab)
                _ if (character == 'w' || character == '\x17') && shift_held => {
                    let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        if win.terminal_tabs.len() > 1 {
                            let idx = win.terminal_active_tab;
                            let tab_id = win.terminal_tabs[idx];
                            win.terminal_tabs.remove(idx);
                            if win.terminal_active_tab >= win.terminal_tabs.len() {
                                win.terminal_active_tab = win.terminal_tabs.len() - 1;
                            }
                            drop(wm);
                            crate::terminal::destroy_tab(tab_id);
                        }
                    }
                    crate::gui::request_redraw();
                    return;
                }
                // Ctrl+Shift+| (backslash) — Horizontal split pane (10.12)
                _ if character == '|' && shift_held => {
                    let mut pm = crate::terminal::split_pane::PANE_MANAGER.lock();
                    if let Some(new_id) =
                        pm.split(crate::terminal::split_pane::SplitDirection::Horizontal)
                    {
                        crate::serial_println!("[Terminal] Split horizontal, new pane {}", new_id);
                    }
                    crate::gui::request_redraw();
                    return;
                }
                // Ctrl+Shift+_ (minus) — Vertical split pane (10.12)
                _ if character == '_' && shift_held => {
                    let mut pm = crate::terminal::split_pane::PANE_MANAGER.lock();
                    if let Some(new_id) =
                        pm.split(crate::terminal::split_pane::SplitDirection::Vertical)
                    {
                        crate::serial_println!("[Terminal] Split vertical, new pane {}", new_id);
                    }
                    crate::gui::request_redraw();
                    return;
                }
                // Ctrl+Shift+N — Focus next pane (10.12)
                _ if (character == 'n' || character == '\x0E') && shift_held => {
                    let mut pm = crate::terminal::split_pane::PANE_MANAGER.lock();
                    pm.focus_next();
                    crate::serial_println!("[Terminal] Focus next pane");
                    crate::gui::request_redraw();
                    return;
                }
                // Ctrl+Shift+Q — Close current pane (10.12)
                _ if (character == 'q' || character == '\x11') && shift_held => {
                    let pm = crate::terminal::split_pane::PANE_MANAGER.lock();
                    if let Some(focused) = pm.focused_pane_id {
                        if pm.pane_count() > 1 {
                            drop(pm);
                            let mut pm2 = crate::terminal::split_pane::PANE_MANAGER.lock();
                            pm2.close_pane(focused);
                            crate::serial_println!("[Terminal] Closed pane {}", focused);
                        }
                    }
                    crate::gui::request_redraw();
                    return;
                }
                _ => {}
            }
        }
        // Ctrl+Tab — Next tab, Ctrl+Shift+Tab — Previous tab
        if ctrl_held && character == '\t' {
            let mut wm = crate::gui::window::WINDOW_MANAGER.lock();
            if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                if win.terminal_tabs.len() > 1 {
                    if shift_held {
                        if win.terminal_active_tab == 0 {
                            win.terminal_active_tab = win.terminal_tabs.len() - 1;
                        } else {
                            win.terminal_active_tab -= 1;
                        }
                    } else {
                        win.terminal_active_tab =
                            (win.terminal_active_tab + 1) % win.terminal_tabs.len();
                    }
                }
            }
            drop(wm);
            crate::gui::request_redraw();
            return;
        }

        // Route to the specific terminal instance (active tab)
        let active_term_id = {
            let wm = crate::gui::window::WINDOW_MANAGER.lock();
            wm.windows
                .iter()
                .find(|w| w.id == wid)
                .and_then(|w| {
                    if w.terminal_tabs.is_empty() {
                        Some(wid)
                    } else {
                        w.terminal_tabs.get(w.terminal_active_tab).copied()
                    }
                })
                .unwrap_or(wid)
        };

        let terminal_key = if ctrl_held {
            match character {
                'a' | '\x01' => Some(TerminalKey::CtrlA),
                'c' | '\x03' => Some(TerminalKey::CtrlC),
                'd' | '\x04' => Some(TerminalKey::CtrlD),
                'e' | '\x05' => Some(TerminalKey::CtrlE),
                'k' | '\x0B' => Some(TerminalKey::CtrlK),
                'l' | '\x0C' => Some(TerminalKey::CtrlL),
                'r' | '\x12' => Some(TerminalKey::CtrlR),
                'u' | '\x15' => Some(TerminalKey::CtrlU),
                'w' | '\x17' => Some(TerminalKey::CtrlW),
                '.' => {
                    // Ctrl+. triggers compose mode (P8.6)
                    COMPOSE_STATE.lock().begin();
                    return;
                }
                _ => None,
            }
        } else {
            match character {
                '\n' | '\r' => Some(TerminalKey::Enter),
                '\x08' => Some(TerminalKey::Backspace),
                '\x7F' => Some(TerminalKey::Backspace),
                '\x1B' => Some(TerminalKey::Escape),
                '\t' => Some(TerminalKey::Tab),
                ch if !ch.is_control() => Some(TerminalKey::Char(ch)),
                _ => None,
            }
        };

        if let Some(key) = terminal_key {
            crate::terminal::handle_key_for_window(active_term_id, key);
            crate::gui::request_redraw();
        }
        return;
    }

    // Check if a browser window is focused
    if let Some(bwid) = focused_browser_window_id() {
        use crate::gui::browser::BrowserKey;
        let browser_key = if ctrl_held {
            match character {
                'l' | '\x0C' => Some(BrowserKey::CtrlL),
                'r' | '\x12' => Some(BrowserKey::CtrlR),
                't' | '\x14' => Some(BrowserKey::CtrlT),
                'd' | '\x04' => Some(BrowserKey::CtrlD),
                _ => None,
            }
        } else {
            match character {
                '\n' | '\r' => Some(BrowserKey::Enter),
                '\x08' | '\x7F' => Some(BrowserKey::Backspace),
                '\x1B' => Some(BrowserKey::Escape),
                '\t' => Some(BrowserKey::Tab),
                ch if !ch.is_control() => Some(BrowserKey::Char(ch)),
                _ => None,
            }
        };

        if let Some(bk) = browser_key {
            crate::gui::browser::handle_browser_key(bwid, bk);
            crate::gui::request_redraw();
        }
        return;
    }

    // Check if a file explorer window is focused
    if let Some(ewid) = focused_explorer_window_id() {
        use crate::gui::event_types::KeyCode as EK;
        let explorer_key = if ctrl_held {
            match character {
                'c' | '\x03' => Some(EK::C),
                'x' | '\x18' => Some(EK::X),
                'v' | '\x16' => Some(EK::V),
                'h' | '\x08' => Some(EK::H),
                'l' | '\x0C' => Some(EK::L),
                'f' | '\x06' => Some(EK::F),
                'p' | '\x10' => Some(EK::P),
                'g' | '\x07' => Some(EK::G),
                _ => None,
            }
        } else {
            match character {
                '\n' | '\r' => Some(EK::Enter),
                '\x08' | '\x7F' => Some(EK::Backspace),
                '\x1B' => Some(EK::Escape),
                ch if !ch.is_control() => {
                    // Route printable chars to address bar / rename / search
                    let wm = crate::gui::window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter().find(|w| w.id == ewid) {
                        if win.explorer_editing_path {
                            drop(wm);
                            crate::gui::explorer::address_bar_char(ewid, ch);
                            crate::gui::request_redraw();
                            return;
                        } else if win.explorer_renaming.is_some() {
                            drop(wm);
                            crate::gui::explorer::rename_char(ewid, ch);
                            crate::gui::request_redraw();
                            return;
                        } else if win.explorer_search_active {
                            drop(wm);
                            crate::gui::explorer::search_char(ewid, ch);
                            crate::gui::request_redraw();
                            return;
                        }
                    }
                    None
                }
                _ => None,
            }
        };
        if let Some(ek) = explorer_key {
            if crate::gui::explorer::handle_key(ewid, ek, ctrl_held, shift_held) {
                crate::gui::request_redraw();
            }
            return;
        }
    }

    // Check if a text editor window is focused
    if let Some(ed_wid) = focused_editor_window_id() {
        // Check if editor is in search/replace mode
        let in_search = crate::gui::editor::is_in_search_mode(ed_wid);

        if in_search {
            // ── Search/Replace mode key routing ──
            match character {
                '\n' | '\r' => {
                    crate::gui::editor::handle_search_enter(ed_wid);
                    return;
                }
                '\x08' | '\x7F' => {
                    crate::gui::editor::handle_search_backspace(ed_wid);
                    return;
                }
                '\x1B' => {
                    crate::gui::editor::handle_search_escape(ed_wid);
                    return;
                }
                '\t' => {
                    crate::gui::editor::handle_search_tab(ed_wid);
                    return;
                }
                ch if ctrl_held && (ch == 'a' || ch == '\x01') => {
                    crate::gui::editor::handle_replace_all(ed_wid);
                    return;
                }
                ch if !ch.is_control() => {
                    crate::gui::editor::handle_search_char(ed_wid, ch);
                    return;
                }
                _ => {}
            }
            return;
        }

        if ctrl_held {
            match character {
                's' | '\x13' => {
                    crate::gui::editor::handle_save(ed_wid);
                    crate::gui::request_redraw();
                    return;
                }
                'z' | '\x1A' => {
                    crate::gui::editor::handle_undo(ed_wid);
                    return;
                }
                'y' | '\x19' => {
                    crate::gui::editor::handle_redo(ed_wid);
                    return;
                }
                'a' | '\x01' => {
                    crate::gui::editor::handle_select_all(ed_wid);
                    return;
                }
                'f' | '\x06' => {
                    crate::gui::editor::handle_find(ed_wid);
                    return;
                }
                'h' | '\x08' => {
                    crate::gui::editor::handle_find_replace(ed_wid);
                    return;
                }
                _ => {}
            }
        } else {
            match character {
                '\n' | '\r' => {
                    crate::gui::editor::handle_enter(ed_wid);
                    return;
                }
                '\x08' | '\x7F' => {
                    crate::gui::editor::handle_backspace(ed_wid);
                    return;
                }
                '\t' => {
                    crate::gui::editor::handle_tab(ed_wid);
                    return;
                }
                '\x1B' => {
                    // Escape in editor — do nothing special for now
                    return;
                }
                ch if !ch.is_control() => {
                    crate::gui::editor::handle_char(ed_wid, ch);
                    return;
                }
                _ => {}
            }
        }
        return;
    }

    // Check if an AI assistant window is focused
    if let Some(ai_wid) = crate::gui::ai_assistant::focused_ai_window_id() {
        if ctrl_held {
            crate::gui::ai_assistant::handle_ctrl_key(ai_wid, character);
            return;
        }
        match character {
            '\n' | '\r' => {
                crate::gui::ai_assistant::handle_enter(ai_wid);
                return;
            }
            '\x08' | '\x7F' => {
                crate::gui::ai_assistant::handle_backspace(ai_wid);
                return;
            }
            '\x1B' => {
                crate::gui::ai_assistant::handle_escape(ai_wid);
                return;
            }
            ch if !ch.is_control() => {
                crate::gui::ai_assistant::handle_char(ai_wid, ch);
                return;
            }
            _ => {}
        }
        return;
    }

    // Check if a settings window is focused — route Tab/Enter/Space
    if focused_settings_window_id().is_some() {
        let ch = Some(character);
        match character {
            '\t' | '\n' | '\r' | ' '
                if crate::gui::settings::handle_settings_key(0, ch, shift_held) =>
            {
                crate::gui::request_redraw();
                return;
            }
            '\x1B' => {
                // Escape: disable keyboard nav, reset focus
                {
                    let mut state = crate::gui::settings::SETTINGS_STATE.lock();
                    state.keyboard_nav = false;
                    state.focus_index = -1;
                }
                crate::gui::request_redraw();
                return;
            }
            _ => {}
        }
    }

    // Desktop-level key handling (no terminal focused)
    match character {
        '\x1B' => {
            // Escape - close context menu, start menu, or deselect icons
            if crate::gui::desktop::CONTEXT_MENU.lock().visible {
                crate::gui::desktop::close_context_menu();
            } else if crate::gui::startmenu::is_visible() {
                crate::gui::startmenu::close();
            } else {
                // Deselect all icons
                let mut desktop = crate::gui::desktop::DESKTOP.lock();
                for icon in desktop.icons.iter_mut() {
                    icon.selected = false;
                }
                desktop.selected_icon = None;
            }
            crate::gui::request_redraw();
        }
        '\n' | '\r' => {
            // Enter - open selected icon or activate focused window
            let desktop = crate::gui::desktop::DESKTOP.lock();
            if let Some(idx) = desktop.selected_icon {
                let name = desktop.icons[idx].name.clone();
                let icon_type = desktop.icons[idx].icon_type;
                drop(desktop);
                crate::gui::desktop::open_application(&name, icon_type);
                crate::gui::request_redraw();
            }
        }
        '\t' => {
            // Tab - cycle through desktop icons
            let mut desktop = crate::gui::desktop::DESKTOP.lock();
            if desktop.icons.is_empty() {
                return;
            }
            let next = match desktop.selected_icon {
                Some(idx) => {
                    if shift_held {
                        if idx == 0 {
                            desktop.icons.len() - 1
                        } else {
                            idx - 1
                        }
                    } else {
                        (idx + 1) % desktop.icons.len()
                    }
                }
                None => 0,
            };
            for (i, icon) in desktop.icons.iter_mut().enumerate() {
                icon.selected = i == next;
            }
            desktop.selected_icon = Some(next);
            drop(desktop);
            crate::gui::request_redraw();
        }
        _ => {
            // Other keys on the desktop (no focused terminal)
        }
    }
}
